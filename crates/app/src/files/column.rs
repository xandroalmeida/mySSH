//! Uma coluna do painel (local ou remota): estado da navegação e desenho da lista.

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, EventFilter, FontId, Id, Key, Modifiers, Rect, RichText, Sense, Stroke,
    StrokeKind, TextStyle, WidgetInfo, WidgetType, pos2, vec2,
};

use super::contract::Kind;
use super::fmt::{fmt_mtime, fmt_size};
use super::model::{Entry, Selection, Side, Sort, SortKey, sort_entries, visible_indices};
use crate::i18n;

/// O que a coluna pede ao painel (executado depois do desenho, sem empréstimos cruzados).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    Navigate(Side, String),
    Back(Side),
    Up(Side),
    Home(Side),
    Refresh(Side),
    /// Duplo clique / Enter em pasta ou link: entra.
    Open(Side, String),
    /// Transferir a seleção da coluna de origem para a outra.
    Transfer(Side),
    NewDir(Side),
    Rename(Side),
    Delete(Side),
}

/// Carga do arrastar-soltar entre colunas: de onde saiu (a seleção é lida da coluna de origem).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragPayload {
    pub from: Side,
}

#[derive(Debug, Clone)]
struct Pending {
    path: String,
    /// Entra no histórico de "Voltar" ao concluir.
    record: bool,
}

pub struct PaneState {
    pub side: Side,
    /// Pasta exibida (confirmada por uma listagem bem-sucedida).
    pub path: String,
    /// Texto do campo de caminho.
    pub edit: String,
    pending: Option<Pending>,
    pub loading: bool,
    pub error: Option<String>,
    /// Ordenadas; inclui ocultos.
    pub entries: Vec<Entry>,
    pub sel: Selection,
    pub sort: Sort,
    pub show_hidden: bool,
    back: Vec<String>,
    gen_: u64,
    scroll_to_cursor: bool,
    scroll_y: f32,
    view_h: f32,
}

impl PaneState {
    pub fn new(side: Side) -> Self {
        Self {
            side,
            path: String::new(),
            edit: String::new(),
            pending: None,
            loading: false,
            error: None,
            entries: Vec::new(),
            sel: Selection::default(),
            sort: Sort::default(),
            show_hidden: false,
            back: Vec::new(),
            gen_: 0,
            scroll_to_cursor: false,
            scroll_y: 0.0,
            view_h: 0.0,
        }
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn visible(&self) -> Vec<usize> {
        visible_indices(&self.entries, self.show_hidden)
    }

    /// Entradas selecionadas e visíveis, na ordem da lista.
    pub fn selected(&self) -> Vec<&Entry> {
        self.visible()
            .into_iter()
            .map(|i| &self.entries[i])
            .filter(|e| self.sel.contains(&e.name))
            .collect()
    }

    /// Começa uma navegação; devolve a geração para descartar respostas atrasadas.
    pub fn begin_nav(&mut self, path: String, record: bool) -> u64 {
        self.gen_ += 1;
        self.loading = true;
        self.error = None;
        self.pending = Some(Pending { path, record });
        self.gen_
    }

    /// Volta ao histórico: devolve o destino sem registrar a ida.
    pub fn pop_back(&mut self) -> Option<String> {
        self.back.pop()
    }

    pub fn on_listed(&mut self, gen_: u64, res: Result<Vec<Entry>, String>) {
        if gen_ != self.gen_ {
            return;
        }
        self.loading = false;
        let pending = self.pending.take();
        match res {
            Ok(mut v) => {
                sort_entries(&mut v, self.sort);
                if let Some(p) = pending
                    && p.path != self.path
                {
                    if p.record && !self.path.is_empty() {
                        self.back.push(std::mem::take(&mut self.path));
                    }
                    self.path = p.path;
                    self.sel.clear();
                    self.sel.cursor = None;
                    self.scroll_y = 0.0;
                }
                self.entries = v;
                self.edit = self.path.clone();
                self.error = None;
                let vis = self.visible();
                let names: Vec<&str> = vis.iter().map(|i| self.entries[*i].name.as_str()).collect();
                self.sel.retain_existing(&names);
            }
            Err(e) => {
                self.error = Some(e);
                self.edit = self.path.clone();
            }
        }
    }

    pub fn resort(&mut self) {
        sort_entries(&mut self.entries, self.sort);
    }
}

pub struct ColCtx<'a> {
    pub uid: u64,
    pub label: &'a str,
    /// A coluna está disponível (remoto sem backend = `false`).
    pub available: bool,
    /// Teclado liberado (nenhum diálogo aberto).
    pub keys: bool,
    /// Há seleção na coluna de origem e destino conhecido (liga o menu "Enviar/Baixar").
    pub can_transfer: bool,
    /// Devolve o foco do teclado à lista (ao fechar um diálogo).
    pub focus_request: bool,
}

