//! Painel de arquivos (M3-B): duas colunas (local e remota), fila de transferências, diálogos e
//! arrastar-soltar. A navegação local usa `tokio::fs`; a remota e a fila falam com um
//! [`backend::FilesBackend`] (hoje `MemBackend` nos testes; o motor real é ligado na integração).
//!
//! Nada aqui bloqueia a thread de UI: listagens e operações rodam no runtime e respondem por
//! canal; os eventos da fila são drenados com orçamento por frame (`pump`).

pub mod backend;
mod column;
pub mod contract;
mod dialogs;
pub mod fmt;
mod local;
pub mod model;
pub mod plan;
pub mod queue;
pub mod sync_engine;
pub mod sync_text;
pub mod sync_ui;

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod sync_tests;
#[cfg(test)]
mod ui_tests;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Align2, Id, Order, RichText, Sense, Stroke, StrokeKind, vec2};

use self::backend::{FilesBackend, RemoteFs};
use self::column::{Act, ColCtx, DragPayload, PaneState, column_ui};
use self::contract::*;
use self::dialogs::FsDialog;
use self::model::{Entry, Side, remote_join, remote_parent, validate_name};
use self::queue::{Queue, QueueAction, queue_ui};
use crate::i18n;

/// Eventos da fila aplicados por frame (o resto fica para o próximo; mantém o terminal fluido).
pub const EVENT_BUDGET: usize = 128;
/// Mensagens de listagem/operação aplicadas por frame.
const MSG_BUDGET: usize = 32;

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

enum FsMsg {
    Listed {
        side: Side,
        gen_: u64,
        res: Result<Vec<Entry>, String>,
    },
    Home(Result<String, String>),
    /// Resultado de abrir o SFTP da sessão.
    Backend(Result<Box<dyn FilesBackend>, String>),
    Op {
        side: Side,
        res: Result<(), String>,
    },
}

pub struct FilesPanel {
    uid: u64,
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
    backend: Option<Box<dyn FilesBackend>>,
    remote: Option<Arc<dyn RemoteFs>>,
    opening: bool,
    pub local: PaneState,
    pub remote_pane: PaneState,
    pub queue: Queue,
    pub sync: sync_ui::SyncUi,
    sync_engine: Option<Arc<dyn sync_engine::SyncEngine>>,
    /// Id do perfil e arquivo do journal da sincronização.
    sync_ctx: Option<(String, PathBuf)>,
    /// Política de sobrescrita das próximas transferências.
    pub overwrite: Overwrite,
    tx: Sender<FsMsg>,
    rx: Receiver<FsMsg>,
    pub(crate) dialog: Option<FsDialog>,
    conflict_all: bool,
    pub notice: Option<String>,
    /// Lado em que cada transferência grava (para atualizar a listagem ao terminar).
    dest: HashMap<TransferId, Side>,
    remote_rect: Option<egui::Rect>,
    /// Origem de cada transferência (para mostrar os metadados do arquivo que chegaria num conflito).
    sources: HashMap<TransferId, Entry>,
    /// Por que a coluna remota não está disponível (abrindo, erro, sem SFTP).
    pub remote_status: Option<String>,
    /// Coluna que recebe o foco do teclado no próximo desenho.
    refocus: Option<Side>,
}

impl FilesPanel {
    /// `backend = None`: a coluna remota fica indisponível (só navegação local).
    pub fn new(
        rt: tokio::runtime::Handle,
        ctx: egui::Context,
        backend: Option<Box<dyn FilesBackend>>,
        local_start: PathBuf,
        overwrite: Overwrite,
    ) -> Self {
        let (tx, rx) = channel();
        let remote = backend.as_ref().map(|b| b.remote());
        let mut p = Self {
            uid: NEXT_UID.fetch_add(1, Ordering::Relaxed),
            rt,
            ctx,
            backend,
            remote,
            opening: false,
            local: PaneState::new(Side::Local),
            remote_pane: PaneState::new(Side::Remote),
            queue: Queue::default(),
            sync: sync_ui::SyncUi::default(),
            sync_engine: None,
            sync_ctx: None,
            overwrite,
            tx,
            rx,
            dialog: None,
            conflict_all: false,
            notice: None,
            dest: HashMap::new(),
            remote_rect: None,
            sources: HashMap::new(),
            remote_status: Some(i18n::FILES_REMOTE_UNAVAILABLE().to_string()),
            refocus: None,
        };
        p.navigate(
            Side::Local,
            local_start.to_string_lossy().into_owned(),
            false,
        );
        if let Some(r) = p.remote.clone() {
            p.remote_status = None;
            let (tx, ctx) = (p.tx.clone(), p.ctx.clone());
            p.remote_pane.loading = true;
            p.rt.spawn(async move {
                let res = r.home().await.map_err(|e| e.to_string());
                let _ = tx.send(FsMsg::Home(res));
                ctx.request_repaint();
            });
        }
        p
    }

