//! Aba dividida em painéis. Cada painel é um [`TabBody`] próprio (sessão independente); a
//! geometria vem da árvore de [`crate::split`]. Cada painel recebe o próprio retângulo, então o
//! terminal dele envia o próprio `window_change` ao mudar de tamanho.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Color32, Id, RichText, Sense, Stroke, StrokeKind, UiBuilder, vec2};
use myssh_core::{Environment, Profile};

use crate::env;
use crate::i18n;
use crate::split::{Dir, Node, PaneId};
use crate::tabs::{StatusBadge, TabBody, TabEnv, TabEvent, TabKind};
use crate::theming;

/// Máximo de painéis por aba.
pub const MAX_PANES: usize = 8;

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

pub struct Pane {
    pub id: PaneId,
    pub kind: TabKind,
    pub body: Box<dyn TabBody>,
}

pub struct SplitBody {
    uid: u64,
    panes: Vec<Pane>,
    tree: Node,
    focused: PaneId,
    next_id: PaneId,
    /// Painéis fechados esperando o desligamento educado da sessão.
    closing: Vec<Box<dyn TabBody>>,
    events: Vec<TabEvent>,
    /// Soltar o foco do teclado para o painel focado tomá-lo no próximo desenho.
    refocus: bool,
}

impl SplitBody {
    /// Envolve o conteúdo atual da aba como único painel.
    pub fn new(kind: TabKind, body: Box<dyn TabBody>) -> Self {
        Self {
            uid: NEXT_UID.fetch_add(1, Ordering::Relaxed),
            panes: vec![Pane { id: 1, kind, body }],
            tree: Node::Leaf(1),
            focused: 1,
            next_id: 2,
            closing: Vec::new(),
            events: Vec::new(),
            refocus: false,
        }
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    #[cfg(test)]
    pub fn tree(&self) -> &Node {
        &self.tree
    }

    pub fn focused_id(&self) -> PaneId {
        self.focused
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.tree.leaves()
    }

    pub fn focused_kind(&self) -> &TabKind {
        self.panes
            .iter()
            .find(|p| p.id == self.focused)
            .map_or(&self.panes[0].kind, |p| &p.kind)
    }

    pub fn is_pane_busy(&self, id: PaneId) -> bool {
        self.panes
            .iter()
            .find(|p| p.id == id)
            .is_some_and(|p| p.body.is_busy())
    }

    pub fn pane_transfers(&self, id: PaneId) -> usize {
        self.panes
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.body.active_transfers())
    }

