//! Execução de um plano: ordem do plano, janela de transferências, deletes revalidados, journal.

use crate::side::{AnyTree, kind_name, local_path, remote_path};
use crate::{ConflictChoice, Decisions, RunSummary, SyncError, SyncEvent, SyncRunner, save_state};
use myssh_sync::{
    Action, ConflictReason, EntryKind, FileTree, Journal, JournalItem, Op, Plan, PlanItem, RelPath,
    Snapshot, Status,
};
use myssh_transfer::{
    ConflictDecision, Direction, Overwrite, RemoteBrowser, TransferEvent, TransferId,
    TransferManager, TransferRequest,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

struct Flight {
    path: RelPath,
    op: Op,
    size: u64,
    bytes_done: u64,
    last_error: Option<String>,
}

pub(crate) struct Run<'a> {
    r: &'a SyncRunner,
    plan: &'a Plan,
    decisions: &'a Decisions,
    journal: Journal,
    items: HashMap<RelPath, &'a PlanItem>,
    events: mpsc::Sender<SyncEvent>,
    cancel: CancellationToken,
    dst: AnyTree,
    browser: Option<RemoteBrowser>,
    mgr: TransferManager,
    wake: Arc<Notify>,
    in_flight: HashMap<TransferId, Flight>,
    verified: HashSet<RelPath>,
    failed_dirs: Vec<RelPath>,
    cancel_sent: bool,
    // contadores
    done: usize,
    failed: usize,
    skipped: usize,
    items_total: usize,
    bytes_total: u64,
    bytes_completed: u64,
    current: String,
    last_progress: Instant,
    last_save: Instant,
    dirty: bool,
}

fn is_under(dirs: &[RelPath], p: &RelPath) -> bool {
    dirs.iter().any(|d| {
        let pre = format!("{}/", d.as_str());
        p.as_str().starts_with(&pre)
    })
}

