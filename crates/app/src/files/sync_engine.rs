//! Interface da UI com o executor de sincronização (`myssh-syncrun`) e a versão simulada dos testes.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use myssh_ssh::Connection;
use myssh_syncrun::myssh_sync::{CompareMode, Direction, Journal, Plan, PlanOptions};
use myssh_syncrun::{Decisions, SyncError, SyncEvent, SyncRunner, SyncState};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::sync_text::parse_exclusions;

pub type SyncFut<T> = Pin<Box<dyn Future<Output = Result<T, SyncError>> + Send>>;

/// O que o usuário escolheu no diálogo de opções.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncRequest {
    pub direction: Direction,
    /// Comparação por conteúdo (lê os arquivos dos dois lados); senão tamanho + data.
    pub content: bool,
    pub mirror: bool,
    /// Padrões de exclusão (um por item), já sem linhas vazias.
    pub exclusions: Vec<String>,
    pub local_root: PathBuf,
    pub remote_root: String,
}

impl SyncRequest {
    pub fn plan_options(&self) -> Result<PlanOptions, SyncError> {
        let mut o = PlanOptions::new(self.direction);
        if self.content {
            o.compare = CompareMode::Content;
        }
        o.mirror_deletes = self.mirror;
        o.exclusions = parse_exclusions(&self.exclusions.join("\n")).map_err(SyncError::Io)?;
        Ok(o)
    }
}

pub trait SyncEngine: Send + Sync {
    fn plan(&self, req: SyncRequest, events: mpsc::Sender<SyncEvent>) -> SyncFut<Plan>;
    /// `resume = Some(journal)` retoma uma execução interrompida.
    fn execute(
        &self,
        req: SyncRequest,
        plan: Arc<Plan>,
        decisions: Decisions,
        resume: Option<Journal>,
        events: mpsc::Sender<SyncEvent>,
        cancel: CancellationToken,
    ) -> SyncFut<Journal>;
    /// Estado gravado por uma execução anterior (se houver).
    fn load_state(&self) -> Pin<Box<dyn Future<Output = Option<SyncState>> + Send>>;
    fn discard_state(&self) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// Executor real sobre a conexão SSH da sessão.
pub struct RealSync {
    pub conn: Arc<Connection>,
    pub profile_id: String,
    pub journal_path: PathBuf,
}

impl RealSync {
    fn runner(&self, req: &SyncRequest) -> Result<SyncRunner, SyncError> {
        Ok(SyncRunner::new(
            self.conn.clone(),
            req.local_root.clone(),
            req.remote_root.clone(),
            self.profile_id.clone(),
            self.journal_path.clone(),
            req.plan_options()?,
        ))
    }
}

impl SyncEngine for RealSync {
    fn plan(&self, req: SyncRequest, events: mpsc::Sender<SyncEvent>) -> SyncFut<Plan> {
        let runner = self.runner(&req);
        Box::pin(async move { runner?.plan_with_events(Some(&events)).await })
    }

    fn execute(
        &self,
        req: SyncRequest,
        plan: Arc<Plan>,
        decisions: Decisions,
        resume: Option<Journal>,
        events: mpsc::Sender<SyncEvent>,
        cancel: CancellationToken,
    ) -> SyncFut<Journal> {
        let runner = self.runner(&req);
        Box::pin(async move {
            let r = runner?;
            match resume {
                Some(j) => r.resume(&plan, &decisions, j, events, cancel).await,
                None => r.execute(&plan, &decisions, events, cancel).await,
            }
        })
    }

    fn load_state(&self) -> Pin<Box<dyn Future<Output = Option<SyncState>> + Send>> {
        let p = self.journal_path.clone();
        Box::pin(async move {
            if !tokio::fs::try_exists(&p).await.unwrap_or(false) {
                return None;
            }
            myssh_syncrun::load_state(&p).await.ok()
        })
    }

    fn discard_state(&self) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let p = self.journal_path.clone();
        Box::pin(async move {
            let _ = tokio::fs::remove_file(p).await;
        })
    }
}

#[cfg(test)]
pub use mem::{ExecScript, MemSync};

#[cfg(test)]
mod mem {
    use std::sync::Mutex;

    use myssh_syncrun::RunSummary;
    use myssh_syncrun::myssh_sync::Changed;

    use super::*;

    /// O que a execução simulada faz.
    pub enum ExecScript {
        /// Emite os eventos e termina bem.
        Events(Vec<SyncEvent>),
        /// `PlanStale` com essas mudanças.
        Stale(Vec<Changed>),
        /// Fica rodando até ser cancelada (emite `Finished` com `cancelled`).
        UntilCancelled(Vec<SyncEvent>),
    }

    #[derive(Default)]
    pub struct MemSync {
        pub plan_result: Mutex<Option<Plan>>,
        pub plan_events: Mutex<Vec<SyncEvent>>,
        pub exec: Mutex<Option<ExecScript>>,
        pub state: Mutex<Option<SyncState>>,
        /// Pedidos recebidos.
        pub plan_requests: Mutex<Vec<SyncRequest>>,
        pub executions: Mutex<Vec<(SyncRequest, Decisions, bool)>>,
    }

    impl SyncEngine for MemSync {
        fn plan(&self, req: SyncRequest, events: mpsc::Sender<SyncEvent>) -> SyncFut<Plan> {
            self.plan_requests.lock().unwrap().push(req);
            let plan = self.plan_result.lock().unwrap().clone();
            let evs = self.plan_events.lock().unwrap().clone();
            Box::pin(async move {
                for e in evs {
                    let _ = events.send(e).await;
                }
                plan.ok_or_else(|| SyncError::Io("sem plano simulado".into()))
            })
        }

        fn execute(
            &self,
            req: SyncRequest,
            _plan: Arc<Plan>,
            decisions: Decisions,
            resume: Option<Journal>,
            events: mpsc::Sender<SyncEvent>,
            cancel: CancellationToken,
        ) -> SyncFut<Journal> {
            self.executions
                .lock()
                .unwrap()
                .push((req, decisions, resume.is_some()));
            let script = self.exec.lock().unwrap().take();
            Box::pin(async move {
                match script {
                    Some(ExecScript::Events(evs)) => {
                        for e in evs {
                            let _ = events.send(e).await;
                        }
                        Ok(Journal::default())
                    }
                    Some(ExecScript::Stale(c)) => Err(SyncError::PlanStale(c)),
                    Some(ExecScript::UntilCancelled(evs)) => {
                        for e in evs {
                            let _ = events.send(e).await;
                        }
                        cancel.cancelled().await;
                        let _ = events
                            .send(SyncEvent::Finished(RunSummary {
                                cancelled: true,
                                pending: 1,
                                ..Default::default()
                            }))
                            .await;
                        Ok(Journal::default())
                    }
                    None => Err(SyncError::Io("sem script de execução".into())),
                }
            })
        }

        fn load_state(&self) -> Pin<Box<dyn Future<Output = Option<SyncState>> + Send>> {
            let s = self.state.lock().unwrap().take();
            Box::pin(async move { s })
        }

        fn discard_state(&self) -> Pin<Box<dyn Future<Output = ()> + Send>> {
            *self.state.lock().unwrap() = None;
            Box::pin(async {})
        }
    }
}
