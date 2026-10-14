//! Widget egui que desenha a grade do `alacritty_terminal::Term` e traduz entrada.

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;
use egui::{
    accesskit, Align2, Color32, Event, EventFilter, FontId, Id, ImeEvent, MouseWheelUnit, Pos2,
    Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::keys::{encode_key, sanitize_paste};
use crate::terminal::{resolve_color, TermSession, DEFAULT_BG};

pub fn terminal_id() -> Id {
    Id::new("poc-terminal-widget")
}

pub fn font() -> FontId {
    FontId::monospace(14.0)
}

/// Largura/altura da célula em pontos, alinhadas à grade de pixels físicos.
pub fn cell_metrics(ctx: &egui::Context, font: &FontId) -> (f32, f32) {
    let ppp = ctx.pixels_per_point();
    let (w, h) = ctx.fonts_mut(|f| (f.glyph_width(font, 'M'), f.row_height(font)));
    let snap = |v: f32| (v * ppp).round().max(1.0) / ppp;
    (snap(w), snap(h))
}

fn rgb(c: alacritty_terminal::vte::ansi::Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// Desenha o terminal ocupando todo o espaço disponível e processa a entrada.
/// Redimensiona o `Term`/PTY conforme o espaço (cols/rows pela métrica da fonte).
pub fn show(ui: &mut Ui, sess: &mut TermSession, font: &FontId, interactive: bool) {
    let (cw, ch) = cell_metrics(ui.ctx(), font);
    let ppp = ui.ctx().pixels_per_point();
    let avail = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
    let id = terminal_id();
    let resp = ui.interact(rect, id, Sense::click_and_drag());

    let cols = ((rect.width() / cw).floor() as i64).clamp(2, 500) as u16;
    let rows = ((rect.height() / ch).floor() as i64).clamp(1, 300) as u16;
    sess.resize(
        cols,
        rows,
        ((cw * ppp).round() as u16, (ch * ppp).round() as u16),
    );

    if interactive {
        ui.memory_mut(|m| {
            m.request_focus(id);
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        handle_input(ui, sess, &resp, rect, (cw, ch));
    }

    // ---------------------------------------------------------------- desenho
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, rgb(DEFAULT_BG));
    let origin = rect.min;
    let cell_rect = |row: usize, col: usize, n: usize| {
        Rect::from_min_size(
            Pos2::new(origin.x + col as f32 * cw, origin.y + row as f32 * ch),
            Vec2::new(cw * n as f32, ch),
        )
    };

    let content = sess.term.renderable_content();
    let overrides = content.colors;
    let offset = content.display_offset as i32;
    let selection = content.selection;
    let cursor = content.cursor;
    let show_cursor = content.mode.contains(TermMode::SHOW_CURSOR) && offset == 0;

    let mut bg_run: Option<(usize, usize, usize, Color32)> = None; // row, start, end(excl), cor
    let mut glyphs: Vec<(Pos2, String, Color32, bool, bool, bool, f32)> = Vec::new();
    let default_bg = rgb(DEFAULT_BG);
    let sel_bg = Color32::from_rgb(0x44, 0x55, 0x77);

    for item in content.display_iter {
        let cell = &item.cell;
        let row = item.point.line.0 + offset;
        if row < 0 || row >= rows as i32 || item.point.column.0 >= cols as usize {
            continue;
        }
        let (row, col) = (row as usize, item.point.column.0);
        let mut fg = resolve_color(cell.fg, overrides, cell.flags, true);
        let mut bg = resolve_color(cell.bg, overrides, cell.flags, false);
        if cell.flags.contains(Flags::INVERSE) {
            // Inverso: troca fg/bg. (Bold/dim já foram aplicados ao fg original.)
            std::mem::swap(&mut fg, &mut bg);
        }
        let mut bgc = rgb(bg);
        if selection.is_some_and(|s| s.contains(item.point)) {
            bgc = sel_bg;
        }

        // Fundo: agrupa células consecutivas de mesma cor.
        let wide_spacer = cell.flags.contains(Flags::WIDE_CHAR_SPACER);
        if bgc != default_bg {
            match &mut bg_run {
                Some((r, _, end, c)) if *r == row && *end == col && *c == bgc => *end = col + 1,
                _ => {
                    if let Some((r, s, e, c)) = bg_run.take() {
                        painter.rect_filled(cell_rect(r, s, e - s), 0.0, c);
                    }
                    bg_run = Some((row, col, col + 1, bgc));
                }
            }
        } else if let Some((r, s, e, c)) = bg_run.take() {
            painter.rect_filled(cell_rect(r, s, e - s), 0.0, c);
        }

        if wide_spacer || cell.flags.contains(Flags::HIDDEN) {
            continue;
        }
        let blank = cell.c == ' ' || cell.c == '\0';
        let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);
        let strike = cell.flags.contains(Flags::STRIKEOUT);
        if blank && !underline && !strike {
            continue;
        }
        let mut text = String::new();
        if !blank {
            text.push(cell.c);
            if let Some(zw) = cell.zerowidth() {
                text.extend(zw.iter());
            }
        }
        let n = if cell.flags.contains(Flags::WIDE_CHAR) {
            2.0
        } else {
            1.0
        };
        glyphs.push((
            cell_rect(row, col, 1).min,
            text,
            rgb(fg),
            cell.flags.contains(Flags::BOLD),
            underline,
            strike,
            cw * n,
        ));
    }
    if let Some((r, s, e, c)) = bg_run.take() {
        painter.rect_filled(cell_rect(r, s, e - s), 0.0, c);
    }

    // Cursor (bloco) desenhado entre fundo e texto; o glifo sob ele é redesenhado invertido.
    let cursor_pos =
        (cursor.point.line.0 >= 0 && show_cursor && cursor.shape != CursorShape::Hidden)
            .then_some((cursor.point.line.0 as usize, cursor.point.column.0));
    let focused = resp.has_focus();
    if let Some((r, c)) = cursor_pos {
        let rc = cell_rect(r, c, 1);
        let col = rgb(crate::terminal::DEFAULT_FG);
        match (cursor.shape, focused) {
            (CursorShape::Block, true) => {
                painter.rect_filled(rc, 0.0, col);
            }
            (CursorShape::Underline, _) => {
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(rc.min.x, rc.max.y - 2.0), rc.max),
                    0.0,
                    col,
                );
            }
            (CursorShape::Beam, _) => {
                painter.rect_filled(
                    Rect::from_min_size(rc.min, Vec2::new(2.0, rc.height())),
                    0.0,
                    col,
                );
            }
            _ => {
                painter.rect_stroke(rc, 0.0, Stroke::new(1.0, col), StrokeKind::Inside);
            }
        }
    }

    for (pos, text, color, bold, underline, strike, width) in glyphs {
        let mut color = color;
        if let Some((r, c)) = cursor_pos {
            let at = Pos2::new(origin.x + c as f32 * cw, origin.y + r as f32 * ch);
            if focused && cursor.shape == CursorShape::Block && pos == at {
                color = default_bg;
            }
        }
        if !text.is_empty() {
            painter.text(pos, Align2::LEFT_TOP, &text, font.clone(), color);
            if bold {
                // Sem família monoespaçada bold embutida: dupla impressão desloca 0,5px.
                painter.text(
                    pos + Vec2::new(0.5, 0.0),
                    Align2::LEFT_TOP,
                    &text,
                    font.clone(),
                    color,
                );
            }
        }
        if underline {
            let y = pos.y + ch - 1.5;
            painter.line_segment(
                [Pos2::new(pos.x, y), Pos2::new(pos.x + width, y)],
                Stroke::new(1.0, color),
            );
        }
        if strike {
            let y = pos.y + ch * 0.5;
            painter.line_segment(
                [Pos2::new(pos.x, y), Pos2::new(pos.x + width, y)],
                Stroke::new(1.0, color),
            );
        }
    }

    // IME: informa onde está o cursor para o popup de candidatos / dead keys.
    if interactive {
        let (r, c) = cursor_pos.unwrap_or((0, 0));
        let cr = cell_rect(r, c, 1);
        ui.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Terminal,
                rect,
                cursor_rect: cr,
                should_interrupt_composition: false,
            });
        });
    }

    // Acessibilidade: expõe a tela como nó Terminal com o texto visível como valor.
    // (Só calcula o texto se o AccessKit estiver ativo.)
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_role(accesskit::Role::Terminal);
        node.set_label("Terminal SSH");
        node.set_value(sess.screen_text());
    });
}