const ROW_H: f32 = 22.0;

/// Informação de acessibilidade de um campo de texto com nome próprio.
pub fn text_info(label: &str, value: &str) -> WidgetInfo {
    WidgetInfo {
        label: Some(label.to_string()),
        current_text_value: Some(value.to_string()),
        ..WidgetInfo::new(WidgetType::TextEdit)
    }
}

/// Ícones desenhados (independem das fontes de emoji do sistema).
fn paint_icon(ui: &egui::Ui, cell: Rect, kind: Kind, color: Color32) {
    let p = ui.painter();
    let c = Rect::from_center_size(cell.center(), vec2(14.0, 12.0));
    match kind {
        Kind::Dir => {
            let amber = Color32::from_rgb(222, 170, 60);
            let tab = Rect::from_min_size(c.min, vec2(6.0, 3.0));
            p.rect_filled(tab, 1.0, amber);
            let body = Rect::from_min_max(pos2(c.left(), c.top() + 2.0), c.max);
            p.rect_filled(body, 1.5, amber);
        }
        Kind::File | Kind::Symlink | Kind::Other => {
            let page = Rect::from_center_size(cell.center(), vec2(10.0, 13.0));
            let st = Stroke::new(1.2, color);
            p.rect_stroke(page, 1.5, st, StrokeKind::Inside);
            if kind == Kind::File {
                for dy in [-2.0, 1.0, 4.0] {
                    let y = page.center().y + dy;
                    p.line_segment(
                        [pos2(page.left() + 2.5, y), pos2(page.right() - 2.5, y)],
                        Stroke::new(1.0, color.gamma_multiply(0.6)),
                    );
                }
            }
            if kind == Kind::Symlink {
                // seta diagonal
                let (a, b) = (
                    pos2(page.left() + 2.5, page.bottom() - 2.5),
                    pos2(page.right() - 2.5, page.top() + 3.0),
                );
                p.line_segment([a, b], st);
                p.line_segment([b, pos2(b.x - 4.0, b.y)], st);
                p.line_segment([b, pos2(b.x, b.y + 4.0)], st);
            }
        }
    }
}

pub fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Dir => i18n::FILES_KIND_DIR,
        Kind::File => i18n::FILES_KIND_FILE,
        Kind::Symlink => i18n::FILES_KIND_LINK,
        Kind::Other => i18n::FILES_KIND_OTHER,
    }
}

fn clipped_text(
    ui: &egui::Ui,
    cell: Rect,
    align: Align2,
    text: &str,
    color: Color32,
    font: &FontId,
) {
    let mut job = LayoutJob::simple(text.to_owned(), font.clone(), color, cell.width().max(1.0));
    job.wrap = TextWrapping {
        max_rows: 1,
        break_anywhere: true,
        max_width: cell.width().max(1.0),
        ..Default::default()
    };
    let galley = ui.painter().layout_job(job);
    let pos = align.align_size_within_rect(galley.size(), cell).min;
    ui.painter().galley(pos, galley, color);
}

