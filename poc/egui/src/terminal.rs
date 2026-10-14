//! Sessão de terminal sem UI: junta `SessionHandle` (rede) + `alacritty_terminal::Term`.
//! É o que a GUI e os testes de integração compartilham.

use std::sync::mpsc as std_mpsc;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

use crate::session::{HostKeyPrompt, SessionCmd, SessionEvent, SessionHandle};

/// Histórico de rolagem limitado (memória previsível por sessão).
pub const SCROLLBACK_LINES: usize = 5_000;

pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Mensagens do `Term` para quem o possui. Só repassa o que é seguro:
/// respostas de protocolo ao servidor. Clipboard/título/URL da saída remota são ignorados.
pub enum ListenerMsg {
    PtyWrite(String),
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send>),
    SizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send>),
}

pub struct Listener(std_mpsc::Sender<ListenerMsg>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let msg = match event {
            Event::PtyWrite(s) => ListenerMsg::PtyWrite(s),
            Event::ColorRequest(i, f) => ListenerMsg::ColorRequest(i, f),
            Event::TextAreaSizeRequest(f) => ListenerMsg::SizeRequest(f),
            // ClipboardStore/ClipboardLoad (OSC 52), Title, Bell etc.: deliberadamente ignorados.
            _ => return,
        };
        let _ = self.0.send(msg);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting,
    AwaitingHostKey,
    Connected,
    Closed(Option<String>),
}

pub struct TermSession {
    pub term: Term<Listener>,
    parser: Processor,
    listener_rx: std_mpsc::Receiver<ListenerMsg>,
    handle: SessionHandle,
    pub status: Status,
    pub prompt: Option<HostKeyPrompt>,
    pub cols: u16,
    pub rows: u16,
    /// Tamanho da célula em pixels físicos (para respostas de tamanho de janela).
    pub cell_px: (u16, u16),
    pub bytes_received: u64,
}

pub fn term_config() -> Config {
    Config {
        scrolling_history: SCROLLBACK_LINES,
        osc52: Osc52::Disabled,
        ..Config::default()
    }
}

impl TermSession {
    pub fn new(handle: SessionHandle, cols: u16, rows: u16) -> Self {
        let (tx, rx) = std_mpsc::channel();
        let term = Term::new(
            term_config(),
            &TermSize {
                cols: cols as usize,
                rows: rows as usize,
            },
            Listener(tx),
        );
        Self {
            term,
            parser: Processor::new(),
            listener_rx: rx,
            handle,
            status: Status::Connecting,
            prompt: None,
            cols,
            rows,
            cell_px: (8, 16),
            bytes_received: 0,
        }
    }

    /// Processa até ~`budget` bytes de saída remota. Retorna `true` se ainda há mais
    /// pendente (o chamador deve pedir novo repaint em vez de travar a UI).
    pub fn pump(&mut self, budget: usize) -> bool {
        let mut used = 0usize;
        let mut more = false;
        loop {
            if used >= budget {
                more = true;
                break;
            }
            match self.handle.evt_rx.try_recv() {
                Ok(SessionEvent::Data(bytes)) => {
                    used += bytes.len();
                    self.bytes_received += bytes.len() as u64;
                    self.parser.advance(&mut self.term, &bytes);
                }
                Ok(SessionEvent::HostKeyPrompt(p)) => {
                    self.prompt = Some(p);
                    self.status = Status::AwaitingHostKey;
                }
                Ok(SessionEvent::Connected) => self.status = Status::Connected,
                Ok(SessionEvent::Closed(err)) => {
                    self.prompt = None;
                    self.status = Status::Closed(err);
                }
                Err(_) => break,
            }
        }
        self.flush_listener();
        more
    }

    fn flush_listener(&mut self) {
        while let Ok(msg) = self.listener_rx.try_recv() {
            let reply = match msg {
                ListenerMsg::PtyWrite(s) => s,
                ListenerMsg::ColorRequest(idx, f) => f(resolve_index(idx, self.term.colors())),
                ListenerMsg::SizeRequest(f) => f(WindowSize {
                    num_lines: self.rows,
                    num_cols: self.cols,
                    cell_width: self.cell_px.0,
                    cell_height: self.cell_px.1,
                }),
            };
            self.send_input(reply.into_bytes());
        }
    }

    /// Envia bytes ao servidor sem bloquear a UI (descarta se o canal estiver cheio).
    pub fn send_input(&self, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() {
            return true;
        }
        self.handle
            .cmd_tx
            .try_send(SessionCmd::Input(bytes))
            .is_ok()
    }