    /// Já tem backend ou está abrindo um.
    pub fn has_backend(&self) -> bool {
        self.backend.is_some() || self.opening
    }

    /// Abre o SFTP da conexão da sessão (fora da thread de UI) e liga a coluna remota.
    pub fn attach_connection(&mut self, conn: std::sync::Arc<myssh_ssh::Connection>) {
        self.opening = true;
        let (pid, jpath) = self.sync_ctx.clone().unwrap_or_else(|| {
            (
                "sessao".into(),
                std::env::temp_dir().join("myssh-sync.json"),
            )
        });
        self.sync_engine = Some(Arc::new(sync_engine::RealSync {
            conn: conn.clone(),
            profile_id: pid,
            journal_path: jpath,
        }));
        self.remote_status = Some(i18n::FILES_REMOTE_OPENING().to_string());
        let (tx, ctx, rt) = (self.tx.clone(), self.ctx.clone(), self.rt.clone());
        let notify: Notify = {
            let c = self.ctx.clone();
            Arc::new(move || c.request_repaint())
        };
        self.rt.spawn(async move {
            let res = backend::RealBackend::connect(&rt, conn, notify)
                .await
                .map(|b| Box::new(b) as Box<dyn FilesBackend>)
                .map_err(|e| e.to_string());
            let _ = tx.send(FsMsg::Backend(res));
            ctx.request_repaint();
        });
    }