pub fn column_ui(ui: &mut egui::Ui, pane: &mut PaneState, cx: &ColCtx, acts: &mut Vec<Act>) {
    let side = pane.side;
    ui.push_id((cx.uid, side), |ui| {
        ui.label(RichText::new(cx.label).strong());
        if !cx.available {
            ui.add_space(8.0);
            ui.label(i18n::FILES_REMOTE_UNAVAILABLE);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(pane.can_back(), egui::Button::new(i18n::FILES_BACK))
                .clicked()
            {
                acts.push(Act::Back(side));
            }
            if ui.button(i18n::FILES_UP).clicked() {
                acts.push(Act::Up(side));
            }
            if ui.button(i18n::FILES_REFRESH).clicked() {
                acts.push(Act::Refresh(side));
            }
            if ui.button(i18n::FILES_HOME).clicked() {
                acts.push(Act::Home(side));
            }
            ui.checkbox(&mut pane.show_hidden, i18n::FILES_HIDDEN)
                .on_hover_text(i18n::FILES_HIDDEN_HINT);
        });
        let r = ui.add(
            egui::TextEdit::singleline(&mut pane.edit)
                .hint_text(i18n::FILES_PATH_HINT)
                .font(TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
        let field_name = format!("{} ({})", i18n::FILES_PATH_LABEL, cx.label);
        r.widget_info(|| text_info(&field_name, &pane.edit));
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let p = pane.edit.trim().to_string();
            if p.is_empty() {
                pane.edit = pane.path.clone();
            } else {
                acts.push(Act::Navigate(side, p));
            }
        } else if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            pane.edit = pane.path.clone();
        }

        header_row(ui, pane);
        list_area(ui, pane, cx, acts);
    });
}

/// Larguras das colunas de dados; o nome ocupa o resto.
struct Cols {
    name: Rect,
    size: Rect,
    date: Rect,
    kind: Option<Rect>,
}

fn cols(row: Rect) -> Cols {
    let w = row.width();
    let (size_w, date_w, kind_w) = (74.0, 106.0, 58.0);
    let show_kind = w > 440.0;
    let mut right = row.right() - 6.0;
    let mut take = |width: f32| {
        let r = Rect::from_min_max(pos2(right - width, row.top()), pos2(right, row.bottom()));
        right -= width + 6.0;
        r
    };
    let kind = show_kind.then(|| take(kind_w));
    let date = take(date_w);
    let size = take(size_w);
    let name = Rect::from_min_max(
        pos2(row.left() + 6.0, row.top()),
        pos2(right.max(row.left() + 40.0), row.bottom()),
    );
    Cols {
        name,
        size,
        date,
        kind,
    }
}

fn header_row(ui: &mut egui::Ui, pane: &mut PaneState) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let c = cols(rect);
    let mut items = vec![
        (SortKey::Name, i18n::FILES_COL_NAME, {
            let mut n = c.name;
            n.min.x += 20.0;
            n
        }),
        (SortKey::Size, i18n::FILES_COL_SIZE, c.size),
        (SortKey::Modified, i18n::FILES_COL_MODIFIED, c.date),
    ];
    if let Some(k) = c.kind {
        items.push((SortKey::Kind, i18n::FILES_COL_KIND, k));
    }
    for (key, label, cell) in items {
        let arrow = if pane.sort.key == key {
            if pane.sort.asc { " ▲" } else { " ▼" }
        } else {
            ""
        };
        let text = format!("{label}{arrow}");
        let id = ui.id().with(("hdr", key as u8));
        let r = ui.interact(cell, id, Sense::click());
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text.clone()));
        let font = FontId::proportional(12.0);
        let color = if r.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        };
        let align = if key == SortKey::Size {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        clipped_text(ui, cell, align, &text, color, &font);
        if r.clicked() {
            pane.sort.toggle(key);
            pane.resort();
        }
    }
    ui.separator();
}