    pub fn pane_syncs(&self, id: PaneId) -> usize {
        self.panes
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.body.active_syncs())
    }

    pub fn pane_title(&self, id: PaneId) -> String {
        self.panes
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.body.title())
            .unwrap_or_default()
    }

    /// Divide o painel focado; o novo painel fica no segundo lado e ganha o foco.
    pub fn split(&mut self, dir: Dir, kind: TabKind, body: Box<dyn TabBody>) -> Option<PaneId> {
        if self.panes.len() >= MAX_PANES {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        if !self.tree.split_leaf(self.focused, dir, id) {
            return None;
        }
        self.panes.push(Pane { id, kind, body });
        self.focused = id;
        self.refocus = true;
        Some(id)
    }

    /// Fecha um painel (a sessão é encerrada em segundo plano). `true` = não sobrou nenhum.
    pub fn close_pane(&mut self, id: PaneId) -> bool {
        let Some(i) = self.panes.iter().position(|p| p.id == id) else {
            return false;
        };
        let before = self.tree.leaves();
        let pos = before.iter().position(|x| *x == id).unwrap_or(0);
        let mut pane = self.panes.remove(i);
        pane.body.close();
        self.closing.push(pane.body);
        let tree = std::mem::replace(&mut self.tree, Node::Leaf(0));
        match tree.remove(id) {
            Some(t) => {
                self.tree = t;
                if self.focused == id {
                    let after = self.tree.leaves();
                    self.focused = after[pos.saturating_sub(1).min(after.len() - 1)];
                    self.refocus = true;
                }
                false
            }
            None => true,
        }
    }

    /// Passa o foco para o próximo painel (na ordem da árvore).
    pub fn cycle_focus(&mut self) {
        let l = self.tree.leaves();
        if let Some(i) = l.iter().position(|x| *x == self.focused) {
            self.focused = l[(i + 1) % l.len()];
            self.refocus = true;
        }
    }

    /// Atualiza o tema dos painéis a partir dos perfis salvos (editados no diálogo).
    pub fn refresh_themes(&mut self, profiles: &[Profile]) {
        for p in &mut self.panes {
            refresh_kind_theme(&mut p.kind, profiles);
        }
    }

    fn header_ui(
        &self,
        ui: &mut egui::Ui,
        pane: &Pane,
        focused: bool,
        focus_color: Color32,
        accent: Option<Color32>,
        close: &mut Option<PaneId>,
    ) {
        let (fill, text) = if focused {
            (focus_color, theming::contrast_text(focus_color))
        } else {
            (ui.visuals().faint_bg_color, ui.visuals().text_color())
        };
        egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if let (false, Some(a)) = (focused, accent) {
                        let (r, _) = ui.allocate_exact_size(vec2(4.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(r, 1.0, a);
                    }
                    let marker = if focused { "● " } else { "○ " };
                    ui.label(
                        RichText::new(format!("{marker}{}", pane.body.title()))
                            .strong()
                            .color(text),
                    );
                    if let TabKind::Terminal { profile, .. } = &pane.kind
                        && let Some(s) = env::style(profile.environment)
                    {
                        ui.label(
                            RichText::new(format!(" {} {} ", s.icon, s.label))
                                .small()
                                .color(s.text)
                                .background_color(s.fill),
                        );
                    }
                    if let Some(b) = pane.body.status() {
                        ui.label(
                            RichText::new(format!("{} {}", b.icon, b.text))
                                .small()
                                .color(if focused { text } else { b.color }),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text(i18n::SPLIT_CLOSE_PANE)
                            .clicked()
                        {
                            *close = Some(pane.id);
                        }
                    });
                });
            });
    }
}

/// Copia o tema do perfil salvo para a cópia guardada na aba.
pub fn refresh_kind_theme(kind: &mut TabKind, profiles: &[Profile]) {
    if let TabKind::Terminal {
        profile,
        saved: true,
    } = kind
        && let Some(fresh) = profiles.iter().find(|p| p.id == profile.id)
    {
        profile.theme = fresh.theme.clone();
    }
}

/// Faixa de ambiente (cor + ícone + texto) no topo do conteúdo.
pub fn env_banner(ui: &mut egui::Ui, s: env::EnvStyle, info: &str) {
    egui::Frame::new()
        .fill(s.fill)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!("{} {}  ·  {info}", s.icon, s.label))
                    .strong()
                    .color(s.text),
            );
        });
    ui.add_space(4.0);
}

