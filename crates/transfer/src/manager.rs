//! Fila de transferências: jobs, concorrência por arquivo, canais SFTP próprios, progresso,
//! conflitos, cancelamento e perda de conexão.

use crate::engine::{self, CopyArgs, Meter, Stats};
use crate::error::TransferError;
use crate::names::{self, LocalOs};
use crate::rate::RateMeter;
use crate::scan::{self, Item, Plan, ScanOpts};
use crate::side::Side;
use crate::{
    ConflictDecision, ConflictResponder, Direction, Notify, Overwrite, RemoteOrLocalMeta,
    TransferEvent, TransferId, TransferRequest,
};
use myssh_ssh::{Connection, Sftp};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const TICK: Duration = Duration::from_millis(150);
const RATE_WINDOW: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub(crate) struct Emitter {
    tx: mpsc::UnboundedSender<TransferEvent>,
    notify: Notify,
}

impl Emitter {
    fn emit(&self, e: TransferEvent) {
        let _ = self.tx.send(e);
        (self.notify)();
    }
}

/// Contadores de diagnóstico do gerenciador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferStats {
    /// Bytes de arquivo realmente movidos (sem contar os retomados).
    pub bytes_transferred: u64,
    pub files_resumed: u64,
    pub bytes_resumed: u64,
}

struct Shared {
    conn: Arc<Connection>,
    emit: Emitter,
    sem: Arc<Semaphore>,
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, CancellationToken>>,
    active: AtomicUsize,
    pool: tokio::sync::Mutex<Vec<Sftp>>,
    case_insensitive: AtomicBool,
    scan_max_depth: AtomicUsize,
    scan_max_entries: AtomicU64,
    os: LocalOs,
    stats: Stats,
}

impl Shared {
    async fn checkout(&self) -> Result<Sftp, TransferError> {
        if let Some(s) = self.pool.lock().await.pop() {
            return Ok(s);
        }
        crate::browser::open_sftp(&self.conn).await
    }

    async fn checkin(&self, s: Sftp) {
        if !self.conn.is_closed() {
            self.pool.lock().await.push(s);
        }
    }

    async fn finish_job(&self, id: u64) {
        if let Ok(mut j) = self.jobs.lock() {
            j.remove(&id);
        }
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            // Sem jobs: solta os canais ociosos (não manter a conexão aberta à toa).
            self.pool.lock().await.clear();
        }
    }
}

pub struct TransferManager {
    shared: Arc<Shared>,
    rx: mpsc::UnboundedReceiver<TransferEvent>,
    rt: tokio::runtime::Handle,
}

#[derive(Clone, Copy)]
enum Sticky {
    Overwrite,
    Skip,
}

pub(crate) struct JobShared {
    id: TransferId,
    emit: Emitter,
    pub(crate) cancel: CancellationToken,
    meter: Meter,
    bytes_total: AtomicU64,
    files_total: AtomicU64,
    files_done: AtomicU64,
    ok: AtomicU64,
    failed: AtomicU64,
    skipped: AtomicU64,
    current: Mutex<String>,
    conn_lost: AtomicBool,
    lost_streak: AtomicU64,
    sticky: Mutex<Option<Sticky>>,
    ask: tokio::sync::Mutex<()>,
}

impl JobShared {
    pub(crate) fn scanning(&self, files: u64, bytes: u64) {
        self.emit.emit(TransferEvent::Scanning {
            id: self.id,
            files,
            bytes,
        });
    }

    pub(crate) fn file_error(&self, path: &str, e: &TransferError) {
        self.emit.emit(TransferEvent::FileError {
            id: self.id,
            path: path.to_string(),
            message: e.to_string(),
        });
    }

    fn progress(&self, rate: u64) {
        let cur = self.current.lock().map(|c| c.clone()).unwrap_or_default();
        let total = self.bytes_total.load(Ordering::Relaxed);
        self.emit.emit(TransferEvent::Progress {
            id: self.id,
            current: cur,
            bytes_done: self.meter.credited.load(Ordering::Relaxed).min(total),
            bytes_total: total,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total.load(Ordering::Relaxed),
            rate_bps: rate,
        });
    }
}