    fn install_backend(&mut self, b: Box<dyn FilesBackend>) {
        let r = b.remote();
        self.backend = Some(b);
        self.remote = Some(r.clone());
        self.remote_status = None;
        self.remote_pane.loading = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let res = r.home().await.map_err(|e| e.to_string());
            let _ = tx.send(FsMsg::Home(res));
            ctx.request_repaint();
        });
    }

    pub fn pane(&self, side: Side) -> &PaneState {
        match side {
            Side::Local => &self.local,
            Side::Remote => &self.remote_pane,
        }
    }

    fn pane_mut(&mut self, side: Side) -> &mut PaneState {
        match side {
            Side::Local => &mut self.local,
            Side::Remote => &mut self.remote_pane,
        }
    }

    /// Transferências ainda não terminadas (segundo o motor).
    pub fn active(&self) -> usize {
        self.backend.as_ref().map_or(0, |b| b.active())
    }

    /// Cancela tudo (fechar aba/painel/app, desconectar). As confirmações vêm antes.
    pub fn cancel_all(&mut self) {
        self.cancel_sync();
        if let Some(b) = &self.backend {
            b.cancel_all();
        }
    }

    pub fn needs_modal(&self) -> bool {
        !self.queue.conflicts.is_empty() || self.dialog.is_some() || self.sync.open
    }

    // --------------------------------------------------------------- navegação

    pub fn navigate(&mut self, side: Side, path: String, record: bool) {
        let gen_ = self.pane_mut(side).begin_nav(path.clone(), record);
        self.spawn_list(side, path, gen_);
    }

    pub fn refresh(&mut self, side: Side) {
        let path = self.pane(side).path.clone();
        if path.is_empty() {
            return;
        }
        let gen_ = self.pane_mut(side).begin_nav(path.clone(), false);
        self.spawn_list(side, path, gen_);
    }

    fn spawn_list(&mut self, side: Side, path: String, gen_: u64) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let fut: std::pin::Pin<Box<dyn Future<Output = Result<Vec<Entry>, String>> + Send>> =
            match side {
                Side::Local => Box::pin(local::list(PathBuf::from(path))),
                Side::Remote => {
                    let Some(r) = self.remote.clone() else {
                        self.remote_pane.loading = false;
                        return;
                    };
                    Box::pin(async move {
                        r.list(&path)
                            .await
                            .map(|v| v.into_iter().map(Entry::from).collect())
                            .map_err(|e| e.to_string())
                    })
                }
            };
        self.rt.spawn(async move {
            let res = fut.await;
            let _ = tx.send(FsMsg::Listed { side, gen_, res });
            ctx.request_repaint();
        });
    }

    fn go_up(&mut self, side: Side) {
        let cur = self.pane(side).path.clone();
        let parent = match side {
            Side::Local => local::parent(&cur),
            Side::Remote => (cur != "/" && !cur.is_empty()).then(|| remote_parent(&cur)),
        };
        if let Some(p) = parent {
            self.navigate(side, p, true);
        }
    }

    fn go_home(&mut self, side: Side) {
        match side {
            Side::Local => {
                let h = local::home_dir().to_string_lossy().into_owned();
                self.navigate(side, h, true);
            }
            Side::Remote => {
                let Some(r) = self.remote.clone() else { return };
                let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
                self.rt.spawn(async move {
                    let res = r.home().await.map_err(|e| e.to_string());
                    let _ = tx.send(FsMsg::Home(res));
                    ctx.request_repaint();
                });
            }
        }
    }

    fn go_back(&mut self, side: Side) {
        if let Some(p) = self.pane_mut(side).pop_back() {
            self.navigate(side, p, false);
        }
    }

    // ------------------------------------------------------------- mensagens

    /// Drena listagens/operações e eventos da fila (com orçamento). `true` = ainda há trabalho
    /// pendente (a UI deve pedir outro frame).
    pub fn pump(&mut self) -> bool {
        self.pump_sync();
        for _ in 0..MSG_BUDGET {
            let Ok(m) = self.rx.try_recv() else { break };
            match m {
                FsMsg::Listed { side, gen_, res } => self.pane_mut(side).on_listed(gen_, res),
                FsMsg::Backend(Ok(b)) => {
                    self.opening = false;
                    self.install_backend(b);
                }
                FsMsg::Backend(Err(e)) => {
                    // `opening` fica ligado: não tenta de novo a cada frame (reabrir o painel tenta).
                    self.remote_status = Some(i18n::files_sftp_failed(&e));
                }
                FsMsg::Home(Ok(h)) => self.navigate(Side::Remote, h, true),
                FsMsg::Home(Err(e)) => {
                    self.remote_pane.loading = false;
                    self.remote_pane.error = Some(e);
                }
                FsMsg::Op { side, res } => {
                    if let Err(e) = res {
                        self.notice = Some(e);
                    }
                    self.refresh(side);
                }
            }
        }
        let mut events = Vec::new();
        if let Some(b) = self.backend.as_mut() {
            while events.len() < EVENT_BUDGET {
                let Some(ev) = b.try_recv() else { break };
                events.push(ev);
            }
        }
        let more = events.len() == EVENT_BUDGET;
        for ev in events {
            let refresh = match &ev {
                TransferEvent::Finished { id, ok, .. } if *ok > 0 => self.dest.get(id).copied(),
                TransferEvent::Cancelled { id } => self.dest.get(id).copied(),
                _ => None,
            };
            let incoming = match &ev {
                TransferEvent::Conflict { id, path, .. } => self.incoming_for(*id, path),
                _ => None,
            };
            let before = self.queue.conflicts.len();
            self.queue.apply(ev);
            if self.queue.conflicts.len() > before
                && let Some(c) = self.queue.conflicts.back_mut()
            {
                c.incoming = incoming;
            }
            if let Some(side) = refresh {
                self.refresh(side);
            }
        }
        if more {
            self.ctx.request_repaint();
        }
        more
    }

    // ---------------------------------------------------------- transferências

    /// Metadados do arquivo que chegaria, quando a origem é um arquivo único da listagem.
    fn incoming_for(&self, id: TransferId, path: &str) -> Option<RemoteOrLocalMeta> {
        let e = self.sources.get(&id)?;
        let same = path == e.name || path.ends_with(&format!("/{}", e.name));
        (e.kind == Kind::File && same).then_some(RemoteOrLocalMeta {
            kind: Kind::File,
            size: e.size,
            mtime: e.mtime,
        })
    }

    fn start_transfer(&mut self, from: Side) {
        self.notice = None;
        let direction = match from {
            Side::Local => Direction::Upload,
            Side::Remote => Direction::Download,
        };
        if self.backend.is_none() {
            self.notice = Some(i18n::FILES_REMOTE_UNAVAILABLE().into());
            return;
        }
        if self.remote_pane.path.is_empty() {
            self.notice = Some(i18n::FILES_NO_REMOTE_DIR().into());
            return;
        }
        let selected: Vec<Entry> = self.pane(from).selected().into_iter().cloned().collect();
        if selected.is_empty() {
            self.notice = Some(i18n::FILES_NOTHING_SELECTED().into());
            return;
        }
        let refs: Vec<&Entry> = selected.iter().collect();
        let reqs = plan::build_requests(
            direction,
            &refs,
            std::path::Path::new(&self.local.path),
            &self.remote_pane.path,
            self.overwrite,
        );
        self.enqueue_all(
            reqs,
            from.other(),
            selected
                .into_iter()
                .filter(|e| e.kind != Kind::Other)
                .collect(),
        );
    }

    fn enqueue_all(&mut self, reqs: Vec<TransferRequest>, dest: Side, srcs: Vec<Entry>) {
        let mut sources = srcs.into_iter().map(Some).chain(std::iter::repeat(None));
        let Some(b) = &self.backend else { return };
        for r in reqs {
            let src = sources.next().flatten();
            let id = b.enqueue(r);
            self.dest.insert(id, dest);
            if let Some(e) = src {
                self.sources.insert(id, e);
            }
        }
    }

    /// Arquivos soltos do SO sobre a coluna remota.
    fn upload_dropped(&mut self, paths: Vec<PathBuf>) {
        self.notice = None;
        if self.backend.is_none() {
            self.notice = Some(i18n::FILES_REMOTE_UNAVAILABLE().into());
            return;
        }
        if self.remote_pane.path.is_empty() {
            self.notice = Some(i18n::FILES_NO_REMOTE_DIR().into());
            return;
        }
        let reqs = plan::requests_for_drop(&paths, &self.remote_pane.path, self.overwrite);
        self.enqueue_all(reqs, Side::Remote, Vec::new());
    }

    // ------------------------------------------------------------- ações

    fn handle(&mut self, act: Act) {
        match act {
            Act::Navigate(s, p) => self.navigate(s, p, true),
            Act::Back(s) => self.go_back(s),
            Act::Up(s) => self.go_up(s),
            Act::Home(s) => self.go_home(s),
            Act::Refresh(s) => self.refresh(s),
            Act::Open(s, p) => self.navigate(s, p, true),
            Act::Transfer(s) => self.start_transfer(s),
            Act::Sync => self.open_sync(),
            Act::NewDir(s) => self.dialog = Some(FsDialog::new_dir(s)),
            Act::Rename(s) => {
                let sel = self.pane(s).selected();
                if let [e] = sel.as_slice() {
                    let d = FsDialog::rename(s, e);
                    self.dialog = Some(d);
                }
            }
            Act::Delete(s) => {
                let items: Vec<Entry> = self.pane(s).selected().into_iter().cloned().collect();
                if !items.is_empty() {
                    self.dialog = Some(FsDialog::delete(s, items));
                }
            }
        }
    }

    fn spawn_op(&self, side: Side, fut: impl Future<Output = Result<(), String>> + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let res = fut.await;
            let _ = tx.send(FsMsg::Op { side, res });
            ctx.request_repaint();
        });
    }

    fn do_mkdir(&mut self, side: Side, name: &str) {
        let dir = self.pane(side).path.clone();
        match side {
            Side::Local => {
                self.spawn_op(side, local::mkdir(PathBuf::from(local::join(&dir, name))))
            }
            Side::Remote => {
                let Some(r) = self.remote.clone() else { return };
                let path = remote_join(&dir, name);
                self.spawn_op(side, async move {
                    r.mkdir(&path).await.map_err(|e| e.to_string())
                });
            }
        }
    }

    fn do_rename(&mut self, side: Side, from: &str, new_name: &str) {
        match side {
            Side::Local => {
                let parent = local::parent(from).unwrap_or_default();
                let to = local::join(&parent, new_name);
                self.spawn_op(side, local::rename(PathBuf::from(from), PathBuf::from(to)));
            }
            Side::Remote => {
                let Some(r) = self.remote.clone() else { return };
                let to = remote_join(&remote_parent(from), new_name);
                let from = from.to_string();
                self.spawn_op(side, async move {
                    r.rename(&from, &to).await.map_err(|e| e.to_string())
                });
            }
        }
    }

    fn do_delete(&mut self, side: Side, items: Vec<Entry>) {
        match side {
            Side::Local => {
                self.spawn_op(side, async move {
                    let mut first_err = None;
                    for e in items {
                        if let Err(m) = local::remove(PathBuf::from(&e.path)).await {
                            first_err.get_or_insert(format!("{}: {m}", e.name));
                        }
                    }
                    first_err.map_or(Ok(()), Err)
                });
            }
            Side::Remote => {
                let Some(r) = self.remote.clone() else { return };
                self.spawn_op(side, async move {
                    let mut first_err = None;
                    for e in items {
                        if let Err(m) = r.remove(&e.path, e.is_dir()).await {
                            first_err.get_or_insert(format!("{}: {}", e.name, m));
                        }
                    }
                    first_err.map_or(Ok(()), Err)
                });
            }
        }
    }

    // ------------------------------------------------------------------ UI

    /// Desenha o painel. Devolve `true` quando o usuário pediu para fechá-lo.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut close = false;
        let ctx = ui.ctx().clone();
        let mut acts: Vec<Act> = Vec::new();
        let keys = !self.needs_modal();

        ui.horizontal(|ui| {
            ui.label(RichText::new(i18n::FILES_TITLE()).strong());
            ui.label(i18n::FILES_POLICY_LABEL());
            egui::ComboBox::from_id_salt((self.uid, "policy"))
                .selected_text(plan::overwrite_label(self.overwrite))
                .show_ui(ui, |ui| {
                    for p in plan::POLICIES {
                        ui.selectable_value(&mut self.overwrite, p, plan::overwrite_label(p));
                    }
                })
                .response
                .on_hover_text(i18n::FILES_POLICY_HINT());
            if let Some(n) = &self.notice {
                ui.colored_label(crate::view::warn_color(ui), n);
            }
            if self.sync.running() && !self.sync.open {
                ui.label(i18n::SYNC_RUNNING_HIDDEN());
                if ui.button(i18n::SYNC_SHOW()).clicked() {
                    self.sync.open = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(i18n::FILES_CLOSE()).clicked() {
                    close = true;
                }
            });
        });
        ui.separator();

        let avail = ui.available_rect_before_wrap();
        let queue_h = if self.queue.items.is_empty() {
            0.0
        } else {
            (avail.height() * 0.45)
                .clamp(96.0, 300.0)
                .min(avail.height() * 0.6)
        };
        let cols_rect = egui::Rect::from_min_max(
            avail.min,
            egui::pos2(avail.right(), avail.bottom() - queue_h),
        );
        let mid_w = 104.0;
        let col_w = ((cols_rect.width() - mid_w) / 2.0 - 8.0).max(120.0);
        let left = egui::Rect::from_min_size(cols_rect.min, vec2(col_w, cols_rect.height()));
        let mid = egui::Rect::from_min_size(
            egui::pos2(left.right() + 4.0, cols_rect.top()),
            vec2(mid_w, cols_rect.height()),
        );
        let right = egui::Rect::from_min_size(
            egui::pos2(mid.right() + 4.0, cols_rect.top()),
            vec2(col_w, cols_rect.height()),
        );
        self.remote_rect = Some(right);

        let remote_ok = self.backend.is_some();
        let remote_ready = remote_ok && !self.remote_pane.path.is_empty();
        let can_up = remote_ready && !self.local.selected().is_empty();
        let can_down = remote_ready && !self.remote_pane.selected().is_empty();
        let uid = self.uid;
        let refocus = self.refocus.take();

        let sub = |ui: &mut egui::Ui, rect: egui::Rect, f: &mut dyn FnMut(&mut egui::Ui)| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.set_clip_rect(rect.intersect(ui.clip_rect()));
                f(ui);
            });
        };
        sub(ui, left, &mut |ui| {
            let cx = ColCtx {
                uid,
                label: i18n::FILES_LOCAL(),
                unavailable: None,
                keys,
                can_transfer: can_up,
                focus_request: refocus == Some(Side::Local),
            };
            column_ui(ui, &mut self.local, &cx, &mut acts);
        });
        sub(ui, mid, &mut |ui| {
            ui.add_space(70.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add_enabled(can_up, egui::Button::new(i18n::FILES_UPLOAD()))
                    .on_hover_text(i18n::FILES_UPLOAD_HINT())
                    .clicked()
                {
                    acts.push(Act::Transfer(Side::Local));
                }
                ui.add_space(6.0);
                if ui
                    .add_enabled(can_down, egui::Button::new(i18n::FILES_DOWNLOAD()))
                    .on_hover_text(i18n::FILES_DOWNLOAD_HINT())
                    .clicked()
                {
                    acts.push(Act::Transfer(Side::Remote));
                }
                ui.add_space(16.0);
                if ui
                    .add_enabled(remote_ready, egui::Button::new(i18n::FILES_SYNC()))
                    .on_hover_text(i18n::FILES_SYNC_HINT())
                    .clicked()
                {
                    acts.push(Act::Sync);
                }
            });
        });
        sub(ui, right, &mut |ui| {
            let cx = ColCtx {
                uid,
                label: i18n::FILES_REMOTE(),
                unavailable: self.remote_status.as_deref(),
                keys,
                can_transfer: can_down,
                focus_request: refocus == Some(Side::Remote),
            };
            column_ui(ui, &mut self.remote_pane, &cx, &mut acts);
        });

        // Arrastar-soltar entre as colunas.
        for (side, rect) in [(Side::Local, left), (Side::Remote, right)] {
            let z = ui.interact(rect, Id::new((self.uid, side, "dnd")), Sense::hover());
            if let Some(p) = z.dnd_hover_payload::<DragPayload>()
                && p.from != side
            {
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(2.0, ui.visuals().selection.stroke.color),
                    StrokeKind::Inside,
                );
            }
            if let Some(p) = z.dnd_release_payload::<DragPayload>()
                && p.from != side
            {
                acts.push(Act::Transfer(p.from));
            }
        }
        self.drag_ghost(&ctx);
        self.os_drops(ui, right);

        // Fila.
        if queue_h > 0.0 {
            let qrect = egui::Rect::from_min_max(
                egui::pos2(avail.left(), avail.bottom() - queue_h + 4.0),
                avail.max,
            );
            let mut qa = None;
            ui.scope_builder(egui::UiBuilder::new().max_rect(qrect), |ui| {
                ui.set_clip_rect(qrect.intersect(ui.clip_rect()));
                ui.separator();
                qa = queue_ui(ui, &mut self.queue);
            });
            match qa {
                Some(QueueAction::Cancel(id)) => {
                    if let Some(b) = &self.backend {
                        b.cancel(id);
                    }
                }
                Some(QueueAction::CancelAll) => self.cancel_all(),
                Some(QueueAction::ClearFinished) => self.queue.clear_finished(),
                None => {}
            }
        }
        ui.allocate_rect(avail, Sense::hover());

        for a in acts {
            self.handle(a);
        }
        close
    }

    fn drag_ghost(&self, ctx: &egui::Context) {
        let Some(p) = egui::DragAndDrop::payload::<DragPayload>(ctx) else {
            return;
        };
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        let n = self.pane(p.from).selected().len();
        egui::Area::new(Id::new((self.uid, "ghost")))
            .order(Order::Tooltip)
            .fixed_pos(pos + vec2(14.0, 14.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(if n == 1 {
                        "1 item".to_string()
                    } else {
                        format!("{n} itens")
                    });
                });
            });
    }

    /// Arquivos do SO: realce ao pairar sobre a coluna remota e upload ao soltar.
    fn os_drops(&mut self, ui: &mut egui::Ui, remote_rect: egui::Rect) {
        let (hovering, dropped, pos) = ui.ctx().input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>(),
                i.pointer.latest_pos(),
            )
        });
        let over = pos.is_none_or(|p| remote_rect.contains(p));
        if hovering && over {
            let c = ui.visuals().selection.stroke.color;
            ui.painter()
                .rect_filled(remote_rect, 4.0, c.gamma_multiply(0.12));
            ui.painter()
                .rect_stroke(remote_rect, 4.0, Stroke::new(2.0, c), StrokeKind::Inside);
            ui.painter().text(
                remote_rect.center(),
                Align2::CENTER_CENTER,
                i18n::FILES_DROP_HINT(),
                egui::FontId::proportional(15.0),
                ui.visuals().strong_text_color(),
            );
        }
        if !dropped.is_empty() && over {
            self.upload_dropped(dropped);
        }
    }

    /// Diálogos modais (conflito primeiro).
    pub fn modal_ui(&mut self, ctx: &egui::Context) {
        if let Some(c) = self.queue.conflicts.front() {
            let pending = self.queue.conflicts.len();
            if let Some(choice) = dialogs::conflict_dialog(ctx, c, pending, &mut self.conflict_all)
            {
                let (d, sticky) = plan::resolve_conflict(choice, self.conflict_all);
                self.queue.answer_front(d, sticky);
                self.conflict_all = false;
            }
            return;
        }
        if self.sync.open && self.dialog.is_none() {
            self.sync_dialog(ctx);
            return;
        }
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let side = d.side();
        let names: Vec<String> = self
            .pane(side)
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect();
        let result = dialogs::fs_dialog(ctx, &mut d, &names);
        if !matches!(result, dialogs::DialogResult::Keep) {
            self.refocus = Some(side);
        }
        match result {
            dialogs::DialogResult::Keep => self.dialog = Some(d),
            dialogs::DialogResult::Close => {}
            dialogs::DialogResult::Mkdir(side, name) => self.do_mkdir(side, &name),
            dialogs::DialogResult::Rename(side, from, name) => self.do_rename(side, &from, &name),
            dialogs::DialogResult::Delete(side, items) => self.do_delete(side, items),
        }
    }

    #[cfg(test)]
    fn handle_for_test(&mut self, act: Act) {
        self.handle(act);
    }
}

