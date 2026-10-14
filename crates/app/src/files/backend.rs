//! Interface que a UI de arquivos usa para falar com o motor de transferência. Espelha o
//! contrato (`RemoteBrowser` + `TransferManager`); na integração, uma implementação fina sobre
//! `myssh_transfer` substitui [`MemBackend`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use myssh_ssh::Connection;

use super::contract::*;

pub type BoxFut<T> = Pin<Box<dyn Future<Output = Result<T, TransferError>> + Send>>;

/// Navegação remota (`RemoteBrowser`). Clonável por `Arc` para as tarefas em segundo plano.
pub trait RemoteFs: Send + Sync {
    fn home(&self) -> BoxFut<String>;
    /// Sem `.` e `..`.
    fn list(&self, dir: &str) -> BoxFut<Vec<RemoteEntry>>;
    fn mkdir(&self, path: &str) -> BoxFut<()>;
    fn rename(&self, from: &str, to: &str) -> BoxFut<()>;
    fn remove(&self, path: &str, recursive: bool) -> BoxFut<()>;
}

/// Navegação remota + fila (`TransferManager`).
pub trait FilesBackend: Send {
    fn remote(&self) -> Arc<dyn RemoteFs>;
    fn enqueue(&self, req: TransferRequest) -> TransferId;
    fn cancel(&self, id: TransferId);
    fn cancel_all(&self);
    /// Chamado pela UI a cada frame, com orçamento.
    fn try_recv(&mut self) -> Option<TransferEvent>;
    /// Transferências ainda não terminadas (para confirmar o fechamento).
    fn active(&self) -> usize;
}

// ---------------------------------------------------------------------------------------------

/// Navegação remota real (um `RemoteBrowser` com canal SFTP próprio).
struct RealRemote(Arc<RemoteBrowser>);

impl RemoteFs for RealRemote {
    fn home(&self) -> BoxFut<String> {
        let b = self.0.clone();
        Box::pin(async move { b.home().await })
    }

    fn list(&self, dir: &str) -> BoxFut<Vec<RemoteEntry>> {
        let (b, d) = (self.0.clone(), dir.to_string());
        Box::pin(async move { b.list(&d).await })
    }

    fn mkdir(&self, path: &str) -> BoxFut<()> {
        let (b, p) = (self.0.clone(), path.to_string());
        Box::pin(async move { b.mkdir(&p).await })
    }

    fn rename(&self, from: &str, to: &str) -> BoxFut<()> {
        let (b, f, t) = (self.0.clone(), from.to_string(), to.to_string());
        Box::pin(async move { b.rename(&f, &t).await })
    }

    fn remove(&self, path: &str, recursive: bool) -> BoxFut<()> {
        let (b, p) = (self.0.clone(), path.to_string());
        Box::pin(async move { b.remove(&p, recursive).await })
    }
}

/// Backend de produção: `RemoteBrowser` + `TransferManager` sobre a conexão da sessão.
pub struct RealBackend {
    remote: Arc<dyn RemoteFs>,
    mgr: TransferManager,
}

/// Arquivos simultâneos na fila (padrão do contrato).
pub const MAX_CONCURRENT: usize = 3;

impl RealBackend {
    /// Abre o canal SFTP do navegador (a fila abre os dela sob demanda).
    pub async fn connect(
        rt: &tokio::runtime::Handle,
        conn: Arc<Connection>,
        notify: Notify,
    ) -> Result<Self, TransferError> {
        let browser = RemoteBrowser::open(conn.clone()).await?;
        Ok(Self {
            remote: Arc::new(RealRemote(Arc::new(browser))),
            mgr: TransferManager::new(rt, conn, MAX_CONCURRENT, notify),
        })
    }
}

