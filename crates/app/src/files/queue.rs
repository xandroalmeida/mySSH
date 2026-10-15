//! Estado da fila de transferências (aplicação de eventos) e sua UI.

use std::collections::{HashSet, VecDeque};

use egui::{Color32, RichText};

use super::contract::*;
use super::fmt::{fmt_eta, fmt_rate, fmt_size, percent};
use crate::i18n;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Queued,
    Scanning,
    Running,
    Done,
    /// Terminou, mas com falhas em alguns arquivos.
    Failed,
    Cancelled,
}

impl State {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => i18n::XFER_QUEUED(),
            Self::Scanning => i18n::XFER_SCANNING(),
            Self::Running => i18n::XFER_RUNNING(),
            Self::Done => i18n::XFER_DONE(),
            Self::Failed => i18n::XFER_FAILED(),
            Self::Cancelled => i18n::XFER_CANCELLED(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: TransferId,
    pub label: String,
    pub state: State,
    pub current: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub rate_bps: u64,
    pub ok: u64,
    pub failed: u64,
    pub skipped: u64,
    pub errors: Vec<(String, String)>,
    pub expanded: bool,
    pub cancel_requested: bool,
}

impl Item {
    fn new(id: TransferId, label: String) -> Self {
        Self {
            id,
            label,
            state: State::Queued,
            current: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            rate_bps: 0,
            ok: 0,
            failed: 0,
            skipped: 0,
            errors: Vec::new(),
            expanded: false,
            cancel_requested: false,
        }
    }
}

/// Conflito esperando decisão do usuário.
#[derive(Debug)]
pub struct PendingConflict {
    pub id: TransferId,
    pub path: String,
    pub existing: RemoteOrLocalMeta,
    pub incoming: Option<RemoteOrLocalMeta>,
    pub responder: ConflictResponder,
}

/// Máximo de erros por arquivo guardados por transferência (o resto vira contagem).
pub const MAX_ERRORS_KEPT: usize = 200;

#[derive(Default)]
pub struct Queue {
    pub items: Vec<Item>,
    pub conflicts: VecDeque<PendingConflict>,
    /// Transferências em que o usuário escolheu "renomear em todos".
    pub sticky_rename: HashSet<TransferId>,
}

impl Queue {
    fn item_mut(&mut self, id: TransferId) -> &mut Item {
        if let Some(i) = self.items.iter().position(|i| i.id == id) {
            return &mut self.items[i];
        }
        self.items.push(Item::new(id, String::new()));
        self.items.last_mut().expect("acabou de inserir")
    }