/// Painel de uma sessão: a coluna local funciona de imediato; a remota aparece quando o SFTP abre.
pub fn new_for_session(
    env: &crate::tabs::TabEnv,
    conn: Option<Arc<myssh_ssh::Connection>>,
    profile_id: &str,
) -> FilesPanel {
    let mut p = FilesPanel::new(
        env.rt.clone(),
        env.ctx.clone(),
        None,
        local::home_dir(),
        env.default_overwrite,
    );
    // journal por perfil no diretório de dados (sem hostname nem segredos)
    p.sync_ctx = env.data_dir.as_ref().map(|d| {
        (
            profile_id.to_string(),
            d.join("sync").join(format!("{profile_id}.json")),
        )
    });
    match conn {
        Some(c) => p.attach_connection(c),
        None => p.remote_status = Some(i18n::FILES_NO_CONNECTION().to_string()),
    }
    p
}

/// Valida um nome contra a pasta atual (mensagem para o diálogo).
pub(crate) fn name_problem(
    name: &str,
    existing: &[String],
    same_as: Option<&str>,
) -> Option<String> {
    if let Err(m) = validate_name(name) {
        return Some(m.to_string());
    }
    let n = name.trim();
    if Some(n) != same_as && existing.iter().any(|e| e == n) {
        return Some(i18n::FILES_ERR_EXISTS().to_string());
    }
    None
}

/// Painel com `MemBackend` para testes em outros módulos.
#[cfg(test)]
pub(crate) fn test_panel(
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
) -> (FilesPanel, backend::MemHandle) {
    let c = ctx.clone();
    let (be, mem) = backend::MemBackend::new(Arc::new(move || c.request_repaint()));
    let p = FilesPanel::new(
        rt,
        ctx,
        Some(Box::new(be)),
        std::env::temp_dir(),
        Overwrite::Ask,
    );
    (p, mem)
}

#[cfg(test)]
impl FilesPanel {
    /// Testes: troca o executor de sincronização.
    pub(crate) fn set_sync_engine(&mut self, e: Arc<dyn sync_engine::SyncEngine>) {
        self.sync_engine = Some(e);
    }
}