fn handle_input(
    ui: &Ui,
    sess: &mut TermSession,
    resp: &egui::Response,
    rect: Rect,
    (cw, ch): (f32, f32),
) {
    let events = ui.input(|i| i.events.clone());
    let mode = *sess.term.mode();
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let mut sent = false;

    // Seleção com o mouse.
    let to_point = |pos: Pos2, offset: usize, rows: u16| -> (Point, Direction) {
        let x = (pos.x - rect.min.x).max(0.0);
        let y = (pos.y - rect.min.y).max(0.0);
        let col = ((x / cw) as usize).min(sess.cols as usize - 1);
        let row = ((y / ch) as i32).min(rows as i32 - 1);
        let side = if (x / cw).fract() < 0.5 {
            Direction::Left
        } else {
            Direction::Right
        };
        (Point::new(Line(row - offset as i32), Column(col)), side)
    };
    let offset = sess.term.grid().display_offset();
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (p, s) = to_point(pos, offset, sess.rows);
            sess.term.selection = Some(Selection::new(SelectionType::Simple, p, s));
        }
    } else if resp.dragged_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (p, s) = to_point(pos, offset, sess.rows);
            if let Some(sel) = sess.term.selection.as_mut() {
                sel.update(p, s);
            }
        }
    } else if resp.clicked() {
        sess.term.selection = None;
    }

    for ev in events {
        match ev {
            Event::Text(t) => {
                if !t.chars().any(|c| c.is_control()) {
                    sent |= sess.send_input(t.into_bytes());
                }
            }
            Event::Ime(ImeEvent::Commit(t)) => {
                if !t.chars().any(|c| c.is_control()) {
                    sent |= sess.send_input(t.into_bytes());
                }
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                // Cmd+tecla (macOS) pertence ao sistema/app, não ao terminal.
                if modifiers.mac_cmd {
                    continue;
                }
                if let Some(bytes) = encode_key(key, modifiers, app_cursor) {
                    sent |= sess.send_input(bytes);
                } else if modifiers.alt && !modifiers.ctrl {
                    // Alt+tecla imprimível => ESC + caractere.
                    let name = key.name();
                    if name.chars().count() == 1 {
                        let c = name.chars().next().unwrap_or(' ');
                        let c = if modifiers.shift {
                            c
                        } else {
                            c.to_ascii_lowercase()
                        };
                        let mut b = vec![0x1b];
                        b.extend_from_slice(c.to_string().as_bytes());
                        sent |= sess.send_input(b);
                    }
                }
            }
            Event::Copy => {
                if let Some(text) = sess.term.selection_to_string() {
                    ui.ctx().copy_text(text);
                } else if !cfg!(target_os = "macos") {
                    sent |= sess.send_input(vec![0x03]); // Ctrl+C
                }
            }
            Event::Cut => {
                if !cfg!(target_os = "macos") {
                    sent |= sess.send_input(vec![0x18]); // Ctrl+X
                }
            }
            Event::Paste(t) => {
                sent |=
                    sess.send_input(sanitize_paste(&t, mode.contains(TermMode::BRACKETED_PASTE)));
            }
            Event::MouseWheel { unit, delta, .. } if resp.hovered() => {
                let lines = match unit {
                    MouseWheelUnit::Line => delta.y,
                    MouseWheelUnit::Point => delta.y / ch,
                    MouseWheelUnit::Page => delta.y * sess.rows as f32,
                };
                let n = lines.round() as i32;
                if n != 0 {
                    if mode.contains(TermMode::ALT_SCREEN) {
                        if mode.contains(TermMode::ALTERNATE_SCROLL) {
                            let up = n > 0;
                            let seq: &[u8] = match (up, app_cursor) {
                                (true, true) => b"\x1bOA",
                                (true, false) => b"\x1b[A",
                                (false, true) => b"\x1bOB",
                                (false, false) => b"\x1b[B",
                            };
                            for _ in 0..n.unsigned_abs().min(10) {
                                sess.send_input(seq.to_vec());
                            }
                        }
                    } else {
                        sess.term.scroll_display(Scroll::Delta(n));
                    }
                }
            }
            _ => {}
        }
    }
    if sent {
        sess.term.scroll_display(Scroll::Bottom);
    }
}