fn list_area(ui: &mut egui::Ui, pane: &mut PaneState, cx: &ColCtx, acts: &mut Vec<Act>) {
    let side = pane.side;
    let list_id = Id::new((cx.uid, side, "list"));
    let area = ui.available_rect_before_wrap();
    let list_resp = ui.interact(area, list_id, Sense::click());
    // Só clique de ponteiro: Enter/Espaço no widget focado também gera `clicked()`.
    if list_resp.clicked() && list_resp.interact_pointer_pos().is_some() {
        ui.memory_mut(|m| m.request_focus(list_id));
        pane.sel.clear();
    }
    if cx.focus_request {
        ui.memory_mut(|m| m.request_focus(list_id));
    }
    let focused = ui.memory(|m| m.has_focus(list_id));
    if focused {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                list_id,
                EventFilter {
                    tab: false,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });
    }
    let n_sel = pane.sel.names.len();
    let can_transfer = cx.can_transfer;
    list_resp.context_menu(|ui| {
        if ui.button(i18n::FILES_NEW_DIR).clicked() {
            acts.push(Act::NewDir(side));
            ui.close();
        }
        if ui.button(i18n::FILES_REFRESH).clicked() {
            acts.push(Act::Refresh(side));
            ui.close();
        }
    });

    if pane.loading && pane.entries.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(i18n::FILES_LOADING);
        });
    }
    if let Some(err) = pane.error.clone() {
        ui.horizontal_wrapped(|ui| {
            let c = if ui.visuals().dark_mode {
                Color32::from_rgb(255, 120, 110)
            } else {
                Color32::from_rgb(180, 30, 30)
            };
            ui.colored_label(c, format!("⚠ {err}"));
            if ui.button(i18n::FILES_RETRY).clicked() {
                acts.push(Act::Refresh(side));
            }
        });
    }

    let vis = pane.visible();
    if vis.is_empty() && !pane.loading && pane.error.is_none() {
        ui.weak(i18n::FILES_EMPTY);
    }
    let entries = std::mem::take(&mut pane.entries);
    let names: Vec<&str> = vis.iter().map(|i| entries[*i].name.as_str()).collect();

    // Teclado (só com o foco na lista).
    if focused && cx.keys {
        let sel = &mut pane.sel;
        let mut moved = false;
        ui.input_mut(|i| {
            for (key, delta) in [
                (Key::ArrowDown, 1isize),
                (Key::ArrowUp, -1),
                (Key::PageDown, 10),
                (Key::PageUp, -10),
            ] {
                if i.consume_key(Modifiers::SHIFT, key) {
                    sel.move_cursor(delta, &names, true);
                    moved = true;
                } else if i.consume_key(Modifiers::NONE, key) {
                    sel.move_cursor(delta, &names, false);
                    moved = true;
                }
            }
            if !names.is_empty() {
                if i.consume_key(Modifiers::SHIFT, Key::Home) {
                    sel.go_to(0, &names, true);
                    moved = true;
                } else if i.consume_key(Modifiers::NONE, Key::Home) {
                    sel.go_to(0, &names, false);
                    moved = true;
                }
                if i.consume_key(Modifiers::SHIFT, Key::End) {
                    sel.go_to(names.len() - 1, &names, true);
                    moved = true;
                } else if i.consume_key(Modifiers::NONE, Key::End) {
                    sel.go_to(names.len() - 1, &names, false);
                    moved = true;
                }
            }
            if i.consume_key(Modifiers::COMMAND, Key::A) {
                sel.select_all(&names);
            }
            if i.consume_key(Modifiers::NONE, Key::Enter)
                && let Some(c) = sel.cursor.and_then(|c| vis.get(c))
            {
                let e = &entries[*c];
                if matches!(e.kind, Kind::Dir | Kind::Symlink) {
                    acts.push(Act::Open(side, e.path.clone()));
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Backspace) {
                acts.push(Act::Up(side));
            }
            if i.consume_key(Modifiers::NONE, Key::Delete) && !sel.names.is_empty() {
                acts.push(Act::Delete(side));
            }
            if i.consume_key(Modifiers::NONE, Key::F2) && sel.names.len() == 1 {
                acts.push(Act::Rename(side));
            }
        });
        pane.scroll_to_cursor |= moved;
    }

    let mut scroll = egui::ScrollArea::vertical()
        .id_salt((cx.uid, side, "rows"))
        .auto_shrink([false, false]);
    if pane.scroll_to_cursor {
        pane.scroll_to_cursor = false;
        if let Some(c) = pane.sel.cursor {
            let top = c as f32 * ROW_H;
            let (y, h) = (pane.scroll_y, pane.view_h.max(ROW_H));
            if top < y {
                scroll = scroll.vertical_scroll_offset(top);
            } else if top + ROW_H > y + h {
                scroll = scroll.vertical_scroll_offset(top + ROW_H - h);
            }
        }
    }
    let font = TextStyle::Body.resolve(ui.style());
    let out = scroll.show_rows(ui, ROW_H, vis.len(), |ui, range| {
        for vi in range {
            let e = &entries[vis[vi]];
            let selected = pane.sel.contains(&e.name);
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
            let desc = format!(
                "{}, {}{}",
                e.name,
                kind_label(e.kind),
                if e.kind == Kind::File {
                    format!(", {}", fmt_size(e.size))
                } else {
                    String::new()
                }
            );
            resp.widget_info(|| {
                WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, desc.clone())
            });
            let mods = ui.input(|i| i.modifiers);
            if resp.clicked() || resp.secondary_clicked() && !selected {
                ui.memory_mut(|m| m.request_focus(list_id));
                let toggle = resp.clicked() && mods.command;
                let range = resp.clicked() && mods.shift;
                pane.sel.click(vi, &names, toggle, range);
            }
            if resp.drag_started() {
                if !selected {
                    pane.sel.click(vi, &names, false, false);
                }
                ui.memory_mut(|m| m.request_focus(list_id));
            }
            resp.dnd_set_drag_payload(DragPayload { from: side });
            if resp.double_clicked() && matches!(e.kind, Kind::Dir | Kind::Symlink) {
                acts.push(Act::Open(side, e.path.clone()));
            }

            // Fundo / foco visível.
            let v = ui.visuals();
            if pane.sel.contains(&e.name) {
                ui.painter().rect_filled(rect, 2.0, v.selection.bg_fill);
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 2.0, v.widgets.hovered.weak_bg_fill);
            }
            if focused && pane.sel.cursor == Some(vi) {
                ui.painter().rect_stroke(
                    rect.shrink(0.5),
                    2.0,
                    Stroke::new(1.5, v.selection.stroke.color),
                    StrokeKind::Inside,
                );
            }
            let text_color = if pane.sel.contains(&e.name) {
                v.selection.stroke.color
            } else {
                v.text_color()
            };
            let weak = v.weak_text_color();
            let c = cols(rect);
            let mut name_cell = c.name;
            name_cell.min.x += 20.0;
            let icon_cell = Rect::from_min_size(c.name.min, vec2(18.0, ROW_H));
            paint_icon(ui, icon_cell, e.kind, text_color);
            clipped_text(
                ui,
                name_cell,
                Align2::LEFT_CENTER,
                &e.name,
                text_color,
                &font,
            );
            if e.kind == Kind::File {
                clipped_text(
                    ui,
                    c.size,
                    Align2::RIGHT_CENTER,
                    &fmt_size(e.size),
                    weak,
                    &font,
                );
            }
            clipped_text(
                ui,
                c.date,
                Align2::LEFT_CENTER,
                &fmt_mtime(e.mtime),
                weak,
                &font,
            );
            if let Some(k) = c.kind {
                clipped_text(ui, k, Align2::LEFT_CENTER, kind_label(e.kind), weak, &font);
            }

            resp.context_menu(|ui| {
                let (up, down) = (side == Side::Local, side == Side::Remote);
                if (up || down)
                    && ui
                        .add_enabled(
                            can_transfer,
                            egui::Button::new(if up {
                                i18n::FILES_CM_UPLOAD
                            } else {
                                i18n::FILES_CM_DOWNLOAD
                            }),
                        )
                        .clicked()
                {
                    acts.push(Act::Transfer(side));
                    ui.close();
                }
                if ui.button(i18n::FILES_NEW_DIR).clicked() {
                    acts.push(Act::NewDir(side));
                    ui.close();
                }
                if ui
                    .add_enabled(n_sel <= 1, egui::Button::new(i18n::FILES_RENAME))
                    .clicked()
                {
                    acts.push(Act::Rename(side));
                    ui.close();
                }
                if ui.button(i18n::FILES_DELETE).clicked() {
                    acts.push(Act::Delete(side));
                    ui.close();
                }
                if ui.button(i18n::FILES_REFRESH).clicked() {
                    acts.push(Act::Refresh(side));
                    ui.close();
                }
            });
        }
    });
    pane.scroll_y = out.state.offset.y;
    pane.view_h = out.inner_rect.height();
    pane.entries = entries;

    if focused {
        ui.painter().rect_stroke(
            area.shrink(0.5),
            3.0,
            Stroke::new(1.5, ui.visuals().selection.stroke.color),
            StrokeKind::Inside,
        );
    }
}
