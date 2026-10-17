//! Widget egui que desenha a grade do `alacritty_terminal::Term` e traduz a entrada.
//!
//! Atalhos (egui-winit converte Cmd/Ctrl+C/V/X em `Copy`/`Paste`/`Cut`):
//! - Copiar: Cmd+C (macOS) / Ctrl+Shift+C (Win/Linux). Sem seleção, Ctrl+C envia `^C`.
//! - Colar: Cmd+V (macOS) / Ctrl+V e Ctrl+Shift+V (Win/Linux). `^V` literal não tem atalho.
//! - Rolagem: roda do mouse, Shift+PageUp/PageDown, Shift+Home/End (fora da tela alternativa).
//! - Mouse: arrastar seleciona; duplo clique seleciona a palavra; triplo clique, a linha.

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use egui::{
    Align2, Color32, Event, EventFilter, FontId, Id, ImeEvent, Key, MouseWheelUnit, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, Ui, Vec2, accesskit,
};

use crate::keys::encode_key;
use crate::session::{DEFAULT_PUMP_BUDGET, TermSession};
use crate::theme::{TermStyle, TermTheme};

/// Resultado de um frame do terminal.
pub struct TermResponse {
    /// Resposta egui da área do terminal.
    pub response: Response,
    /// Texto multilinha colado que a UI deve confirmar (ver `TermSession::set_confirm_multiline`).
    /// Depois da confirmação, chamar `TermSession::paste_confirmed`.
    pub paste_needs_confirm: Option<String>,
    /// Houve cópia para a área de transferência neste frame.
    pub copied: bool,
    /// Colunas x linhas atuais.
    pub size: (u16, u16),
}

/// Largura/altura da célula em pontos, alinhadas à grade de pixels físicos.
pub fn cell_metrics(ctx: &egui::Context, font: &FontId) -> (f32, f32) {
    let ppp = ctx.pixels_per_point();
    let (w, h) = ctx.fonts_mut(|f| (f.glyph_width(font, 'M'), f.row_height(font)));
    let snap = |v: f32| (v * ppp).round().max(1.0) / ppp;
    (snap(w), snap(h))
}

fn dim(c: Color32) -> Color32 {
    Color32::from_rgb(
        (c.r() as u16 * 2 / 3) as u8,
        (c.g() as u16 * 2 / 3) as u8,
        (c.b() as u16 * 2 / 3) as u8,
    )
}

