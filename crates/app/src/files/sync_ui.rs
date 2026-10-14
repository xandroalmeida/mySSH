//! Diálogo de sincronização: opções, prévia (plano), execução, PlanStale e retomada.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, RichText};
use myssh_syncrun::myssh_sync::{
    Action, Changed, Direction, Journal, Plan, RelPath, Side as PSide,
};
use myssh_syncrun::{ConflictChoice, RunSummary, SyncError, SyncEvent, SyncState};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::FilesPanel;
use super::fmt::{fmt_size, percent};
use super::model::Side;
use super::sync_engine::SyncRequest;
use super::sync_text::{self as txt, Filter};
use crate::i18n;

/// Mensagens processadas por frame.
const MSG_BUDGET: usize = 64;

pub enum Phase {
    Options,
    Planning {
        done: usize,
        total: usize,
        current: String,
    },
    Preview(Arc<Plan>),
    Running,
    Finished(RunSummary),
    Stale(Vec<Changed>),
    Failed(String),
}

pub enum SyncMsg {
    Plan {
        gen_: u64,
        res: Result<Plan, SyncError>,
    },
    Event {
        gen_: u64,
        ev: SyncEvent,
    },
    Done {
        gen_: u64,
        res: Result<Journal, SyncError>,
    },
    Resume(Option<SyncState>),
}

#[derive(Default, Clone)]
pub struct Progress {
    pub items_done: usize,
    pub items_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

pub struct ResumeInfo {
    pub state: SyncState,
    pub remaining: usize,
}

pub struct SyncUi {
    pub open: bool,
    pub phase: Phase,
    pub direction: Direction,
    pub content: bool,
    pub mirror: bool,
    pub excl_text: String,
    pub filter: Filter,
    pub query: String,
    pub choices: HashMap<RelPath, ConflictChoice>,
    pub confirm_deletes: bool,
    pub resume: Option<ResumeInfo>,
    /// Retomando: journal carregado do disco (o executor refaz só o que falta).
    resume_journal: Option<Journal>,
    pub req: Option<SyncRequest>,
    pub local_root: PathBuf,
    pub remote_root: String,
    pub progress: Progress,
    pub failures: Vec<(String, String)>,
    pub skipped_items: usize,
    pub warnings: Vec<String>,
    last_summary: Option<RunSummary>,
    gen_: u64,
    cancel: Option<CancellationToken>,
    pub error: Option<String>,
    tx: Sender<SyncMsg>,
    rx: Receiver<SyncMsg>,
}

impl Default for SyncUi {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            open: false,
            phase: Phase::Options,
            direction: Direction::LocalToRemote,
            content: false,
            mirror: false,
            excl_text: ".git/".into(),
            filter: Filter::All,
            query: String::new(),
            choices: HashMap::new(),
            confirm_deletes: false,
            resume: None,
            resume_journal: None,
            req: None,
            local_root: PathBuf::new(),
            remote_root: String::new(),
            progress: Progress::default(),
            failures: Vec::new(),
            skipped_items: 0,
            warnings: Vec::new(),
            last_summary: None,
            gen_: 0,
            cancel: None,
            error: None,
            tx,
            rx,
        }
    }
}