    #[cfg(test)]
    pub fn get(&self, id: TransferId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn active_count(&self) -> usize {
        self.items.iter().filter(|i| !i.state.is_terminal()).count()
    }

    pub fn clear_finished(&mut self) {
        self.items.retain(|i| !i.state.is_terminal());
    }

    /// Aplica um evento do motor. Um item já terminado ignora eventos atrasados.
    pub fn apply(&mut self, ev: TransferEvent) {
        match ev {
            TransferEvent::Queued { id, label } => {
                let it = self.item_mut(id);
                it.label = label;
            }
            TransferEvent::Scanning { id, files, bytes } => {
                let it = self.item_mut(id);
                if !it.state.is_terminal() {
                    it.state = State::Scanning;
                    it.files_total = files;
                    it.bytes_total = bytes;
                }
            }
            TransferEvent::Progress {
                id,
                current,
                bytes_done,
                bytes_total,
                files_done,
                files_total,
                rate_bps,
            } => {
                let it = self.item_mut(id);
                if !it.state.is_terminal() {
                    it.state = State::Running;
                    it.current = current;
                    it.bytes_done = bytes_done;
                    it.bytes_total = bytes_total;
                    it.files_done = files_done;
                    it.files_total = files_total;
                    it.rate_bps = rate_bps;
                }
            }
            TransferEvent::Conflict {
                id,
                path,
                existing,
                responder,
            } => {
                if self.sticky_rename.contains(&id) {
                    responder.answer(ConflictDecision::Rename);
                } else {
                    self.conflicts.push_back(PendingConflict {
                        id,
                        path,
                        existing,
                        incoming: None,
                        responder,
                    });
                }
            }
            TransferEvent::FileError { id, path, message } => {
                let it = self.item_mut(id);
                it.failed += 1;
                if it.errors.len() < MAX_ERRORS_KEPT {
                    it.errors.push((path, message));
                }
            }
            TransferEvent::Finished {
                id,
                ok,
                failed,
                skipped,
            } => {
                let it = self.item_mut(id);
                it.ok = ok;
                it.failed = failed;
                it.skipped = skipped;
                it.state = if failed > 0 {
                    State::Failed
                } else {
                    State::Done
                };
                if failed > 0 {
                    it.expanded = true;
                }
                if it.state == State::Done {
                    it.bytes_done = it.bytes_total;
                }
                self.forget(id);
            }
            TransferEvent::Cancelled { id } => {
                self.item_mut(id).state = State::Cancelled;
                self.forget(id);
            }
        }
    }

    /// A transferência terminou: conflitos pendentes dela perdem o sentido.
    fn forget(&mut self, id: TransferId) {
        self.conflicts.retain(|c| c.id != id);
        self.sticky_rename.remove(&id);
    }

    /// Responde ao conflito da frente da fila.
    pub fn answer_front(&mut self, decision: ConflictDecision, sticky_rename: bool) {
        if let Some(c) = self.conflicts.pop_front() {
            c.responder.answer(decision);
            if sticky_rename {
                self.sticky_rename.insert(c.id);
                // os já pendentes dessa transferência recebem a mesma resposta
                let id = c.id;
                let mut rest = VecDeque::new();
                for p in self.conflicts.drain(..) {
                    if p.id == id {
                        p.responder.answer(ConflictDecision::Rename);
                    } else {
                        rest.push_back(p);
                    }
                }
                self.conflicts = rest;
            }
            if matches!(decision, ConflictDecision::Cancel)
                && let Some(i) = self.items.iter_mut().find(|i| i.id == c.id)
            {
                i.cancel_requested = true;
            }
        }
    }
}

/// O que o usuário pediu na lista da fila.
#[derive(Debug, PartialEq, Eq)]
pub enum QueueAction {
    Cancel(TransferId),
    CancelAll,
    ClearFinished,
}

fn state_color(ui: &egui::Ui, s: State) -> Color32 {
    let dark = ui.visuals().dark_mode;
    match (s, dark) {
        (State::Done, true) => Color32::from_rgb(110, 210, 130),
        (State::Done, false) => Color32::from_rgb(25, 120, 50),
        (State::Failed, true) => Color32::from_rgb(255, 130, 120),
        (State::Failed, false) => Color32::from_rgb(180, 30, 30),
        (State::Cancelled, true) => Color32::from_rgb(200, 200, 120),
        (State::Cancelled, false) => Color32::from_rgb(130, 100, 0),
        _ => ui.visuals().text_color(),
    }
}

/// Cor da barra: erro e cancelamento nunca usam o azul de "em andamento/concluída".
pub fn bar_fill(ui: &egui::Ui, s: State) -> Option<Color32> {
    match s {
        State::Failed => Some(if ui.visuals().dark_mode {
            Color32::from_rgb(170, 60, 55)
        } else {
            Color32::from_rgb(200, 70, 60)
        }),
        State::Cancelled => Some(Color32::from_gray(120)),
        _ => None,
    }
}

/// Resumo numérico de uma linha.
pub fn summary_line(it: &Item) -> String {
    let mut parts = vec![format!(
        "{} / {}",
        fmt_size(it.bytes_done),
        fmt_size(it.bytes_total)
    )];
    parts.push(i18n::xfer_files(it.files_done, it.files_total));
    if it.state == State::Running {
        parts.push(fmt_rate(it.rate_bps));
        parts.push(format!(
            "{} {}",
            i18n::XFER_ETA(),
            fmt_eta(it.bytes_done, it.bytes_total, it.rate_bps)
        ));
    }
    if it.state.is_terminal() && (it.skipped > 0 || it.failed > 0) {
        parts.push(i18n::xfer_result(it.ok, it.failed, it.skipped));
    }
    parts.join("  ·  ")
}

pub fn queue_ui(ui: &mut egui::Ui, q: &mut Queue) -> Option<QueueAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(i18n::xfer_title(q.items.len(), q.active_count())).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    q.active_count() > 0,
                    egui::Button::new(i18n::XFER_CANCEL_ALL()),
                )
                .clicked()
            {
                action = Some(QueueAction::CancelAll);
            }
            if ui
                .add_enabled(
                    q.items.iter().any(|i| i.state.is_terminal()),
                    egui::Button::new(i18n::XFER_CLEAR()),
                )
                .clicked()
            {
                action = Some(QueueAction::ClearFinished);
            }
        });
    });
    egui::ScrollArea::vertical()
        .id_salt("xfer-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for it in q.items.iter_mut() {
                item_ui(ui, it, &mut action);
                ui.separator();
            }
        });
    action
}