impl TransferManager {
    /// `max_concurrent` arquivos simultâneos no total (padrão sugerido: 3), cada um num canal SFTP
    /// próprio (distinto do `RemoteBrowser`).
    pub fn new(
        rt: &tokio::runtime::Handle,
        conn: Arc<Connection>,
        max_concurrent: usize,
        notify: Notify,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let os = LocalOs::current();
        TransferManager {
            shared: Arc::new(Shared {
                conn,
                emit: Emitter { tx, notify },
                sem: Arc::new(Semaphore::new(max_concurrent.max(1))),
                next_id: AtomicU64::new(1),
                jobs: Mutex::new(HashMap::new()),
                active: AtomicUsize::new(0),
                pool: tokio::sync::Mutex::new(Vec::new()),
                case_insensitive: AtomicBool::new(os.default_case_insensitive()),
                scan_max_depth: AtomicUsize::new(crate::scan::DEFAULT_MAX_DEPTH),
                scan_max_entries: AtomicU64::new(crate::scan::DEFAULT_MAX_ENTRIES),
                os,
                stats: Stats::default(),
            }),
            rx,
            rt: rt.clone(),
        }
    }

    /// Define se o sistema de arquivos local ignora caixa (padrão: macOS/Windows sim, Linux não).
    /// Vale para os próximos downloads.
    pub fn set_local_case_insensitive(&self, v: bool) {
        self.shared.case_insensitive.store(v, Ordering::Relaxed);
    }

    /// Limites da varredura de diretórios (padrão: 64 níveis, 1.000.000 de entradas). Ao exceder,
    /// a transferência falha com erro claro. Vale para os próximos trabalhos.
    pub fn set_scan_limits(&self, max_depth: usize, max_entries: u64) {
        self.shared
            .scan_max_depth
            .store(max_depth, Ordering::Relaxed);
        self.shared
            .scan_max_entries
            .store(max_entries, Ordering::Relaxed);
    }

    pub fn stats(&self) -> TransferStats {
        let s = &self.shared.stats;
        TransferStats {
            bytes_transferred: s.bytes_transferred.load(Ordering::Relaxed),
            files_resumed: s.files_resumed.load(Ordering::Relaxed),
            bytes_resumed: s.bytes_resumed.load(Ordering::Relaxed),
        }
    }

    pub fn enqueue(&self, req: TransferRequest) -> TransferId {
        let sh = self.shared.clone();
        let raw = sh.next_id.fetch_add(1, Ordering::Relaxed);
        let id = TransferId(raw);
        let label = match req.direction {
            Direction::Upload => req
                .local
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| req.local.to_string_lossy().into_owned()),
            Direction::Download => names::remote_basename(&req.remote)
                .unwrap_or(&req.remote)
                .to_string(),
        };
        let token = CancellationToken::new();
        if let Ok(mut j) = sh.jobs.lock() {
            j.insert(raw, token.clone());
        }
        sh.active.fetch_add(1, Ordering::AcqRel);
        sh.emit.emit(TransferEvent::Queued { id, label });
        self.rt.spawn(async move {
            run_job(sh.clone(), id, req, token).await;
            sh.finish_job(id.0).await;
        });
        id
    }

    pub fn cancel(&self, id: TransferId) {
        if let Ok(j) = self.shared.jobs.lock()
            && let Some(t) = j.get(&id.0)
        {
            t.cancel();
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(j) = self.shared.jobs.lock() {
            for t in j.values() {
                t.cancel();
            }
        }
    }

    /// Próximo evento, se houver (a UI drena por frame).
    pub fn try_recv(&mut self) -> Option<TransferEvent> {
        self.rx.try_recv().ok()
    }

    /// Transferências em andamento ou na fila (para confirmar o fechamento).
    pub fn active(&self) -> usize {
        self.shared.active.load(Ordering::Acquire)
    }
}