impl SyncUi {
    /// Identifica a fase (estado de tamanho do diálogo não vaza de uma fase para outra).
    fn phase_tag(&self) -> u8 {
        match self.phase {
            Phase::Options => 0,
            Phase::Planning { .. } => 1,
            Phase::Preview(_) => 2,
            Phase::Running => 3,
            Phase::Finished(_) => 4,
            Phase::Stale(_) => 5,
            Phase::Failed(_) => 6,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }

    fn reset_run(&mut self) {
        self.choices.clear();
        self.confirm_deletes = false;
        self.filter = Filter::All;
        self.query.clear();
        self.progress = Progress::default();
        self.failures.clear();
        self.skipped_items = 0;
        self.warnings.clear();
        self.last_summary = None;
        self.resume_journal = None;
        self.error = None;
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Ask {
    Plan,
    Execute,
    BackToOptions,
    Close,
    Cancel,
    Hide,
    Resume,
    Discard,
}

impl FilesPanel {
    /// "Sincronizar…": raízes = pastas atuais das duas colunas.
    pub fn open_sync(&mut self) {
        self.notice = None;
        if self.sync_engine.is_none() {
            self.notice = Some(i18n::SYNC_UNAVAILABLE.into());
            return;
        }
        if self.remote_pane.path.is_empty() {
            self.notice = Some(i18n::FILES_NO_REMOTE_DIR.into());
            return;
        }
        self.sync.open = true;
        if self.sync.running() {
            return;
        }
        self.sync.phase = Phase::Options;
        self.sync.reset_run();
        self.sync.resume = None;
        self.sync.local_root = PathBuf::from(&self.local.path);
        self.sync.remote_root = self.remote_pane.path.clone();
        // há sincronização interrompida?
        if let Some(e) = self.sync_engine.clone() {
            let (tx, ctx) = (self.sync.tx.clone(), self.ctx.clone());
            self.rt.spawn(async move {
                let st = e.load_state().await;
                let _ = tx.send(SyncMsg::Resume(st));
                ctx.request_repaint();
            });
        }
    }

    pub fn sync_running(&self) -> bool {
        self.sync.running()
    }

    /// Cancela a execução em curso (fechar aba/app).
    pub(super) fn cancel_sync(&mut self) {
        if let Some(c) = &self.sync.cancel {
            c.cancel();
        }
    }

    pub(super) fn pump_sync(&mut self) {
        for _ in 0..MSG_BUDGET {
            let Ok(m) = self.sync.rx.try_recv() else {
                break;
            };
            match m {
                SyncMsg::Resume(st) => {
                    self.sync.resume = st
                        .map(|state| {
                            let remaining = state.journal.remaining().count();
                            ResumeInfo { state, remaining }
                        })
                        .filter(|r| r.remaining > 0);
                }
                SyncMsg::Plan { gen_, res } if gen_ == self.sync.gen_ => match res {
                    Ok(plan) => {
                        self.sync.phase = Phase::Preview(Arc::new(plan));
                        self.sync.filter = Filter::All;
                    }
                    Err(e) => self.sync.phase = Phase::Failed(e.to_string()),
                },
                SyncMsg::Event { gen_, ev } if gen_ == self.sync.gen_ => self.on_sync_event(ev),
                SyncMsg::Done { gen_, res } if gen_ == self.sync.gen_ => match res {
                    Ok(_) => {
                        let s = self.sync.last_summary.take().unwrap_or_default();
                        self.sync.phase = Phase::Finished(s);
                        self.sync.cancel = None;
                        // terminou: atualiza o lado do destino
                        let dest = match self.sync.req.as_ref().map(|r| r.direction) {
                            Some(Direction::RemoteToLocal) => Side::Local,
                            _ => Side::Remote,
                        };
                        self.refresh(dest);
                    }
                    Err(SyncError::PlanStale(c)) => {
                        self.sync.phase = Phase::Stale(c);
                        self.sync.cancel = None;
                    }
                    Err(e) => {
                        self.sync.phase = Phase::Failed(e.to_string());
                        self.sync.cancel = None;
                    }
                },
                _ => {} // resposta de uma geração antiga (prévia abandonada)
            }
        }
    }

    pub(super) fn on_sync_event(&mut self, ev: SyncEvent) {
        match ev {
            SyncEvent::Hashing {
                done,
                total,
                current,
            } => {
                if let Phase::Planning { .. } = self.sync.phase {
                    self.sync.phase = Phase::Planning {
                        done,
                        total,
                        current,
                    };
                }
            }
            SyncEvent::Progress {
                items_done,
                items_total,
                bytes_done,
                bytes_total,
                current,
            } => {
                self.sync.progress = Progress {
                    items_done,
                    items_total,
                    bytes_done,
                    bytes_total,
                    current,
                };
            }
            SyncEvent::ItemFailed { path, message, .. } => {
                if self.sync.failures.len() < 500 {
                    self.sync.failures.push((path.to_string(), message));
                }
            }
            SyncEvent::ItemSkipped { .. } => self.sync.skipped_items += 1,
            SyncEvent::Warning { message } => {
                if self.sync.warnings.len() < 50 {
                    self.sync.warnings.push(message);
                }
            }
            SyncEvent::Finished(s) => {
                // O resumo pode chegar depois do retorno da execução (canais distintos).
                if matches!(self.sync.phase, Phase::Finished(_)) {
                    self.sync.phase = Phase::Finished(s);
                }
                self.sync.last_summary = Some(s);
            }
            SyncEvent::ItemDone { .. } => {}
        }
    }

    fn sync_request(&self) -> Result<SyncRequest, String> {
        let s = &self.sync;
        txt::parse_exclusions(&s.excl_text)?;
        Ok(SyncRequest {
            direction: s.direction,
            content: s.content,
            mirror: s.mirror,
            exclusions: txt::exclusion_patterns(&s.excl_text),
            local_root: s.local_root.clone(),
            remote_root: s.remote_root.clone(),
        })
    }

    /// Canal de eventos do executor → fila de mensagens da UI (com repaint).
    fn sync_events(&self, gen_: u64) -> mpsc::Sender<SyncEvent> {
        let (etx, mut erx) = mpsc::channel::<SyncEvent>(256);
        let (tx, ctx) = (self.sync.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            while let Some(ev) = erx.recv().await {
                if tx.send(SyncMsg::Event { gen_, ev }).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        etx
    }

    pub(super) fn start_plan(&mut self) {
        let Some(engine) = self.sync_engine.clone() else {
            return;
        };
        let req = match self.sync_request() {
            Ok(r) => r,
            Err(m) => {
                self.sync.error = Some(m);
                return;
            }
        };
        self.sync.error = None;
        self.sync.reset_run();
        self.sync.gen_ += 1;
        let gen_ = self.sync.gen_;
        self.sync.req = Some(req.clone());
        self.sync.phase = Phase::Planning {
            done: 0,
            total: 0,
            current: String::new(),
        };
        let events = self.sync_events(gen_);
        let (tx, ctx) = (self.sync.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let res = engine.plan(req, events).await;
            let _ = tx.send(SyncMsg::Plan { gen_, res });
            ctx.request_repaint();
        });
    }

    fn start_execute(&mut self) {
        let Some(engine) = self.sync_engine.clone() else {
            return;
        };
        let (Phase::Preview(plan), Some(req)) = (&self.sync.phase, self.sync.req.clone()) else {
            return;
        };
        let plan = plan.clone();
        let decisions = txt::build_decisions(&plan, &self.sync.choices, self.sync.confirm_deletes);
        if !txt::can_execute(&plan, self.sync.confirm_deletes, &decisions) {
            return;
        }
        let resume = self.sync.resume_journal.take();
        self.sync.gen_ += 1;
        let gen_ = self.sync.gen_;
        self.sync.progress = Progress::default();
        self.sync.failures.clear();
        self.sync.skipped_items = 0;
        self.sync.last_summary = None;
        self.sync.phase = Phase::Running;
        let cancel = CancellationToken::new();
        self.sync.cancel = Some(cancel.clone());
        let events = self.sync_events(gen_);
        let (tx, ctx) = (self.sync.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let res = engine
                .execute(req, plan, decisions, resume, events, cancel)
                .await;
            let _ = tx.send(SyncMsg::Done { gen_, res });
            ctx.request_repaint();
        });
    }

    /// Usa o estado interrompido como prévia: o usuário decide de novo e o executor refaz só o
    /// que falta (revalidando antes).
    fn start_resume(&mut self) {
        let Some(r) = self.sync.resume.take() else {
            return;
        };
        let st = r.state;
        self.sync.reset_run();
        self.sync.direction = st.plan.direction;
        self.sync.mirror = st.plan.mirror_deletes;
        self.sync.req = Some(SyncRequest {
            direction: st.plan.direction,
            content: false,
            mirror: st.plan.mirror_deletes,
            exclusions: txt::exclusion_patterns(&self.sync.excl_text),
            local_root: self.sync.local_root.clone(),
            remote_root: self.sync.remote_root.clone(),
        });
        // Só o que falta aparece na prévia.
        let remaining: std::collections::HashSet<RelPath> =
            st.journal.remaining().map(|i| i.path.clone()).collect();
        let plan = Plan {
            direction: st.plan.direction,
            mirror_deletes: st.plan.mirror_deletes,
            items: st
                .plan
                .items
                .into_iter()
                .filter(|i| remaining.contains(&i.path))
                .collect(),
        };
        self.sync.resume_journal = Some(st.journal);
        self.sync.phase = Phase::Preview(Arc::new(plan));
    }

    fn discard_resume(&mut self) {
        self.sync.resume = None;
        if let Some(e) = self.sync_engine.clone() {
            self.rt.spawn(async move { e.discard_state().await });
        }
    }

    fn apply_sync(&mut self, a: Ask) {
        match a {
            Ask::Plan => self.start_plan(),
            Ask::Execute => self.start_execute(),
            Ask::BackToOptions => {
                self.sync.phase = Phase::Options;
                self.sync.resume_journal = None;
            }
            Ask::Cancel => self.cancel_sync(),
            Ask::Hide => self.sync.open = false,
            Ask::Resume => self.start_resume(),
            Ask::Discard => self.discard_resume(),
            Ask::Close => {
                self.sync.open = false;
                if !self.sync.running() {
                    // abandona prévia/resultado (respostas atrasadas são ignoradas)
                    self.sync.gen_ += 1;
                    self.sync.phase = Phase::Options;
                }
            }
        }
    }

    pub(super) fn sync_dialog(&mut self, ctx: &egui::Context) {
        let mut ask: Option<Ask> = None;
        let max_h = (ctx.content_rect().height() - 120.0).max(320.0);
        let resp = egui::Modal::new(egui::Id::new((
            self.uid,
            "sync-dialog",
            self.sync.phase_tag(),
        )))
        .show(ctx, |ui| {
            ui.set_width(800.0);
            ui.heading(i18n::SYNC_TITLE);
            ui.add_space(4.0);
            // rodapé fixo (botões) + corpo rolável acima dele
            let footer_h = 34.0;
            egui::ScrollArea::vertical()
                .max_height((max_h - footer_h).max(200.0))
                .auto_shrink([false, true])
                .id_salt(("sync-body", self.sync.phase_tag()))
                .show(ui, |ui| self.sync_body(ui, &mut ask));
            ui.add_space(6.0);
            ui.separator();
            self.sync_footer(ui, &mut ask);
        });
        if resp.should_close() && ask.is_none() && !self.sync.running() {
            ask = Some(Ask::Close);
        } else if resp.should_close() && self.sync.running() {
            ask = Some(Ask::Hide);
        }
        if let Some(a) = ask {
            self.apply_sync(a);
        }
    }

    fn sync_body(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        match &self.sync.phase {
            Phase::Options => self.options_ui(ui, ask),
            Phase::Planning { .. } => self.planning_ui(ui, ask),
            Phase::Preview(_) => self.preview_ui(ui, ask),
            Phase::Running => self.running_ui(ui, ask),
            Phase::Finished(_) => self.finished_ui(ui, ask),
            Phase::Stale(_) => self.stale_ui(ui, ask),
            Phase::Failed(_) => self.failed_ui(ui, ask),
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let red = err_color(ui);
        egui::Grid::new("sync-roots").num_columns(2).show(ui, |ui| {
            ui.label(i18n::FILES_LOCAL);
            ui.label(RichText::new(self.sync.local_root.display().to_string()).monospace());
            ui.end_row();
            ui.label(i18n::FILES_REMOTE);
            ui.label(RichText::new(&self.sync.remote_root).monospace());
            ui.end_row();
        });
        ui.add_space(8.0);
        if let Some(r) = &self.sync.resume {
            let msg = i18n::sync_resume_found(r.remaining);
            let mut resume = false;
            let mut discard = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(msg).strong());
                ui.weak(i18n::SYNC_RESUME_HELP);
                ui.horizontal(|ui| {
                    resume = ui.button(i18n::SYNC_RESUME).clicked();
                    discard = ui.button(i18n::SYNC_DISCARD).clicked();
                });
            });
            if resume {
                *ask = Some(Ask::Resume);
            } else if discard {
                *ask = Some(Ask::Discard);
            }
            ui.add_space(8.0);
        }
        ui.label(RichText::new(i18n::SYNC_DIRECTION).strong());
        ui.radio_value(
            &mut self.sync.direction,
            Direction::LocalToRemote,
            i18n::SYNC_L2R,
        );
        ui.radio_value(
            &mut self.sync.direction,
            Direction::RemoteToLocal,
            i18n::SYNC_R2L,
        );
        let dest = match self.sync.direction {
            Direction::LocalToRemote => i18n::FILES_REMOTE,
            Direction::RemoteToLocal => i18n::FILES_LOCAL,
        };
        ui.colored_label(warn(ui), i18n::sync_dest_changes(dest));
        ui.add_space(8.0);
        ui.label(RichText::new(i18n::SYNC_COMPARE).strong());
        ui.radio_value(&mut self.sync.content, false, i18n::SYNC_COMPARE_FAST);
        ui.radio_value(&mut self.sync.content, true, i18n::SYNC_COMPARE_CONTENT);
        if self.sync.content {
            ui.colored_label(warn(ui), i18n::SYNC_CONTENT_COST);
        }
        ui.add_space(8.0);
        ui.label(RichText::new(i18n::SYNC_EXCLUDES).strong());
        ui.weak(i18n::SYNC_EXCLUDES_HELP);
        ui.add(
            egui::TextEdit::multiline(&mut self.sync.excl_text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        )
        .widget_info(|| super::column::text_info(i18n::SYNC_EXCLUDES, &self.sync.excl_text));
        let bad = txt::parse_exclusions(&self.sync.excl_text).err();
        if let Some(m) = &bad {
            ui.colored_label(red, m);
        }
        ui.add_space(8.0);
        ui.checkbox(&mut self.sync.mirror, i18n::SYNC_MIRROR);
        if self.sync.mirror {
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.5, red))
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.colored_label(red, i18n::SYNC_MIRROR_WARNING);
                });
        }
        if let Some(e) = &self.sync.error {
            ui.colored_label(red, e);
        }
        let _ = ask;
    }

    fn planning_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let Phase::Planning {
            done,
            total,
            current,
        } = &self.sync.phase
        else {
            return;
        };
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(i18n::SYNC_PLANNING);
        });
        if *total > 0 {
            let frac = percent(*done as u64, *total as u64) / 100.0;
            ui.add(
                egui::ProgressBar::new(frac)
                    .text(i18n::sync_hashing(*done, *total))
                    .desired_height(16.0),
            );
            ui.add(egui::Label::new(RichText::new(current).weak().small()).truncate());
        }
        let _ = ask;
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let Phase::Preview(plan) = &self.sync.phase else {
            return;
        };
        let plan = plan.clone();
        let s = plan.summary();
        let n_del = txt::delete_count(&plan);
        let resuming = self.sync.resume_journal.is_some();
        ui.label(RichText::new(i18n::sync_summary(
            s.create,
            s.update,
            s.conflict,
            s.delete,
            s.skip,
            &fmt_size(s.bytes_to_transfer),
        )));
        ui.weak(i18n::sync_direction_line(matches!(
            plan.direction,
            Direction::LocalToRemote
        )));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for f in Filter::ALL {
                if f == Filter::Delete && n_del == 0 {
                    continue; // sem espelho a seção nem aparece
                }
                let n = if f == Filter::All {
                    plan.items.len()
                } else {
                    txt::count(&plan, f)
                };
                let label = format!("{} ({n})", f.label());
                if ui.selectable_label(self.sync.filter == f, label).clicked() {
                    self.sync.filter = f;
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.sync.query)
                    .hint_text(i18n::SYNC_SEARCH)
                    .desired_width(180.0),
            );
        });
        ui.add_space(4.0);
        let idx = txt::filter_items(&plan, self.sync.filter, &self.sync.query);
        let red = err_color(ui);
        let row_h = 22.0;
        egui::ScrollArea::vertical()
            .id_salt("sync-items")
            .max_height(260.0)
            .auto_shrink([false, false])
            .show_rows(ui, row_h, idx.len(), |ui, range| {
                for n in range {
                    let it = &plan.items[idx[n]];
                    ui.horizontal(|ui| {
                        ui.set_height(row_h);
                        let col = match it.action {
                            Action::Delete => red,
                            Action::Conflict { .. } => warn(ui),
                            _ => ui.visuals().text_color(),
                        };
                        ui.add_sized(
                            [70.0, row_h],
                            egui::Label::new(
                                RichText::new(txt::action_label(&it.action)).color(col),
                            ),
                        );
                        let name = if it.kind == myssh_syncrun::myssh_sync::EntryKind::Dir {
                            format!("{}/", it.path)
                        } else {
                            it.path.to_string()
                        };
                        ui.add_sized(
                            [260.0, row_h],
                            egui::Label::new(RichText::new(name).monospace()).truncate(),
                        );
                        if txt::conflict_resolvable(it) {
                            let cur = self
                                .sync
                                .choices
                                .get(&it.path)
                                .copied()
                                .unwrap_or(ConflictChoice::Skip);
                            let mut c = cur;
                            ui.selectable_value(&mut c, ConflictChoice::Skip, i18n::SYNC_SKIP);
                            ui.selectable_value(
                                &mut c,
                                ConflictChoice::UseSource,
                                i18n::SYNC_USE_SOURCE,
                            );
                            if c != cur {
                                self.sync.choices.insert(it.path.clone(), c);
                            }
                        } else if matches!(it.action, Action::Conflict { .. }) {
                            ui.weak(i18n::SYNC_NOT_RESOLVABLE);
                        }
                        let reason = txt::reason_text(&it.action);
                        ui.add(egui::Label::new(RichText::new(&reason).weak()).truncate())
                            .on_hover_text(reason);
                    });
                }
            });
        if idx.is_empty() {
            ui.weak(i18n::SYNC_NO_ITEMS);
        }
        ui.add_space(6.0);
        if n_del > 0 {
            egui::Frame::new()
                .stroke(egui::Stroke::new(2.0, red))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(
                        red,
                        RichText::new(i18n::sync_delete_section(n_del)).strong(),
                    );
                    ui.checkbox(
                        &mut self.sync.confirm_deletes,
                        txt::confirm_deletes_label(n_del),
                    );
                });
            ui.add_space(6.0);
        }
        let decisions = txt::build_decisions(&plan, &self.sync.choices, self.sync.confirm_deletes);
        let can = txt::can_execute(&plan, self.sync.confirm_deletes, &decisions);
        let mut label = txt::execute_label(&plan, self.sync.confirm_deletes);
        if resuming {
            label = format!("{} ({})", i18n::SYNC_RESUME, label);
        }
        if !can && n_del == 0 && txt::executable_count(&plan, &decisions) == 0 {
            ui.weak(i18n::SYNC_NOTHING_TO_DO);
        }
        let _ = (ask, can, label);
    }

    fn running_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let p = self.sync.progress.clone();
        ui.label(RichText::new(i18n::SYNC_RUNNING).strong());
        let frac = if p.bytes_total > 0 {
            percent(p.bytes_done, p.bytes_total) / 100.0
        } else {
            percent(p.items_done as u64, p.items_total as u64) / 100.0
        };
        ui.add(
            egui::ProgressBar::new(frac)
                .text(format!("{:.0}%", frac * 100.0))
                .desired_height(16.0),
        );
        ui.label(i18n::sync_progress_line(
            p.items_done,
            p.items_total,
            &fmt_size(p.bytes_done),
            &fmt_size(p.bytes_total),
        ));
        if !p.current.is_empty() {
            ui.add(egui::Label::new(RichText::new(&p.current).monospace().small()).truncate());
        }
        self.failures_ui(ui);
        let _ = ask;
    }

    fn failures_ui(&self, ui: &mut egui::Ui) {
        if self.sync.failures.is_empty() {
            return;
        }
        let red = err_color(ui);
        ui.add_space(6.0);
        ui.colored_label(red, i18n::xfer_errors(self.sync.failures.len() as u64));
        egui::ScrollArea::vertical()
            .id_salt("sync-failures")
            .max_height(120.0)
            .show(ui, |ui| {
                for (p, m) in &self.sync.failures {
                    ui.label(format!("{p}: {m}"));
                }
            });
    }

    fn finished_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let Phase::Finished(s) = &self.sync.phase else {
            return;
        };
        let s = *s;
        let color = if s.failed > 0 || s.cancelled {
            warn(ui)
        } else {
            ui.visuals().text_color()
        };
        ui.colored_label(
            color,
            RichText::new(if s.cancelled {
                i18n::SYNC_CANCELLED
            } else if s.failed > 0 {
                i18n::SYNC_DONE_WITH_FAILURES
            } else {
                i18n::SYNC_DONE
            })
            .strong(),
        );
        ui.label(i18n::sync_final_line(
            s.done,
            s.failed,
            s.skipped,
            s.pending,
            &fmt_size(s.bytes),
        ));
        if s.pending > 0 {
            ui.weak(i18n::SYNC_PENDING_HELP);
        }
        for w in &self.sync.warnings {
            ui.weak(w);
        }
        self.failures_ui(ui);
        let _ = ask;
    }

    fn stale_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let Phase::Stale(ch) = &self.sync.phase else {
            return;
        };
        let red = err_color(ui);
        ui.colored_label(red, RichText::new(i18n::SYNC_STALE_TITLE).strong());
        ui.label(i18n::sync_stale_body(ch.len()));
        for c in ch.iter().take(8) {
            let lado = match c.side {
                PSide::Source => i18n::SYNC_SIDE_SOURCE,
                PSide::Dest => i18n::SYNC_SIDE_DEST,
            };
            ui.label(RichText::new(format!("{}  ({lado})", c.path)).monospace());
        }
        if ch.len() > 8 {
            ui.weak(i18n::files_delete_more(ch.len() - 8));
        }
        let _ = ask;
    }

    fn failed_ui(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        let Phase::Failed(m) = &self.sync.phase else {
            return;
        };
        ui.colored_label(
            err_color(ui),
            RichText::new(i18n::SYNC_FAILED_TITLE).strong(),
        );
        ui.label(m);
        let _ = ask;
    }

    /// Botões fixos no rodapé do diálogo (sempre visíveis, mesmo com a lista rolando).
    fn sync_footer(&mut self, ui: &mut egui::Ui, ask: &mut Option<Ask>) {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match &self.sync.phase {
                Phase::Options => {
                    let bad = txt::parse_exclusions(&self.sync.excl_text).is_err();
                    if ui
                        .add_enabled(!bad, egui::Button::new(i18n::SYNC_PREVIEW))
                        .clicked()
                    {
                        *ask = Some(Ask::Plan);
                    }
                    if ui.button(i18n::CANCEL).clicked() {
                        *ask = Some(Ask::Close);
                    }
                }
                Phase::Planning { .. } => {
                    if ui.button(i18n::CLOSE).clicked() {
                        *ask = Some(Ask::Close);
                    }
                }
                Phase::Preview(plan) => {
                    let d =
                        txt::build_decisions(plan, &self.sync.choices, self.sync.confirm_deletes);
                    let can = txt::can_execute(plan, self.sync.confirm_deletes, &d);
                    let mut label = txt::execute_label(plan, self.sync.confirm_deletes);
                    if self.sync.resume_journal.is_some() {
                        label = format!("{} ({})", i18n::SYNC_RESUME, label);
                    }
                    if ui.add_enabled(can, egui::Button::new(label)).clicked() {
                        *ask = Some(Ask::Execute);
                    }
                    if ui.button(i18n::SYNC_BACK).clicked() {
                        *ask = Some(Ask::BackToOptions);
                    }
                    if ui.button(i18n::CLOSE).clicked() {
                        *ask = Some(Ask::Close);
                    }
                }
                Phase::Running => {
                    if ui.button(i18n::SYNC_HIDE).clicked() {
                        *ask = Some(Ask::Hide);
                    }
                    if ui.button(i18n::SYNC_CANCEL).clicked() {
                        *ask = Some(Ask::Cancel);
                    }
                }
                Phase::Finished(_) => {
                    if ui.button(i18n::CLOSE).clicked() {
                        *ask = Some(Ask::Close);
                    }
                    if ui.button(i18n::SYNC_NEW).clicked() {
                        *ask = Some(Ask::BackToOptions);
                    }
                }
                Phase::Stale(_) => {
                    if ui.button(i18n::CLOSE).clicked() {
                        *ask = Some(Ask::Close);
                    }
                    if ui.button(i18n::SYNC_NEW_PREVIEW).clicked() {
                        *ask = Some(Ask::Plan);
                    }
                }
                Phase::Failed(_) => {
                    if ui.button(i18n::CLOSE).clicked() {
                        *ask = Some(Ask::Close);
                    }
                    if ui.button(i18n::SYNC_BACK).clicked() {
                        *ask = Some(Ask::BackToOptions);
                    }
                }
            },
        );
    }
}

fn err_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 120, 110)
    } else {
        Color32::from_rgb(180, 30, 30)
    }
}

fn warn(ui: &egui::Ui) -> Color32 {
    crate::view::warn_color(ui)
}