fn rgb(c: alacritty_terminal::vte::ansi::Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// Resolve um índice de cor do alacritty (0..=255 xterm, 256+ nomeadas) para cor do tema.
pub fn resolve_index(idx: usize, overrides: &Colors, theme: &TermTheme) -> Color32 {
    if idx < 269
        && let Some(c) = overrides[idx]
    {
        return rgb(c);
    }
    match idx {
        0..=15 => theme.ansi[idx],
        16..=231 => {
            let i = idx - 16;
            let lvl = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            Color32::from_rgb(lvl(i / 36), lvl((i / 6) % 6), lvl(i % 6))
        }
        232..=255 => {
            let v = (8 + (idx - 232) * 10) as u8;
            Color32::from_rgb(v, v, v)
        }
        256 | 267 => theme.fg,
        257 => theme.bg,
        258 => theme.cursor,
        259..=266 => dim(theme.ansi[idx - 259]),
        268 => dim(theme.fg),
        _ => theme.fg,
    }
}

/// Cor efetiva de uma célula (fg ou bg), tratando BOLD->brilhante e DIM.
pub fn resolve_color(
    color: Color,
    overrides: &Colors,
    flags: Flags,
    is_fg: bool,
    theme: &TermTheme,
) -> Color32 {
    match color {
        Color::Spec(c) => {
            let c = rgb(c);
            if is_fg && flags.contains(Flags::DIM) {
                dim(c)
            } else {
                c
            }
        }
        Color::Indexed(i) => {
            let i = if is_fg && flags.contains(Flags::BOLD) && i < 8 {
                i + 8
            } else {
                i
            };
            let c = resolve_index(i as usize, overrides, theme);
            if is_fg && flags.contains(Flags::DIM) {
                dim(c)
            } else {
                c
            }
        }
        Color::Named(n) => {
            let mut idx = n as usize;
            if is_fg && flags.contains(Flags::BOLD) && idx < 8 {
                idx += 8;
            }
            if is_fg && flags.contains(Flags::DIM) && idx < 8 {
                return resolve_index(259 + n as usize, overrides, theme);
            }
            if is_fg && n == NamedColor::Foreground && flags.contains(Flags::DIM) {
                return resolve_index(268, overrides, theme);
            }
            resolve_index(idx, overrides, theme)
        }
    }
}

/// Desenha o terminal ocupando todo o espaço disponível e processa a entrada.
/// Processa a saída remota (com orçamento), redimensiona o `Term`/PTY conforme o espaço.
pub fn show(ui: &mut Ui, sess: &mut TermSession, style: &TermStyle) -> TermResponse {
    sess.set_theme(&style.theme);
    if sess.pump(DEFAULT_PUMP_BUDGET) {
        ui.ctx().request_repaint(); // ainda há saída pendente: continua no próximo frame
    }
    if let Some(left) = sess.sync_deadline() {
        // Synchronized update aberto: repinta quando o prazo vencer (o servidor pode nunca fechar).
        ui.ctx()
            .request_repaint_after(left + std::time::Duration::from_millis(5));
    }

    let font = &style.font;
    let theme = &style.theme;
    let (cw, ch) = cell_metrics(ui.ctx(), font);
    let ppp = ui.ctx().pixels_per_point();
    let avail = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(avail, Sense::hover());
    let id = Id::new(("myssh-terminal", sess.id()));
    let resp = ui.interact(rect, id, Sense::click_and_drag());

    let cols = ((rect.width() / cw).floor() as i64).clamp(2, 500) as u16;
    let rows = ((rect.height() / ch).floor() as i64).clamp(1, 300) as u16;
    sess.resize(
        cols,
        rows,
        ((cw * ppp).round() as u16, (ch * ppp).round() as u16),
    );

    // Foco: toma o foco se ninguém tem (ex.: após fechar um diálogo) ou ao clicar.
    if resp.clicked() || resp.drag_started() || ui.memory(|m| m.focused().is_none()) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let focused = ui.memory(|m| m.has_focus(id));
    let accepts_input = sess.accepts_input();
    let mut out = InputOutcome::default();
    if focused {
        ui.memory_mut(|m| {
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
    }
    // Seleção/rolagem funcionam mesmo desconectado; teclado só com a sessão conectada.
    handle_input(
        ui,
        sess,
        &resp,
        rect,
        (cw, ch),
        focused && accepts_input,
        &mut out,
    );

    // ---------------------------------------------------------------- desenho
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.bg);
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

    struct Glyph {
        pos: Pos2,
        text: String,
        color: Color32,
        bold: bool,
        underline: bool,
        strike: bool,
        width: f32,
    }
    let mut bg_run: Option<(usize, usize, usize, Color32)> = None; // row, start, end(excl), cor
    let mut glyphs: Vec<Glyph> = Vec::new();

    for item in content.display_iter {
        let cell = &item.cell;
        let row = item.point.line.0 + offset;
        if row < 0 || row >= rows as i32 || item.point.column.0 >= cols as usize {
            continue;
        }
        let (row, col) = (row as usize, item.point.column.0);
        let mut fg = resolve_color(cell.fg, overrides, cell.flags, true, theme);
        let mut bg = resolve_color(cell.bg, overrides, cell.flags, false, theme);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let mut bgc = bg;
        if selection.is_some_and(|s| s.contains(item.point)) {
            bgc = theme.selection;
        }

        // Fundo: agrupa células consecutivas de mesma cor.
        if bgc != theme.bg {
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

        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.flags.contains(Flags::HIDDEN) {
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
        glyphs.push(Glyph {
            pos: cell_rect(row, col, 1).min,
            text,
            color: fg,
            bold: cell.flags.contains(Flags::BOLD),
            underline,
            strike,
            width: cw * n,
        });
    }
    if let Some((r, s, e, c)) = bg_run.take() {
        painter.rect_filled(cell_rect(r, s, e - s), 0.0, c);
    }

    // Cursor (bloco) desenhado entre fundo e texto; o glifo sob ele é redesenhado invertido.
    let cursor_pos =
        (cursor.point.line.0 >= 0 && show_cursor && cursor.shape != CursorShape::Hidden)
            .then_some((cursor.point.line.0 as usize, cursor.point.column.0));
    if let Some((r, c)) = cursor_pos {
        let rc = cell_rect(r, c, 1);
        let col = theme.cursor;
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

    for g in glyphs {
        let mut color = g.color;
        if let Some((r, c)) = cursor_pos {
            let at = Pos2::new(origin.x + c as f32 * cw, origin.y + r as f32 * ch);
            if focused && cursor.shape == CursorShape::Block && g.pos == at {
                color = theme.bg;
            }
        }
        if let Some(bits) = braille_bits(&g.text) {
            // As fontes embutidas não têm Braille (gráficos do btop etc.): desenha os pontos.
            for dot in braille_dots(bits, Rect::from_min_size(g.pos, Vec2::new(cw, ch)), ppp) {
                painter.rect_filled(dot, 0.0, color);
            }
        } else if !g.text.is_empty() {
            painter.text(g.pos, Align2::LEFT_TOP, &g.text, font.clone(), color);
            if g.bold {
                // Sem família monoespaçada bold embutida: dupla impressão desloca 0,5px.
                painter.text(
                    g.pos + Vec2::new(0.5, 0.0),
                    Align2::LEFT_TOP,
                    &g.text,
                    font.clone(),
                    color,
                );
            }
        }
        if g.underline {
            let y = g.pos.y + ch - 1.5;
            painter.line_segment(
                [Pos2::new(g.pos.x, y), Pos2::new(g.pos.x + g.width, y)],
                Stroke::new(1.0, color),
            );
        }
        if g.strike {
            let y = g.pos.y + ch * 0.5;
            painter.line_segment(
                [Pos2::new(g.pos.x, y), Pos2::new(g.pos.x + g.width, y)],
                Stroke::new(1.0, color),
            );
        }
    }

    // IME: informa onde está o cursor para o popup de candidatos / dead keys.
    if focused && accepts_input {
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

    TermResponse {
        response: resp,
        paste_needs_confirm: out.paste_needs_confirm,
        copied: out.copied,
        size: (cols, rows),
    }
}

/// Padrão de pontos de um caractere Braille (U+2800..=U+28FF) isolado, sem combinantes.
fn braille_bits(text: &str) -> Option<u8> {
    let mut it = text.chars();
    let c = it.next()? as u32;
    (it.next().is_none() && (0x2800..=0x28FF).contains(&c)).then(|| (c - 0x2800) as u8)
}

/// Retângulos dos pontos ligados de um padrão Braille numa célula, alinhados aos pixels.
/// Grade 2x4; bits 0-2 e 6 na coluna esquerda (de cima para baixo), 3-5 e 7 na direita.
fn braille_dots(bits: u8, cell: Rect, ppp: f32) -> Vec<Rect> {
    const POS: [(usize, usize); 8] = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let snap = |v: f32| (v * ppp).round() / ppp;
    let (sw, sh) = (cell.width() / 2.0, cell.height() / 4.0);
    let side = (snap(sw.min(sh) * 0.6)).max(1.0 / ppp);
    POS.iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .map(|(_, &(x, y))| {
            let cx = cell.min.x + sw * (x as f32 + 0.5);
            let cy = cell.min.y + sh * (y as f32 + 0.5);
            let min = Pos2::new(snap(cx - side / 2.0), snap(cy - side / 2.0));
            Rect::from_min_size(min, Vec2::splat(side))
        })
        .collect()
}

#[derive(Default)]
struct InputOutcome {
    paste_needs_confirm: Option<String>,
    copied: bool,
}

/// Ponto da grade (e lado da célula) sob a posição do ponteiro.
fn to_point(
    pos: Pos2,
    rect: Rect,
    (cw, ch): (f32, f32),
    cols: u16,
    rows: u16,
    offset: usize,
) -> (Point, Direction) {
    let x = (pos.x - rect.min.x).max(0.0);
    let y = (pos.y - rect.min.y).max(0.0);
    let col = ((x / cw) as usize).min(cols as usize - 1);
    let row = ((y / ch) as i32).min(rows as i32 - 1);
    let side = if (x / cw).fract() < 0.5 {
        Direction::Left
    } else {
        Direction::Right
    };
    (Point::new(Line(row - offset as i32), Column(col)), side)
}

fn handle_input(
    ui: &Ui,
    sess: &mut TermSession,
    resp: &Response,
    rect: Rect,
    (cw, ch): (f32, f32),
    keyboard: bool,
    out: &mut InputOutcome,
) {
    let mode = *sess.term.mode();
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let alt_screen = mode.contains(TermMode::ALT_SCREEN);
    let (cols, rows) = sess.size();
    let mut sent = false;

    // ---- seleção com o mouse (local; independe de a sessão aceitar teclado)
    let offset = sess.term.grid().display_offset();
    let at = |pos: Pos2| to_point(pos, rect, (cw, ch), cols, rows, offset);
    if resp.triple_clicked_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (p, s) = at(pos);
            sess.begin_selection(SelectionType::Lines, p, s);
        }
    } else if resp.double_clicked_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (p, s) = at(pos);
            sess.begin_selection(SelectionType::Semantic, p, s);
        }
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (p, s) = at(pos);
            sess.begin_selection(SelectionType::Simple, p, s);
        }
    } else if resp.dragged_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            // Arrastar para fora do topo/base rola o histórico.
            if pos.y < rect.min.y {
                sess.term.scroll_display(Scroll::Delta(1));
            } else if pos.y > rect.max.y {
                sess.term.scroll_display(Scroll::Delta(-1));
            }
            let (p, s) = at(pos);
            sess.update_selection(p, s);
        }
    } else if resp.clicked_by(egui::PointerButton::Primary) {
        sess.clear_selection();
    }

    let modifiers_now = ui.input(|i| i.modifiers);
    let events = ui.input(|i| i.events.clone());
    for ev in events {
        match ev {
            // ---- rolagem (também sem foco: apenas com o mouse sobre o terminal)
            Event::MouseWheel { unit, delta, .. } if resp.hovered() => {
                let lines = match unit {
                    MouseWheelUnit::Line => delta.y,
                    MouseWheelUnit::Point => delta.y / ch,
                    MouseWheelUnit::Page => delta.y * rows as f32,
                };
                let n = lines.round() as i32;
                if n == 0 {
                    continue;
                }
                if alt_screen {
                    if keyboard && mode.contains(TermMode::ALTERNATE_SCROLL) {
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
            // ---- copiar: com seleção copia; sem seleção, ^C (exceto Ctrl+Shift+C / macOS)
            Event::Copy if resp.has_focus() || keyboard => {
                if let Some(text) = sess.selection_text().filter(|t| !t.is_empty()) {
                    ui.ctx().copy_text(text);
                    out.copied = true;
                } else if keyboard && !cfg!(target_os = "macos") && !modifiers_now.shift {
                    sent |= sess.send_input(vec![0x03]);
                }
            }
            _ if !keyboard => {}
            // ---- entrada de teclado (só com foco e sessão conectada)
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
                // Shift+PageUp/PageDown/Home/End: rolagem do histórico (fora da tela alternativa).
                if modifiers.shift && !modifiers.ctrl && !modifiers.alt && !alt_screen {
                    let scroll = match key {
                        Key::PageUp => Some(Scroll::PageUp),
                        Key::PageDown => Some(Scroll::PageDown),
                        Key::Home => Some(Scroll::Top),
                        Key::End => Some(Scroll::Bottom),
                        _ => None,
                    };
                    if let Some(s) = scroll {
                        sess.term.scroll_display(s);
                        continue;
                    }
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
            Event::Cut => {
                if !cfg!(target_os = "macos") {
                    sent |= sess.send_input(vec![0x18]); // Ctrl+X
                }
            }
            Event::Paste(t) => {
                if let Some(text) = sess.paste(&t) {
                    out.paste_needs_confirm = Some(text);
                }
                sent = true; // paste já rola para o fim
            }
            _ => {}
        }
    }
    if sent {
        sess.term.scroll_display(Scroll::Bottom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_so_caractere_isolado_do_bloco() {
        assert_eq!(braille_bits("\u{2800}"), Some(0));
        assert_eq!(braille_bits("\u{28FF}"), Some(0xFF));
        assert_eq!(braille_bits("\u{2847}"), Some(0x47));
        assert_eq!(braille_bits("A"), None);
        assert_eq!(braille_bits("\u{2801}\u{0301}"), None);
        assert_eq!(braille_bits(""), None);
    }

    #[test]
    fn braille_pontos_nas_posicoes_certas() {
        let cell = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(8.0, 16.0));
        assert!(braille_dots(0, cell, 2.0).is_empty());
        assert_eq!(braille_dots(0xFF, cell, 2.0).len(), 8);
        // ponto 1 (bit 0): canto superior esquerdo; ponto 8 (bit 7): inferior direito
        let a = braille_dots(0x01, cell, 2.0)[0];
        let b = braille_dots(0x80, cell, 2.0)[0];
        assert!(cell.contains_rect(a) && cell.contains_rect(b));
        assert!(a.center().x < cell.center().x && a.center().y < cell.min.y + 4.0);
        assert!(b.center().x > cell.center().x && b.center().y > cell.max.y - 4.0);
        // ponto 7 (bit 6) fica abaixo do ponto 3 (bit 2), na coluna esquerda
        let p3 = braille_dots(0x04, cell, 2.0)[0];
        let p7 = braille_dots(0x40, cell, 2.0)[0];
        assert_eq!(p3.min.x, p7.min.x);
        assert!(p7.min.y > p3.max.y);
    }
}