impl FilesBackend for RealBackend {
    fn remote(&self) -> Arc<dyn RemoteFs> {
        self.remote.clone()
    }
    fn enqueue(&self, req: TransferRequest) -> TransferId {
        self.mgr.enqueue(req)
    }
    fn cancel(&self, id: TransferId) {
        self.mgr.cancel(id)
    }
    fn cancel_all(&self) {
        self.mgr.cancel_all()
    }
    fn try_recv(&mut self) -> Option<TransferEvent> {
        self.mgr.try_recv()
    }
    fn active(&self) -> usize {
        self.mgr.active()
    }
}

#[cfg(test)]
pub use mem::{MemBackend, MemHandle, mem_entry};

#[cfg(test)]
mod mem {
    use std::collections::{BTreeMap, HashSet, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    use super::*;
    use crate::files::model::remote_parent;

    #[derive(Default)]
    pub struct MemState {
        pub dirs: BTreeMap<String, Vec<RemoteEntry>>,
        pub events: VecDeque<TransferEvent>,
        pub requests: Vec<(TransferId, TransferRequest)>,
        pub active: HashSet<TransferId>,
        pub cancelled: Vec<TransferId>,
        pub cancel_all_calls: usize,
        pub list_errors: BTreeMap<String, TransferError>,
        pub removed: Vec<(String, bool)>,
        pub next_id: u64,
        pub lists: usize,
    }

    /// Acesso do teste ao estado do backend simulado.
    #[derive(Clone)]
    pub struct MemHandle {
        pub state: Arc<Mutex<MemState>>,
        notify: myssh_ssh::Notify,
    }

    pub struct MemBackend {
        h: MemHandle,
    }

    pub fn entry(path: &str, kind: Kind, size: u64, mtime_secs: u64) -> RemoteEntry {
        RemoteEntry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.to_string(),
            kind,
            size,
            mtime: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime_secs)),
            perms: Some(0o644),
        }
    }

    impl MemHandle {
        pub fn add(&self, e: RemoteEntry) {
            let parent = remote_parent(&e.path);
            let mut s = self.state.lock().unwrap();
            if e.kind == Kind::Dir {
                s.dirs.entry(e.path.clone()).or_default();
            }
            let v = s.dirs.entry(parent).or_default();
            v.retain(|x| x.name != e.name);
            v.push(e);
        }

        /// Empurra um evento como se o motor o tivesse produzido.
        pub fn push(&self, ev: TransferEvent) {
            {
                let mut s = self.state.lock().unwrap();
                match &ev {
                    TransferEvent::Finished { id, .. } | TransferEvent::Cancelled { id } => {
                        s.active.remove(id);
                    }
                    _ => {}
                }
                s.events.push_back(ev);
            }
            (self.notify)();
        }

        /// Empurra um conflito e devolve onde a resposta da UI chega.
        pub fn push_conflict(
            &self,
            id: TransferId,
            path: &str,
            existing: RemoteOrLocalMeta,
        ) -> tokio::sync::oneshot::Receiver<ConflictDecision> {
            let (responder, rx) = ConflictResponder::channel();
            self.push(TransferEvent::Conflict {
                id,
                path: path.into(),
                existing,
                responder,
            });
            rx
        }

        pub fn requests(&self) -> Vec<(TransferId, TransferRequest)> {
            self.state.lock().unwrap().requests.clone()
        }

        pub fn names_in(&self, dir: &str) -> Vec<String> {
            let s = self.state.lock().unwrap();
            let mut v: Vec<_> = s
                .dirs
                .get(dir)
                .map(|v| v.iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default();
            v.sort();
            v
        }

        pub fn fail_list(&self, dir: &str, err: TransferError) {
            self.state
                .lock()
                .unwrap()
                .list_errors
                .insert(dir.into(), err);
        }
    }

    impl MemBackend {
        pub fn new(notify: myssh_ssh::Notify) -> (Self, MemHandle) {
            let h = MemHandle {
                state: Arc::new(Mutex::new(MemState {
                    next_id: 1,
                    ..Default::default()
                })),
                notify,
            };
            h.state
                .lock()
                .unwrap()
                .dirs
                .insert("/home/ana".into(), Vec::new());
            (Self { h: h.clone() }, h)
        }
    }

    struct MemFs(MemHandle);

    fn ok<T: Send + 'static>(v: Result<T, TransferError>) -> BoxFut<T> {
        Box::pin(async move { v })
    }

    impl RemoteFs for MemFs {
        fn home(&self) -> BoxFut<String> {
            ok(Ok("/home/ana".into()))
        }

        fn list(&self, dir: &str) -> BoxFut<Vec<RemoteEntry>> {
            let mut s = self.0.state.lock().unwrap();
            s.lists += 1;
            if let Some(m) = s.list_errors.get(dir) {
                return ok(Err(m.clone()));
            }
            match s.dirs.get(dir) {
                Some(v) => ok(Ok(v.clone())),
                None => ok(Err(TransferError::NotFound)),
            }
        }

        fn mkdir(&self, path: &str) -> BoxFut<()> {
            self.0.add(entry(path, Kind::Dir, 0, 1_700_000_000));
            ok(Ok(()))
        }

        fn rename(&self, from: &str, to: &str) -> BoxFut<()> {
            let mut s = self.0.state.lock().unwrap();
            let pf = remote_parent(from);
            let found = s
                .dirs
                .get_mut(&pf)
                .and_then(|v| v.iter().position(|e| e.path == from).map(|i| v.remove(i)));
            let Some(mut e) = found else {
                return ok(Err(TransferError::NotFound));
            };
            e.path = to.to_string();
            e.name = to.rsplit('/').next().unwrap_or(to).to_string();
            let kind = e.kind;
            s.dirs.entry(remote_parent(to)).or_default().push(e);
            if kind == Kind::Dir
                && let Some(children) = s.dirs.remove(from)
            {
                s.dirs.insert(to.to_string(), children);
            }
            ok(Ok(()))
        }

        fn remove(&self, path: &str, recursive: bool) -> BoxFut<()> {
            let mut s = self.0.state.lock().unwrap();
            s.removed.push((path.to_string(), recursive));
            let parent = remote_parent(path);
            if let Some(v) = s.dirs.get_mut(&parent) {
                v.retain(|e| e.path != path);
            }
            s.dirs.remove(path);
            ok(Ok(()))
        }
    }

    impl FilesBackend for MemBackend {
        fn remote(&self) -> Arc<dyn RemoteFs> {
            Arc::new(MemFs(self.h.clone()))
        }

        fn enqueue(&self, req: TransferRequest) -> TransferId {
            let id;
            let label = match req.direction {
                Direction::Upload => format!("Enviar {}", req.local.display()),
                Direction::Download => format!("Baixar {}", req.remote),
            };
            {
                let mut s = self.h.state.lock().unwrap();
                id = TransferId(s.next_id);
                s.next_id += 1;
                s.requests.push((id, req));
                s.active.insert(id);
            }
            self.h.push(TransferEvent::Queued { id, label });
            id
        }

        fn cancel(&self, id: TransferId) {
            let was = {
                let mut s = self.h.state.lock().unwrap();
                s.cancelled.push(id);
                s.active.contains(&id)
            };
            if was {
                self.h.push(TransferEvent::Cancelled { id });
            }
        }

        fn cancel_all(&self) {
            let ids: Vec<_> = {
                let mut s = self.h.state.lock().unwrap();
                s.cancel_all_calls += 1;
                let mut v: Vec<_> = s.active.iter().copied().collect();
                v.sort_by_key(|i| i.0);
                v
            };
            for id in ids {
                self.h.push(TransferEvent::Cancelled { id });
            }
        }

        fn try_recv(&mut self) -> Option<TransferEvent> {
            self.h.state.lock().unwrap().events.pop_front()
        }

        fn active(&self) -> usize {
            self.h.state.lock().unwrap().active.len()
        }
    }

    pub use entry as mem_entry;
}