impl Drop for TransferManager {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

fn spawn_ticker(job: Arc<JobShared>) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let stop = CancellationToken::new();
    let s2 = stop.clone();
    let h = tokio::spawn(async move {
        let mut meter = RateMeter::new(RATE_WINDOW);
        let mut iv = tokio::time::interval(TICK);
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = s2.cancelled() => break,
                _ = iv.tick() => {
                    meter.record(Instant::now(), job.meter.transferred.load(Ordering::Relaxed));
                    job.progress(meter.rate_bps());
                }
            }
        }
        meter.record(
            Instant::now(),
            job.meter.transferred.load(Ordering::Relaxed),
        );
        job.progress(meter.rate_bps());
    });
    (stop, h)
}

async fn run_job(sh: Arc<Shared>, id: TransferId, req: TransferRequest, cancel: CancellationToken) {
    let job = Arc::new(JobShared {
        id,
        emit: sh.emit.clone(),
        cancel: cancel.clone(),
        meter: Meter::default(),
        bytes_total: AtomicU64::new(0),
        files_total: AtomicU64::new(0),
        files_done: AtomicU64::new(0),
        ok: AtomicU64::new(0),
        failed: AtomicU64::new(0),
        skipped: AtomicU64::new(0),
        current: Mutex::new(String::new()),
        conn_lost: AtomicBool::new(false),
        lost_streak: AtomicU64::new(0),
        sticky: Mutex::new(None),
        ask: tokio::sync::Mutex::new(()),
    });

    // --- varredura
    let sftp = match sh.checkout().await {
        Ok(s) => s,
        Err(e) => return fail_whole(&job, &req, e),
    };
    let opts = ScanOpts {
        os: sh.os,
        case_insensitive: sh.case_insensitive.load(Ordering::Relaxed),
        limits: crate::scan::ScanLimits {
            max_depth: sh.scan_max_depth.load(Ordering::Relaxed),
            max_entries: sh.scan_max_entries.load(Ordering::Relaxed),
        },
    };
    let scanned = tokio::select! {
        _ = cancel.cancelled() => Err(TransferError::Cancelled),
        r = scan::scan(&job, &req, &sftp, &opts) => r,
    };
    sh.checkin(sftp).await;
    let plan = match scanned {
        Ok(p) => p,
        Err(TransferError::Cancelled) => return job.emit.emit(TransferEvent::Cancelled { id }),
        Err(e) => return fail_whole(&job, &req, e),
    };
    job.bytes_total.store(plan.bytes, Ordering::Relaxed);
    job.files_total.store(plan.files, Ordering::Relaxed);
    job.failed.fetch_add(plan.failed, Ordering::Relaxed);
    job.skipped.fetch_add(plan.skipped, Ordering::Relaxed);

    let (stop, ticker) = spawn_ticker(job.clone());
    let plan = Arc::new(plan);
    execute(&sh, &job, &req, &plan).await;
    stop.cancel();
    let _ = ticker.await;

    if cancel.is_cancelled() {
        job.emit.emit(TransferEvent::Cancelled { id });
    } else {
        job.emit.emit(TransferEvent::Finished {
            id,
            ok: job.ok.load(Ordering::Relaxed),
            failed: job.failed.load(Ordering::Relaxed),
            skipped: job.skipped.load(Ordering::Relaxed),
        });
    }
}

/// Falha antes de qualquer arquivo (raiz inexistente, conexão, symlink na raiz...).
fn fail_whole(job: &JobShared, req: &TransferRequest, e: TransferError) {
    let path = match req.direction {
        Direction::Upload => req.local.to_string_lossy().into_owned(),
        Direction::Download => req.remote.clone(),
    };
    job.file_error(&path, &e);
    let skipped = matches!(
        e,
        TransferError::SymlinkSkipped | TransferError::UnsupportedKind
    );
    job.emit.emit(TransferEvent::Finished {
        id: job.id,
        ok: 0,
        failed: u64::from(!skipped),
        skipped: u64::from(skipped),
    });
}

fn join_rel(side: &Side<'_>, root: &str, rel: &str) -> String {
    let mut p = root.to_string();
    if !rel.is_empty() {
        for c in rel.split('/') {
            p = side.join(&p, c);
        }
    }
    p
}