impl<'a> Run<'a> {
    pub(crate) async fn new(
        r: &'a SyncRunner,
        plan: &'a Plan,
        decisions: &'a Decisions,
        journal: Journal,
        events: mpsc::Sender<SyncEvent>,
        cancel: CancellationToken,
    ) -> Result<Run<'a>, SyncError> {
        let (_, dst) = r.trees().await?;
        let browser = if r.dest_is_remote() {
            Some(
                RemoteBrowser::open(r.conn.clone())
                    .await
                    .map_err(SyncError::Transfer)?,
            )
        } else {
            None
        };
        let wake = Arc::new(Notify::new());
        let w2 = wake.clone();
        let mgr = TransferManager::new(
            &tokio::runtime::Handle::current(),
            r.conn.clone(),
            r.concurrency,
            Arc::new(move || w2.notify_one()),
        );
        let items = plan
            .items
            .iter()
            .filter(|i| !matches!(i.action, Action::Skip { .. }))
            .map(|i| (i.path.clone(), i))
            .collect();
        let remaining: Vec<&JournalItem> = journal.remaining().collect();
        let items_total = remaining.len();
        let bytes_total = remaining
            .iter()
            .filter(|j| j.kind == EntryKind::File && j.op != Op::Delete)
            .filter_map(|j| plan.items.iter().find(|p| p.path == j.path))
            .map(|p| p.src.map_or(0, |s| s.size))
            .sum();
        Ok(Run {
            r,
            plan,
            decisions,
            journal,
            items,
            events,
            cancel,
            dst,
            browser,
            mgr,
            wake,
            in_flight: HashMap::new(),
            verified: HashSet::new(),
            failed_dirs: Vec::new(),
            cancel_sent: false,
            done: 0,
            failed: 0,
            skipped: 0,
            items_total,
            bytes_total,
            bytes_completed: 0,
            current: String::new(),
            last_progress: Instant::now() - Duration::from_secs(1),
            last_save: Instant::now(),
            dirty: false,
        })
    }

    async fn emit(&self, ev: SyncEvent) {
        let _ = self.events.send(ev).await;
    }

    async fn save(&mut self, force: bool) {
        if !self.dirty && !force {
            return;
        }
        if !force && self.last_save.elapsed() < Duration::from_millis(500) {
            return;
        }
        if let Err(e) = save_state(
            &self.r.journal_path,
            &self.r.profile_id,
            self.plan,
            &self.journal,
        )
        .await
        {
            self.emit(SyncEvent::Warning {
                message: format!("Não foi possível gravar o journal: {e}"),
            })
            .await;
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }

    async fn progress(&mut self, force: bool) {
        if !force && self.last_progress.elapsed() < Duration::from_millis(150) {
            return;
        }
        self.last_progress = Instant::now();
        let inflight: u64 = self
            .in_flight
            .values()
            .map(|f| f.bytes_done.min(f.size))
            .sum();
        let ev = SyncEvent::Progress {
            items_done: self.done + self.failed + self.skipped,
            items_total: self.items_total,
            bytes_done: self.bytes_completed + inflight,
            bytes_total: self.bytes_total,
            current: self.current.clone(),
        };
        let _ = self.events.try_send(ev);
    }

    async fn set_done(&mut self, path: &RelPath, op: Op, size: u64) {
        self.done += 1;
        self.bytes_completed += size;
        self.journal.mark(path, op, Status::Done);
        self.dirty = true;
        self.emit(SyncEvent::ItemDone {
            path: path.clone(),
            op,
        })
        .await;
        self.save(false).await;
        self.progress(false).await;
    }

    async fn set_failed(&mut self, path: &RelPath, op: Op, size: u64, msg: String) {
        self.failed += 1;
        self.bytes_completed += size;
        self.journal.mark(path, op, Status::Failed(msg.clone()));
        self.dirty = true;
        self.emit(SyncEvent::ItemFailed {
            path: path.clone(),
            op,
            message: msg,
        })
        .await;
        self.save(false).await;
        self.progress(false).await;
    }

    async fn set_skipped(&mut self, path: &RelPath, op: Op, why: &str) {
        self.skipped += 1;
        self.journal.mark(path, op, Status::Skipped);
        self.dirty = true;
        self.emit(SyncEvent::ItemSkipped {
            path: path.clone(),
            op,
            reason: why.to_string(),
        })
        .await;
        self.save(false).await;
    }

    // ------------------------------------------------------------ principal

    pub(crate) async fn run(mut self) -> Result<Journal, SyncError> {
        self.save(true).await;
        let todo: Vec<JournalItem> = self.journal.remaining().cloned().collect();
        let has_writes = todo.iter().any(|j| j.op != Op::Delete);
        if has_writes {
            self.ensure_dest_root().await?;
        }
        let deletes_ok = self.plan.mirror_deletes && self.decisions.confirm_deletes;
        for ji in todo {
            if self.cancel.is_cancelled() {
                break;
            }
            self.pump_events().await;
            if self.fatal_conn_lost() {
                break;
            }
            if ji.op == Op::Delete && !deletes_ok {
                continue; // fica Pending: nada é apagado sem confirmação explícita
            }
            self.process(ji).await;
        }
        // Espera as transferências em voo (ou o cancelamento delas).
        let deadline = Instant::now() + Duration::from_secs(120);
        while !self.in_flight.is_empty() && Instant::now() < deadline {
            self.pump().await;
        }
        let pending = self
            .journal
            .items
            .iter()
            .filter(|i| i.status == Status::Pending)
            .count();
        self.save(true).await;
        self.progress(true).await;
        let summary = RunSummary {
            done: self.done,
            failed: self.failed,
            skipped: self.skipped,
            pending,
            bytes: self.bytes_completed,
            cancelled: self.cancel.is_cancelled(),
        };
        self.emit(SyncEvent::Finished(summary)).await;
        Ok(self.journal)
    }

    fn fatal_conn_lost(&self) -> bool {
        self.r.conn.is_closed()
    }

    async fn process(&mut self, ji: JournalItem) {
        let Some(pi) = self.items.get(&ji.path).copied() else {
            self.set_skipped(&ji.path, ji.op, "item não consta mais no plano")
                .await;
            return;
        };
        self.current = ji.path.to_string();
        match ji.op {
            Op::Delete => self.do_delete(&ji, pi).await,
            Op::Create | Op::Update if pi.kind == EntryKind::Dir => self.do_mkdir(&ji).await,
            Op::Create | Op::Update => self.do_file(&ji, pi).await,
        }
    }

    // ------------------------------------------------------------ destino

    async fn ensure_dest_root(&mut self) -> Result<(), SyncError> {
        let root = RelPath::root();
        match self.dst.stat(&root).await? {
            Some(e) if e.kind == EntryKind::Dir => {}
            Some(_) => return Err(SyncError::Io("a raiz do destino não é um diretório".into())),
            None => {
                if let Some(b) = &self.browser {
                    // mkdir -p remoto, componente a componente
                    let mut cur = String::new();
                    for c in self.r.remote_root.split('/').filter(|c| !c.is_empty()) {
                        cur.push('/');
                        cur.push_str(c);
                        if b.stat(&cur).await.map_err(SyncError::Transfer)?.is_none() {
                            b.mkdir(&cur).await.map_err(SyncError::Transfer)?;
                        }
                    }
                } else {
                    tokio::fs::create_dir_all(&self.r.local_root)
                        .await
                        .map_err(|e| SyncError::Io(e.to_string()))?;
                }
            }
        }
        self.verified.insert(root);
        Ok(())
    }

    /// Todos os ancestrais (incluindo `dir`) são diretórios reais (lstat), nunca symlinks.
    async fn verify_chain(&mut self, dir: &RelPath) -> Result<(), String> {
        let mut chain = Vec::new();
        let mut cur = Some(dir.clone());
        while let Some(c) = cur {
            if self.verified.contains(&c) {
                break;
            }
            cur = c.parent();
            chain.push(c);
        }
        for c in chain.iter().rev() {
            match self.dst.stat(c).await {
                Ok(Some(e)) if e.kind == EntryKind::Dir => {
                    self.verified.insert(c.clone());
                }
                Ok(Some(e)) => {
                    return Err(format!(
                        "«{c}» no destino é {}, não um diretório",
                        kind_name(e.kind)
                    ));
                }
                Ok(None) => return Err(format!("«{c}» não existe no destino")),
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    async fn do_mkdir(&mut self, ji: &JournalItem) {
        let parent = ji.path.parent().unwrap_or_else(RelPath::root);
        if is_under(&self.failed_dirs, &ji.path) {
            self.failed_dirs.push(ji.path.clone());
            return self
                .set_failed(&ji.path, ji.op, 0, "diretório pai não foi criado".into())
                .await;
        }
        if let Err(m) = self.verify_chain(&parent).await {
            self.failed_dirs.push(ji.path.clone());
            return self.set_failed(&ji.path, ji.op, 0, m).await;
        }
        let res: Result<(), String> = if let Some(b) = &self.browser {
            match remote_path(&self.r.remote_root, &ji.path) {
                Ok(p) => b.mkdir(&p).await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            match local_path(&self.r.local_root, &ji.path) {
                Ok(p) => tokio::fs::create_dir(&p).await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        let res = match res {
            Ok(()) => Ok(()),
            // Pode já existir (corrida): aceita só se for um diretório real.
            Err(m) => match self.dst.stat(&ji.path).await {
                Ok(Some(e)) if e.kind == EntryKind::Dir => Ok(()),
                _ => Err(m),
            },
        };
        match res {
            Ok(()) => {
                self.verified.insert(ji.path.clone());
                self.set_done(&ji.path, ji.op, 0).await;
            }
            Err(m) => {
                self.failed_dirs.push(ji.path.clone());
                self.set_failed(&ji.path, ji.op, 0, format!("não foi possível criar: {m}"))
                    .await;
            }
        }
    }

    // ------------------------------------------------------------ arquivos

    async fn do_file(&mut self, ji: &JournalItem, pi: &PlanItem) {
        let size = pi.src.map_or(0, |s| s.size);
        // Conflitos só com decisão explícita e apenas quando sobrescrever é bem definido.
        if let Action::Conflict { reason } = &pi.action {
            if self.decisions.conflicts.get(&ji.path) != Some(&ConflictChoice::UseSource) {
                return self
                    .set_skipped(&ji.path, ji.op, "conflito sem decisão do usuário")
                    .await;
            }
            let ok = matches!(
                reason,
                ConflictReason::DestModifiedAfter
                    | ConflictReason::DestNewer
                    | ConflictReason::MtimeUnknown
            ) && pi.kind == EntryKind::File
                && pi.src.is_some_and(|s| s.kind == EntryKind::File)
                && pi.dst.is_some_and(|d| d.kind == EntryKind::File);
            if !ok {
                return self
                    .set_failed(
                        &ji.path,
                        ji.op,
                        size,
                        "conflito de tipo ou de nomes: resolva manualmente".into(),
                    )
                    .await;
            }
        }
        let parent = ji.path.parent().unwrap_or_else(RelPath::root);
        if is_under(&self.failed_dirs, &ji.path) {
            return self
                .set_failed(&ji.path, ji.op, size, "diretório pai não foi criado".into())
                .await;
        }
        if let Err(m) = self.verify_chain(&parent).await {
            return self.set_failed(&ji.path, ji.op, size, m).await;
        }
        // O destino deve estar exatamente como no plano.
        let now = match self.dst.stat(&ji.path).await {
            Ok(e) => e.as_ref().map(Snapshot::from),
            Err(e) => return self.set_failed(&ji.path, ji.op, size, e.to_string()).await,
        };
        let expected = if ji.op == Op::Create { None } else { pi.dst };
        if now != expected {
            return self.set_failed(&ji.path, ji.op, size, CHANGED.into()).await;
        }
        // Janela de transferências.
        while self.in_flight.len() >= self.r.concurrency && !self.cancel.is_cancelled() {
            self.pump().await;
        }
        if self.cancel.is_cancelled() {
            return;
        }
        let req = match self.request(&ji.path, ji.op) {
            Ok(r) => r,
            Err(e) => return self.set_failed(&ji.path, ji.op, size, e.to_string()).await,
        };
        let id = self.mgr.enqueue(req);
        self.in_flight.insert(
            id,
            Flight {
                path: ji.path.clone(),
                op: ji.op,
                size,
                bytes_done: 0,
                last_error: None,
            },
        );
        self.progress(false).await;
    }

    fn request(&self, rel: &RelPath, op: Op) -> Result<TransferRequest, SyncError> {
        let parent = rel.parent().unwrap_or_else(RelPath::root);
        let mut req = match self.plan.direction {
            myssh_sync::Direction::LocalToRemote => TransferRequest::new(
                Direction::Upload,
                local_path(&self.r.local_root, rel)?,
                remote_path(&self.r.remote_root, &parent)?,
            ),
            myssh_sync::Direction::RemoteToLocal => TransferRequest::new(
                Direction::Download,
                local_path(&self.r.local_root, &parent)?,
                remote_path(&self.r.remote_root, rel)?,
            ),
        };
        // Create nunca pisa um arquivo que apareceu depois do plano.
        req.overwrite = if op == Op::Create {
            Overwrite::Skip
        } else {
            Overwrite::Overwrite
        };
        req.preserve_mtime = true;
        req.preserve_perms = self.r.preserve_perms;
        Ok(req)
    }

    async fn pump_events(&mut self) {
        while let Some(ev) = self.mgr.try_recv() {
            self.on_event(ev).await;
        }
    }

    async fn pump(&mut self) {
        if self.cancel.is_cancelled() && !self.cancel_sent {
            self.cancel_sent = true;
            self.mgr.cancel_all();
        }
        tokio::select! {
            _ = self.wake.notified() => {}
            _ = self.cancel.cancelled(), if !self.cancel_sent => {}
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
        self.pump_events().await;
        self.progress(false).await;
    }

    async fn on_event(&mut self, ev: TransferEvent) {
        match ev {
            TransferEvent::Progress { id, bytes_done, .. } => {
                if let Some(f) = self.in_flight.get_mut(&id) {
                    f.bytes_done = bytes_done;
                }
            }
            TransferEvent::FileError { id, message, .. } => {
                if let Some(f) = self.in_flight.get_mut(&id) {
                    f.last_error = Some(message);
                }
            }
            TransferEvent::Conflict { responder, .. } => {
                // Não deveria ocorrer (Overwrite/Skip fixos); nunca decide por conta própria.
                responder.answer(ConflictDecision::Skip);
            }
            TransferEvent::Cancelled { id } => {
                // Continua Pending no journal: o resume refaz.
                self.in_flight.remove(&id);
            }
            TransferEvent::Finished {
                id,
                ok,
                failed,
                skipped,
            } => {
                let Some(f) = self.in_flight.remove(&id) else {
                    return;
                };
                if ok >= 1 && failed == 0 {
                    self.set_done(&f.path, f.op, f.size).await;
                } else if skipped >= 1 && failed == 0 {
                    self.set_failed(
                        &f.path,
                        f.op,
                        f.size,
                        "o destino apareceu depois do plano; não foi sobrescrito".into(),
                    )
                    .await;
                } else {
                    let m = f
                        .last_error
                        .unwrap_or_else(|| "falha na transferência".into());
                    self.set_failed(&f.path, f.op, f.size, m).await;
                }
            }
            TransferEvent::Queued { .. } | TransferEvent::Scanning { .. } => {}
        }
    }

    // ------------------------------------------------------------ deletes

    async fn do_delete(&mut self, ji: &JournalItem, pi: &PlanItem) {
        if let Some(h) = &self.r.before_delete {
            h(&ji.path);
        }
        let Some(expected) = pi.dst else {
            return self
                .set_failed(&ji.path, ji.op, 0, "plano sem estado do destino".into())
                .await;
        };
        let parent = ji.path.parent().unwrap_or_else(RelPath::root);
        if let Err(m) = self.verify_chain(&parent).await {
            return self.set_failed(&ji.path, ji.op, 0, m).await;
        }
        // lstat imediatamente antes de apagar.
        let now = match self.dst.stat(&ji.path).await {
            Ok(e) => e,
            Err(e) => return self.set_failed(&ji.path, ji.op, 0, e.to_string()).await,
        };
        let Some(now) = now else {
            // Já não existe: o objetivo está cumprido.
            return self.set_done(&ji.path, ji.op, 0).await;
        };
        // Diretórios: só o tipo conta (o mtime muda quando os filhos são apagados).
        let same = if expected.kind == EntryKind::Dir {
            now.kind == EntryKind::Dir
        } else {
            Snapshot::from(&now) == expected
        };
        if !same {
            return self.set_failed(&ji.path, ji.op, 0, CHANGED.into()).await;
        }
        let res: Result<(), String> = if let Some(b) = &self.browser {
            match remote_path(&self.r.remote_root, &ji.path) {
                // recursive = false: diretório não vazio falha, nunca `rm -r`.
                Ok(p) => b.remove(&p, false).await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            match local_path(&self.r.local_root, &ji.path) {
                Ok(p) => if now.kind == EntryKind::Dir {
                    tokio::fs::remove_dir(&p).await
                } else {
                    tokio::fs::remove_file(&p).await
                }
                .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        match res {
            Ok(()) => {
                self.verified.remove(&ji.path);
                self.set_done(&ji.path, ji.op, 0).await;
            }
            Err(m) => {
                self.set_failed(&ji.path, ji.op, 0, format!("não foi possível apagar: {m}"))
                    .await;
            }
        }
    }
}

pub(crate) const CHANGED: &str = "alterado após o preview";