fn item_ui(ui: &mut egui::Ui, it: &mut Item, action: &mut Option<QueueAction>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&it.label).strong());
        let badge = if it.cancel_requested && !it.state.is_terminal() {
            i18n::XFER_CANCELLING()
        } else {
            it.state.label()
        };
        ui.label(RichText::new(badge).color(state_color(ui, it.state)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can = !it.state.is_terminal() && !it.cancel_requested;
            if ui
                .add_enabled(can, egui::Button::new(i18n::XFER_CANCEL()))
                .on_hover_text(i18n::XFER_CANCEL_HINT())
                .clicked()
            {
                it.cancel_requested = true;
                *action = Some(QueueAction::Cancel(it.id));
            }
        });
    });
    let frac = percent(it.bytes_done, it.bytes_total) / 100.0;
    let text = match it.state {
        State::Scanning => i18n::xfer_counting(it.files_total, it.bytes_total),
        // Com falhas/cancelada a barra não pode parecer "concluída".
        State::Failed => i18n::XFER_FAILED_BAR().to_string(),
        State::Cancelled => i18n::XFER_CANCELLED().to_string(),
        _ => format!("{:.0}%", frac * 100.0),
    };
    let mut bar = egui::ProgressBar::new(frac).text(text).desired_height(14.0);
    if let Some(c) = bar_fill(ui, it.state) {
        bar = bar.fill(c);
    }
    ui.add(bar);
    ui.weak(summary_line(it));
    if it.state == State::Running && !it.current.is_empty() {
        ui.add(egui::Label::new(RichText::new(&it.current).weak().small()).truncate());
    }
    if !it.errors.is_empty() || it.failed > 0 {
        let n = it.failed.max(it.errors.len() as u64);
        let arrow = if it.expanded { "▾" } else { "▸" };
        let title = RichText::new(format!("{arrow} {}", i18n::xfer_errors(n)))
            .color(state_color(ui, State::Failed));
        if ui.add(egui::Button::new(title).frame(false)).clicked() {
            it.expanded = !it.expanded;
        }
        if it.expanded {
            ui.indent(("xfer-err", it.id), |ui| {
                for (path, msg) in &it.errors {
                    ui.label(format!("{path}: {msg}"));
                }
                let hidden = n.saturating_sub(it.errors.len() as u64);
                if hidden > 0 {
                    ui.weak(i18n::xfer_errors_hidden(hidden));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid(n: u64) -> TransferId {
        TransferId(n)
    }

    fn progress(id: u64, done: u64, total: u64) -> TransferEvent {
        TransferEvent::Progress {
            id: tid(id),
            current: "a.bin".into(),
            bytes_done: done,
            bytes_total: total,
            files_done: 1,
            files_total: 3,
            rate_bps: 1000,
        }
    }

    #[test]
    fn ciclo_de_estados() {
        let mut q = Queue::default();
        q.apply(TransferEvent::Queued {
            id: tid(1),
            label: "Enviar x".into(),
        });
        assert_eq!(q.get(tid(1)).unwrap().state, State::Queued);
        assert_eq!(q.active_count(), 1);
        q.apply(TransferEvent::Scanning {
            id: tid(1),
            files: 3,
            bytes: 900,
        });
        assert_eq!(q.get(tid(1)).unwrap().state, State::Scanning);
        q.apply(progress(1, 300, 900));
        let it = q.get(tid(1)).unwrap();
        assert_eq!(
            (it.state, it.bytes_done, it.files_total),
            (State::Running, 300, 3)
        );
        q.apply(TransferEvent::Finished {
            id: tid(1),
            ok: 3,
            failed: 0,
            skipped: 0,
        });
        let it = q.get(tid(1)).unwrap();
        assert_eq!((it.state, it.bytes_done), (State::Done, 900));
        assert_eq!(q.active_count(), 0);
    }

    #[test]
    fn falhas_e_cancelamento() {
        let mut q = Queue::default();
        q.apply(progress(1, 10, 100));
        q.apply(TransferEvent::FileError {
            id: tid(1),
            path: "/x/a".into(),
            message: "Permissão negada".into(),
        });
        q.apply(TransferEvent::Finished {
            id: tid(1),
            ok: 2,
            failed: 1,
            skipped: 0,
        });
        let it = q.get(tid(1)).unwrap();
        assert_eq!(it.state, State::Failed);
        assert_eq!(it.errors.len(), 1);
        assert!(it.expanded);

        q.apply(progress(2, 1, 100));
        q.apply(TransferEvent::Cancelled { id: tid(2) });
        assert_eq!(q.get(tid(2)).unwrap().state, State::Cancelled);
        // evento atrasado não ressuscita o item
        q.apply(progress(2, 50, 100));
        assert_eq!(q.get(tid(2)).unwrap().state, State::Cancelled);
        q.clear_finished();
        assert!(q.items.is_empty());
    }

    #[test]
    fn erros_guardados_tem_limite() {
        let mut q = Queue::default();
        for i in 0..(MAX_ERRORS_KEPT + 50) {
            q.apply(TransferEvent::FileError {
                id: tid(1),
                path: format!("/f{i}"),
                message: "x".into(),
            });
        }
        let it = q.get(tid(1)).unwrap();
        assert_eq!(it.errors.len(), MAX_ERRORS_KEPT);
    }

    fn conflict(
        q: &mut Queue,
        id: u64,
        path: &str,
    ) -> tokio::sync::oneshot::Receiver<ConflictDecision> {
        let (responder, rx) = ConflictResponder::channel();
        q.apply(TransferEvent::Conflict {
            id: tid(id),
            path: path.into(),
            existing: RemoteOrLocalMeta {
                kind: Kind::File,
                size: 1,
                mtime: None,
            },
            responder,
        });
        rx
    }

    #[test]
    fn conflitos_em_fila_e_resposta() {
        let mut q = Queue::default();
        let mut r1 = conflict(&mut q, 1, "/a");
        let mut r2 = conflict(&mut q, 1, "/b");
        assert_eq!(q.conflicts.len(), 2);
        q.answer_front(ConflictDecision::Skip, false);
        assert_eq!(r1.try_recv().unwrap(), ConflictDecision::Skip);
        assert!(r2.try_recv().is_err());
        assert_eq!(q.conflicts.front().unwrap().path, "/b");
    }

    #[test]
    fn renomear_em_todos_responde_os_proximos_sozinho() {
        let mut q = Queue::default();
        let mut r1 = conflict(&mut q, 1, "/a");
        let mut r2 = conflict(&mut q, 1, "/b");
        let mut other = conflict(&mut q, 2, "/z");
        q.answer_front(ConflictDecision::Rename, true);
        assert_eq!(r1.try_recv().unwrap(), ConflictDecision::Rename);
        assert_eq!(r2.try_recv().unwrap(), ConflictDecision::Rename);
        assert!(other.try_recv().is_err());
        // novos conflitos da mesma transferência são respondidos na hora
        let mut r3 = conflict(&mut q, 1, "/c");
        assert_eq!(r3.try_recv().unwrap(), ConflictDecision::Rename);
        assert_eq!(q.conflicts.len(), 1);
        // terminar a transferência descarta a memória e os pendentes
        q.apply(TransferEvent::Cancelled { id: tid(2) });
        assert!(q.conflicts.is_empty());
        q.apply(TransferEvent::Finished {
            id: tid(1),
            ok: 0,
            failed: 0,
            skipped: 0,
        });
        assert!(q.sticky_rename.is_empty());
    }

    #[test]
    fn barra_de_falha_e_cancelamento_nao_parece_concluida() {
        egui::__run_test_ui(|ui| {
            assert!(bar_fill(ui, State::Failed).is_some());
            assert!(bar_fill(ui, State::Cancelled).is_some());
            assert!(bar_fill(ui, State::Done).is_none());
            assert!(bar_fill(ui, State::Running).is_none());
        });
    }

    #[test]
    fn resumo_mostra_taxa_e_eta_so_transferindo() {
        let mut q = Queue::default();
        q.apply(progress(1, 500, 1000));
        let s = summary_line(q.get(tid(1)).unwrap());
        assert!(s.contains("500 B / 1000 B"), "{s}");
        assert!(s.contains("1/3"), "{s}");
        assert!(s.contains("1000 B/s"), "{s}");
        assert!(s.contains("1 s") || s.contains("1 s"), "{s}");
    }
}