fn disp(plan: &Plan, rel: &str) -> String {
    if rel.is_empty() {
        plan.name.clone()
    } else {
        format!("{}/{}", plan.name, rel)
    }
}

struct TaskCtx {
    sh: Arc<Shared>,
    job: Arc<JobShared>,
    plan: Arc<Plan>,
    direction: Direction,
    overwrite: Overwrite,
    preserve_mtime: bool,
    preserve_perms: bool,
}

fn sides(d: Direction, sftp: &Sftp) -> (Side<'_>, Side<'_>) {
    let remote = Side::Remote(std::ops::Deref::deref(sftp));
    match d {
        Direction::Upload => (Side::Local, remote),
        Direction::Download => (remote, Side::Local),
    }
}

fn under_failed(failed_dirs: &[String], rel: &str) -> bool {
    failed_dirs
        .iter()
        .any(|f| f.is_empty() || rel.starts_with(&format!("{f}/")))
}

async fn execute(sh: &Arc<Shared>, job: &Arc<JobShared>, req: &TransferRequest, plan: &Arc<Plan>) {
    // --- diretórios (pré-ordem: pais antes dos filhos)
    let mut created: Vec<(String, Option<std::time::SystemTime>, Option<u32>)> = Vec::new();
    let mut failed_dirs: Vec<String> = Vec::new();
    let sftp = match sh.checkout().await {
        Ok(s) => s,
        Err(e) => {
            job.file_error(&plan.name, &e);
            job.failed.fetch_add(plan.files.max(1), Ordering::Relaxed);
            return;
        }
    };
    {
        let (_, dst_side) = sides(req.direction, &sftp);
        for it in &plan.items {
            let Item::Dir { rel, mtime, perms } = it else {
                continue;
            };
            if job.cancel.is_cancelled() {
                break;
            }
            if under_failed(&failed_dirs, rel) {
                continue;
            }
            let path = join_rel(&dst_side, &plan.dst_root, rel);
            match dst_side.ensure_dir(&path).await {
                Ok(true) => created.push((path, *mtime, *perms)),
                Ok(false) => {}
                Err(e) => {
                    job.file_error(&disp(plan, rel), &e);
                    failed_dirs.push(rel.clone());
                    if e.is_connection_lost() {
                        job.conn_lost.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    }
    if job.conn_lost.load(Ordering::Relaxed) {
        drop(sftp);
    } else {
        sh.checkin(sftp).await;
    }

    // --- arquivos
    let files: Vec<(&str, u64)> = plan
        .items
        .iter()
        .filter_map(|i| match i {
            Item::File { rel, size } => Some((rel.as_str(), *size)),
            _ => None,
        })
        .collect();
    let ctx = Arc::new(TaskCtx {
        sh: sh.clone(),
        job: job.clone(),
        plan: plan.clone(),
        direction: req.direction,
        overwrite: req.overwrite,
        preserve_mtime: req.preserve_mtime,
        preserve_perms: req.preserve_perms,
    });
    let mut set: JoinSet<Option<Sftp>> = JoinSet::new();
    let mut handled = 0usize;
    for (rel, size) in &files {
        if job.cancel.is_cancelled() || job.conn_lost.load(Ordering::Relaxed) {
            break;
        }
        if under_failed(&failed_dirs, rel) {
            job.failed.fetch_add(1, Ordering::Relaxed);
            job.files_done.fetch_add(1, Ordering::Relaxed);
            job.meter.credited.fetch_add(*size, Ordering::Relaxed);
            handled += 1;
            continue;
        }
        let permit = tokio::select! {
            _ = job.cancel.cancelled() => break,
            p = sh.sem.clone().acquire_owned() => match p { Ok(p) => p, Err(_) => break },
        };
        while let Some(r) = set.try_join_next() {
            if let Ok(Some(s)) = r {
                sh.checkin(s).await;
            }
        }
        if job.conn_lost.load(Ordering::Relaxed) {
            break;
        }
        let sftp = match sh.checkout().await {
            Ok(s) => s,
            Err(e) => {
                job.file_error(&disp(plan, rel), &e);
                job.failed.fetch_add(1, Ordering::Relaxed);
                job.files_done.fetch_add(1, Ordering::Relaxed);
                handled += 1;
                if e.is_connection_lost() {
                    job.conn_lost.store(true, Ordering::Relaxed);
                    break;
                }
                continue;
            }
        };
        handled += 1;
        let (c, rel, size) = (ctx.clone(), rel.to_string(), *size);
        set.spawn(file_task(c, rel, size, sftp, permit));
    }
    if job.conn_lost.load(Ordering::Relaxed) && !job.cancel.is_cancelled() {
        let rest = (files.len() - handled) as u64;
        if rest > 0 {
            job.failed.fetch_add(rest, Ordering::Relaxed);
            job.emit.emit(TransferEvent::FileError {
                id: job.id,
                path: plan.name.clone(),
                message: format!(
                    "{} {rest} arquivo(s) não foram transferidos.",
                    TransferError::ConnectionLost
                ),
            });
        }
    }
    while let Some(r) = set.join_next().await {
        if let Ok(Some(s)) = r {
            sh.checkin(s).await;
        }
    }

    // --- data/permissões dos diretórios criados (depois do conteúdo)
    let lost = job.conn_lost.load(Ordering::Relaxed);
    if !job.cancel.is_cancelled()
        && !lost
        && (req.preserve_mtime || req.preserve_perms)
        && !created.is_empty()
        && let Ok(sftp) = sh.checkout().await
    {
        {
            let (_, dst_side) = sides(req.direction, &sftp);
            for (path, mtime, perms) in created.iter().rev() {
                let mt = req.preserve_mtime.then_some(*mtime).flatten();
                let pm = req
                    .preserve_perms
                    .then_some(*perms)
                    .flatten()
                    .map(|p| p & 0o777);
                if let Err(e) = dst_side.set_times_perms(path, mt, pm).await {
                    job.emit.emit(TransferEvent::FileError {
                        id: job.id,
                        path: path.clone(),
                        message: format!(
                            "Diretório criado, mas não foi possível preservar data/permissões: {e}"
                        ),
                    });
                }
            }
        }
        sh.checkin(sftp).await;
    }
}

enum Outcome {
    Done(Option<TransferError>),
    Skipped,
}

async fn file_task(
    ctx: Arc<TaskCtx>,
    rel: String,
    size: u64,
    sftp: Sftp,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Option<Sftp> {
    let (job, plan) = (&ctx.job, &ctx.plan);
    let display = disp(plan, &rel);
    if let Ok(mut c) = job.current.lock() {
        *c = display.clone();
    }
    let contrib = AtomicU64::new(0);
    let res = {
        let (src_side, dst_side) = sides(ctx.direction, &sftp);
        let src = join_rel(&src_side, &plan.src_root, &rel);
        let dst = join_rel(&dst_side, &plan.dst_root, &rel);
        process(&ctx, src_side, dst_side, &src, &dst, &display, &contrib).await
    };
    let c = contrib.load(Ordering::Relaxed);
    if size > c {
        job.meter.credited.fetch_add(size - c, Ordering::Relaxed);
    }
    let mut healthy = true;
    match res {
        Ok(Outcome::Done(w)) => {
            job.ok.fetch_add(1, Ordering::Relaxed);
            job.lost_streak.store(0, Ordering::Relaxed);
            if let Some(w) = w {
                job.emit.emit(TransferEvent::FileError {
                    id: job.id,
                    path: display,
                    message: format!(
                        "Arquivo transferido, mas não foi possível preservar data/permissões: {w}"
                    ),
                });
            }
        }
        Ok(Outcome::Skipped) => {
            job.skipped.fetch_add(1, Ordering::Relaxed);
            job.lost_streak.store(0, Ordering::Relaxed);
        }
        Err(TransferError::Cancelled) => {}
        Err(e) => {
            job.failed.fetch_add(1, Ordering::Relaxed);
            if e.is_connection_lost() {
                healthy = false;
                let streak = job.lost_streak.fetch_add(1, Ordering::Relaxed) + 1;
                if ctx.sh.conn.is_closed() || streak >= 2 {
                    job.conn_lost.store(true, Ordering::Relaxed);
                }
            } else {
                job.lost_streak.store(0, Ordering::Relaxed);
            }
            job.file_error(&display, &e);
        }
    }
    job.files_done.fetch_add(1, Ordering::Relaxed);
    drop(permit);
    healthy.then_some(sftp)
}

async fn process(
    ctx: &TaskCtx,
    src_side: Side<'_>,
    dst_side: Side<'_>,
    src: &str,
    dst: &str,
    display: &str,
    contrib: &AtomicU64,
) -> Result<Outcome, TransferError> {
    let job = &ctx.job;
    if job.cancel.is_cancelled() {
        return Err(TransferError::Cancelled);
    }
    let mut dst_path = dst.to_string();
    let mut rename_base = None;
    if let Some(ex) = dst_side.lstat(&dst_path).await? {
        if ex.kind == crate::Kind::Dir {
            return Err(TransferError::DestinationKind("um diretório"));
        }
        match resolve(ctx, display, &ex).await? {
            Resolve::Skip => return Ok(Outcome::Skipped),
            Resolve::Overwrite => {}
            Resolve::Rename => {
                dst_path = dst_side.free_name(dst).await?;
                rename_base = Some(dst);
            }
        }
    }
    let w = engine::copy_file(CopyArgs {
        src: src_side,
        dst: dst_side,
        src_path: src,
        dst_path: &dst_path,
        rename_base,
        preserve_mtime: ctx.preserve_mtime,
        preserve_perms: ctx.preserve_perms,
        cancel: &job.cancel,
        meter: &job.meter,
        stats: &ctx.sh.stats,
        conn: &ctx.sh.conn,
        contrib,
    })
    .await?;
    Ok(Outcome::Done(w))
}

enum Resolve {
    Overwrite,
    Skip,
    Rename,
}

async fn resolve(
    ctx: &TaskCtx,
    display: &str,
    ex: &crate::side::Meta,
) -> Result<Resolve, TransferError> {
    match ctx.overwrite {
        Overwrite::Overwrite => Ok(Resolve::Overwrite),
        Overwrite::Skip => Ok(Resolve::Skip),
        Overwrite::Rename => Ok(Resolve::Rename),
        Overwrite::Ask => {
            let job = &ctx.job;
            let _turn = job.ask.lock().await;
            let sticky = job.sticky.lock().ok().and_then(|s| *s);
            match sticky {
                Some(Sticky::Overwrite) => return Ok(Resolve::Overwrite),
                Some(Sticky::Skip) => return Ok(Resolve::Skip),
                None => {}
            }
            let (tx, rx) = oneshot::channel();
            job.emit.emit(TransferEvent::Conflict {
                id: job.id,
                path: display.to_string(),
                existing: RemoteOrLocalMeta {
                    kind: ex.kind,
                    size: ex.size,
                    mtime: ex.mtime,
                },
                responder: ConflictResponder::new(tx),
            });
            let answer = tokio::select! {
                _ = job.cancel.cancelled() => return Err(TransferError::Cancelled),
                r = rx => r,
            };
            let set = |s: Sticky| {
                if let Ok(mut g) = job.sticky.lock() {
                    *g = Some(s);
                }
            };
            match answer {
                Ok(ConflictDecision::Overwrite) => Ok(Resolve::Overwrite),
                Ok(ConflictDecision::Skip) => Ok(Resolve::Skip),
                Ok(ConflictDecision::Rename) => Ok(Resolve::Rename),
                Ok(ConflictDecision::OverwriteAll) => {
                    set(Sticky::Overwrite);
                    Ok(Resolve::Overwrite)
                }
                Ok(ConflictDecision::SkipAll) => {
                    set(Sticky::Skip);
                    Ok(Resolve::Skip)
                }
                // Cancelar (ou soltar o responder) cancela a transferência inteira.
                Ok(ConflictDecision::Cancel) | Err(_) => {
                    job.cancel.cancel();
                    Err(TransferError::Cancelled)
                }
            }
        }
    }
}