impl TabBody for SplitBody {
    fn title(&self) -> String {
        let t = self
            .panes
            .iter()
            .find(|p| p.id == self.focused)
            .map(|p| p.body.title())
            .unwrap_or_default();
        if self.panes.len() > 1 {
            format!("{t} (+{})", self.panes.len() - 1)
        } else {
            t
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, env: &TabEnv) {
        let refocus = std::mem::take(&mut self.refocus);
        if refocus {
            ui.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());

        // Divisores arrastáveis.
        let layout = self.tree.layout(rect);
        for d in &layout.dividers {
            let id = Id::new(("split-divider", self.uid, d.path.clone()));
            let r = ui.interact(d.rect, id, Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(match d.dir {
                    Dir::Vertical => egui::CursorIcon::ResizeHorizontal,
                    Dir::Horizontal => egui::CursorIcon::ResizeVertical,
                });
            }
            if r.dragged()
                && let Some(p) = r.interact_pointer_pos()
                && let Some(slot) = self.tree.ratio_mut(&d.path)
            {
                *slot = d.ratio_at(p);
            }
        }
        let layout = self.tree.layout(rect);
        for d in &layout.dividers {
            let active =
                ui.ctx()
                    .is_being_dragged(Id::new(("split-divider", self.uid, d.path.clone())));
            let color = if active {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            };
            let grip = match d.dir {
                Dir::Vertical => vec2(2.0, d.rect.height()),
                Dir::Horizontal => vec2(d.rect.width(), 2.0),
            };
            ui.painter().rect_filled(
                egui::Rect::from_center_size(d.rect.center(), grip),
                1.0,
                color,
            );
        }

        // Clique escolhe o painel focado (o terminal também toma o foco do teclado ao clicar).
        if ui.input(|i| i.pointer.any_pressed())
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && let Some((id, _)) = layout.panes.iter().find(|(_, r)| r.contains(pos))
        {
            self.focused = *id;
        }

        let multi = self.panes.len() > 1;
        let mut order = layout.panes.clone();
        if refocus {
            // Só neste quadro o painel focado é desenhado primeiro: sem ninguém com o foco do
            // teclado, o primeiro terminal desenhado o toma. Fora disso a ordem é estável (mudar
            // a ordem de desenho entre o clique e a soltura perderia o clique).
            order.sort_by_key(|(id, _)| *id != self.focused);
        }
        let mut close = None;
        for (id, prect) in order {
            self.pane_ui(ui, env, id, prect, multi, &mut close);
        }
        if let Some(id) = close {
            self.events.push(TabEvent::ClosePane(id));
        }
    }

    fn pump(&mut self, env: &TabEnv) {
        for p in &mut self.panes {
            p.body.pump(&env.with_profile_theme(&p.kind));
        }
        for b in &mut self.closing {
            b.pump(env);
        }
        self.closing.retain(|b| !b.is_closed());
    }

    fn needs_modal(&self) -> bool {
        self.panes.iter().any(|p| p.body.needs_modal())
    }

    fn modal_ui(&mut self, ctx: &egui::Context, env: &TabEnv) {
        let focused = self.focused;
        let idx = self
            .panes
            .iter()
            .position(|p| p.id == focused && p.body.needs_modal())
            .or_else(|| self.panes.iter().position(|p| p.body.needs_modal()));
        if let Some(i) = idx {
            let e = env.with_profile_theme(&self.panes[i].kind);
            self.panes[i].body.modal_ui(ctx, &e);
        }
    }

    fn drain(&mut self) {
        for p in &mut self.panes {
            p.body.drain();
        }
        for b in &mut self.closing {
            b.drain();
        }
    }

    fn status(&self) -> Option<StatusBadge> {
        self.panes
            .iter()
            .find(|p| p.id == self.focused)
            .and_then(|p| p.body.status())
    }

    fn take_events(&mut self) -> Vec<TabEvent> {
        let mut v = std::mem::take(&mut self.events);
        for p in &mut self.panes {
            v.extend(p.body.take_events());
        }
        v
    }

    fn fills_space(&self) -> bool {
        true
    }

    fn is_busy(&self) -> bool {
        self.panes.iter().any(|p| p.body.is_busy())
    }

    fn active_transfers(&self) -> usize {
        self.panes.iter().map(|p| p.body.active_transfers()).sum()
    }

    fn active_syncs(&self) -> usize {
        self.panes.iter().map(|p| p.body.active_syncs()).sum()
    }

    #[cfg(test)]
    fn files_panel(&self) -> Option<&crate::files::FilesPanel> {
        self.panes
            .iter()
            .find(|p| p.id == self.focused)
            .and_then(|p| p.body.files_panel())
    }

    #[cfg(test)]
    fn files_panel_mut(&mut self) -> Option<&mut crate::files::FilesPanel> {
        let f = self.focused;
        self.panes
            .iter_mut()
            .find(|p| p.id == f)
            .and_then(|p| p.body.files_panel_mut())
    }

    fn toggle_files(&mut self) {
        let f = self.focused;
        if let Some(p) = self.panes.iter_mut().find(|p| p.id == f) {
            p.body.toggle_files();
        }
    }

    fn close(&mut self) {
        for p in &mut self.panes {
            p.body.close();
        }
    }

    fn is_closed(&self) -> bool {
        self.panes.iter().all(|p| p.body.is_closed()) && self.closing.iter().all(|b| b.is_closed())
    }

    fn has_terminal_focus(&self) -> bool {
        self.panes.iter().any(|p| p.body.has_terminal_focus())
    }

    fn as_split(&mut self) -> Option<&mut SplitBody> {
        Some(self)
    }

    fn as_split_ref(&self) -> Option<&SplitBody> {
        Some(self)
    }

    fn manages_chrome(&self) -> bool {
        true
    }

    #[cfg(test)]
    fn debug_screen(&self) -> String {
        self.panes
            .iter()
            .find(|p| p.id == self.focused)
            .map(|p| p.body.debug_screen())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn debug_bytes(&self) -> u64 {
        self.panes
            .iter()
            .find(|p| p.id == self.focused)
            .map_or(0, |p| p.body.debug_bytes())
    }

    #[cfg(test)]
    fn debug_send(&self, bytes: &[u8]) {
        if let Some(p) = self.panes.iter().find(|p| p.id == self.focused) {
            p.body.debug_send(bytes);
        }
    }

    #[cfg(any(test, debug_assertions, feature = "demo"))]
    fn demo_accept_host_key(&mut self) {
        for p in &mut self.panes {
            p.body.demo_accept_host_key();
        }
    }

    #[cfg(any(test, debug_assertions, feature = "demo"))]
    fn simulate_busy(&mut self, busy: bool) {
        for p in &mut self.panes {
            p.body.simulate_busy(busy);
        }
    }
}

impl SplitBody {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        env: &TabEnv,
        id: PaneId,
        prect: egui::Rect,
        multi: bool,
        close: &mut Option<PaneId>,
    ) {
        let Some(idx) = self.panes.iter().position(|p| p.id == id) else {
            return;
        };
        let focused = multi && self.focused == id;
        let uid = self.uid;
        let penv = env.with_profile_theme(&self.panes[idx].kind);
        let (environment, info, accent) = match &self.panes[idx].kind {
            TabKind::Terminal { profile, .. } => (
                profile.environment,
                Some(format!("{}@{}", profile.username, profile.host)),
                theming::resolve(&profile.theme, env.dark, env.font_size).accent,
            ),
            TabKind::Welcome => (Environment::None, None, None),
        };
        let est = env::style(environment)
            .filter(|_| environment != Environment::Production || env.prod_warning);
        let focus_color = accent.unwrap_or(ui.visuals().selection.bg_fill);
        let stroke = match est {
            Some(s) if environment == Environment::Production => Stroke::new(3.0, s.fill),
            _ if focused => Stroke::new(2.0, focus_color),
            _ if multi => Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            _ => Stroke::NONE,
        };
        let inner = prect.shrink(3.0);
        let mut ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("pane", uid, id))
                .max_rect(inner)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(inner.intersect(ui.clip_rect()));
        if multi {
            self.header_ui(
                &mut ui,
                &self.panes[idx],
                focused,
                focus_color,
                accent,
                close,
            );
        } else if let (Some(s), Some(info)) = (est, &info) {
            env_banner(&mut ui, s, info);
        }
        self.panes[idx].body.ui(&mut ui, &penv);
        if stroke.width > 0.0 {
            ui.painter().with_clip_rect(prect).rect_stroke(
                prect.shrink(stroke.width / 2.0),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
        }
    }
}