    pub fn answer_host_key(&mut self, accept: bool) {
        if let Some(mut p) = self.prompt.take() {
            p.answer(accept);
            self.status = if accept {
                Status::Connecting
            } else {
                Status::Closed(Some("host rejeitado".into()))
            };
        }
    }

    /// Redimensiona o `Term` e avisa o servidor (`window_change`).
    pub fn resize(&mut self, cols: u16, rows: u16, cell_px: (u16, u16)) {
        self.cell_px = cell_px;
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.resize(TermSize {
            cols: cols as usize,
            rows: rows as usize,
        });
        let _ = self.handle.cmd_tx.try_send(SessionCmd::Resize {
            cols,
            rows,
            pix_w: cols.saturating_mul(cell_px.0),
            pix_h: rows.saturating_mul(cell_px.1),
        });
    }

    pub fn close(&self) {
        let _ = self.handle.cmd_tx.try_send(SessionCmd::Close);
    }

    pub fn is_alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Linhas visíveis como texto (sem espaços finais).
    pub fn screen_lines(&self) -> Vec<String> {
        let grid = self.term.grid();
        (0..self.rows as i32)
            .map(|r| {
                let mut s = String::new();
                for c in 0..self.cols as usize {
                    let cell = &grid[Line(r)][Column(c)];
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }
                    s.push(cell.c);
                }
                s.trim_end().to_string()
            })
            .collect()
    }

    pub fn screen_text(&self) -> String {
        self.screen_lines().join("\n")
    }
}

// ---------------------------------------------------------------- paleta

const ANSI: [(u8, u8, u8); 16] = [
    (0x1d, 0x1f, 0x21),
    (0xcc, 0x66, 0x66),
    (0xb5, 0xbd, 0x68),
    (0xf0, 0xc6, 0x74),
    (0x81, 0xa2, 0xbe),
    (0xb2, 0x94, 0xbb),
    (0x8a, 0xbe, 0xb7),
    (0xc5, 0xc8, 0xc6),
    (0x66, 0x69, 0x6b),
    (0xd5, 0x4e, 0x53),
    (0xb9, 0xca, 0x4a),
    (0xe7, 0xc5, 0x47),
    (0x7a, 0xa6, 0xda),
    (0xc3, 0x97, 0xd8),
    (0x70, 0xc0, 0xb1),
    (0xea, 0xea, 0xea),
];
pub const DEFAULT_FG: Rgb = Rgb {
    r: 0xc5,
    g: 0xc8,
    b: 0xc6,
};
pub const DEFAULT_BG: Rgb = Rgb {
    r: 0x1d,
    g: 0x1f,
    b: 0x21,
};

fn rgb((r, g, b): (u8, u8, u8)) -> Rgb {
    Rgb { r, g, b }
}

fn dim(c: Rgb) -> Rgb {
    Rgb {
        r: (c.r as u16 * 2 / 3) as u8,
        g: (c.g as u16 * 2 / 3) as u8,
        b: (c.b as u16 * 2 / 3) as u8,
    }
}

/// Resolve um índice de cor do alacritty (0..=255 xterm, 256+ nomeadas) para RGB.
pub fn resolve_index(idx: usize, overrides: &Colors) -> Rgb {
    if idx < 269 {
        if let Some(c) = overrides[idx] {
            return c;
        }
    }
    match idx {
        0..=15 => rgb(ANSI[idx]),
        16..=231 => {
            let i = idx - 16;
            let lvl = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            Rgb {
                r: lvl(i / 36),
                g: lvl((i / 6) % 6),
                b: lvl(i % 6),
            }
        }
        232..=255 => {
            let v = (8 + (idx - 232) * 10) as u8;
            Rgb { r: v, g: v, b: v }
        }
        256 | 267 => DEFAULT_FG,
        257 => DEFAULT_BG,
        258 => DEFAULT_FG,
        259..=266 => dim(rgb(ANSI[idx - 259])),
        268 => dim(DEFAULT_FG),
        _ => DEFAULT_FG,
    }
}

/// Cor efetiva de uma célula (fg ou bg), tratando BOLD->brilhante e DIM.
pub fn resolve_color(color: Color, overrides: &Colors, flags: Flags, is_fg: bool) -> Rgb {
    match color {
        Color::Spec(c) => {
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
            let c = resolve_index(i as usize, overrides);
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
                return resolve_index(259 + n as usize, overrides); // variante "dim" nomeada
            }
            if is_fg && n == NamedColor::Foreground && flags.contains(Flags::DIM) {
                return resolve_index(268, overrides);
            }
            resolve_index(idx, overrides)
        }
    }
}
