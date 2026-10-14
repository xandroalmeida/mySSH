//! `TermSession`: junta `myssh_ssh::SessionHandle` (rede) + `alacritty_terminal::Term`.
//! Não conhece o widget; é o que a GUI e os testes de integração compartilham.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostKey};
use myssh_ssh::{
    HostKeyDecision, HostKeyResponder, PtySize, SessionCommand, SessionEvent, SessionHandle,
};

use crate::keys::{is_multiline, sanitize_paste};
use crate::theme::TermTheme;
use crate::title::sanitize_title;
use crate::widget::resolve_index;

/// Histórico de rolagem padrão (memória previsível por sessão).
pub const DEFAULT_SCROLLBACK: usize = 5_000;
/// Orçamento padrão de bytes de saída remota processados por chamada de `pump` (por frame).
pub const DEFAULT_PUMP_BUDGET: usize = 256 * 1024;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct TermSize {
    cols: usize,
    rows: usize,
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

/// Mensagens do `Term` para quem o possui. Só repassa o que é seguro: respostas de protocolo ao
/// servidor e o título (que será sanitizado). Clipboard (OSC 52), hyperlinks, bell etc. são
/// deliberadamente ignorados.
enum ListenerMsg {
    PtyWrite(String),
    ColorRequest(
        usize,
        Arc<dyn Fn(alacritty_terminal::vte::ansi::Rgb) -> String + Sync + Send>,
    ),
    SizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send>),
    Title(String),
    ResetTitle,
}

pub struct Listener(std_mpsc::Sender<ListenerMsg>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let msg = match event {
            Event::PtyWrite(s) => ListenerMsg::PtyWrite(s),
            Event::ColorRequest(i, f) => ListenerMsg::ColorRequest(i, f),
            Event::TextAreaSizeRequest(f) => ListenerMsg::SizeRequest(f),
            Event::Title(t) => ListenerMsg::Title(t),
            Event::ResetTitle => ListenerMsg::ResetTitle,
            // ClipboardStore/ClipboardLoad (OSC 52), Bell, MouseCursorDirty etc.: ignorados.
            _ => return,
        };
        let _ = self.0.send(msg);
    }
}

/// Host key desconhecida aguardando decisão do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyPrompt {
    pub endpoint: HostEndpoint,
    pub key: HostKey,
}

/// Host key diferente da confiável (conexão já abortada). Só informativo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyAlert {
    pub endpoint: HostEndpoint,
    pub presented: HostKey,
    pub known: Vec<HostKey>,
}

pub struct TermSession {
    pub term: Term<Listener>,
    id: u64,
    parser: Processor,
    listener_rx: std_mpsc::Receiver<ListenerMsg>,
    handle: SessionHandle,
    state: SessionState,
    prompt: Option<HostKeyPrompt>,
    responder: Option<HostKeyResponder>,
    alert: Option<HostKeyAlert>,
    closed: Option<Option<String>>,
    title: Option<String>,
    cols: u16,
    rows: u16,
    /// Tamanho da célula em pixels físicos (para respostas de tamanho de janela).
    cell_px: (u16, u16),
    bytes_received: u64,
    confirm_multiline: bool,
    input_enabled: bool,
    theme: TermTheme,
}

fn term_config(scrollback: usize) -> Config {
    Config {
        scrolling_history: scrollback,
        osc52: Osc52::Disabled,
        ..Config::default()
    }
}

impl TermSession {
    pub fn new(handle: SessionHandle, cols: u16, rows: u16) -> Self {
        Self::with_scrollback(handle, cols, rows, DEFAULT_SCROLLBACK)
    }

    /// `scrollback`: linhas de histórico mantidas (limita a memória por sessão).
    pub fn with_scrollback(handle: SessionHandle, cols: u16, rows: u16, scrollback: usize) -> Self {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let (tx, rx) = std_mpsc::channel();
        let term = Term::new(
            term_config(scrollback),
            &TermSize {
                cols: cols as usize,
                rows: rows as usize,
            },
            Listener(tx),
        );
        Self {
            term,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            parser: Processor::new(),
            listener_rx: rx,
            handle,
            state: SessionState::Connecting,
            prompt: None,
            responder: None,
            alert: None,
            closed: None,
            title: None,
            cols,
            rows,
            cell_px: (8, 16),
            bytes_received: 0,
            confirm_multiline: false,
            input_enabled: true,
            theme: TermTheme::dark(),
        }
    }

    /// Identificador único (para ids de widget quando há várias sessões).
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// `Some(erro)` quando a sessão terminou (`Some(None)` = encerrada sem erro).
    pub fn closed(&self) -> Option<&Option<String>> {
        self.closed.as_ref()
    }

    pub fn is_connected(&self) -> bool {
        self.closed.is_none() && self.state == SessionState::Connected
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Título remoto sanitizado (sem controles, tamanho limitado). Apenas texto: nunca confie.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Prompt de host key desconhecida. A UI deve exibir e chamar [`Self::answer_host_key`].
    pub fn host_key_prompt(&self) -> Option<&HostKeyPrompt> {
        self.prompt.as_ref()
    }

    pub fn host_key_alert(&self) -> Option<&HostKeyAlert> {
        self.alert.as_ref()
    }

    pub fn answer_host_key(&mut self, decision: HostKeyDecision) {
        self.prompt = None;
        if let Some(r) = self.responder.take() {
            r.answer(decision);
        }
    }

    /// Pede confirmação antes de colar texto multilinha (ver [`Self::paste`]).
    pub fn set_confirm_multiline(&mut self, confirm: bool) {
        self.confirm_multiline = confirm;
    }

    /// Desliga/religa a entrada de teclado no widget (ex.: enquanto um diálogo modal está aberto).
    pub fn set_input_enabled(&mut self, enabled: bool) {
        self.input_enabled = enabled;
    }

    pub(crate) fn accepts_input(&self) -> bool {
        self.input_enabled && self.is_connected()
    }

    pub(crate) fn set_theme(&mut self, theme: &TermTheme) {
        if self.theme != *theme {
            self.theme = theme.clone();
        }
    }

    /// Processa até ~`budget` bytes de saída remota. Retorna `true` se ainda há mais pendente
    /// (o chamador deve pedir novo repaint em vez de travar a UI). Enquanto a UI não chama
    /// `pump`, o canal limitado enche e a leitura do canal SSH espera (backpressure).
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
                Ok(SessionEvent::State(s)) => self.state = s,
                Ok(SessionEvent::HostKeyUnknown {
                    endpoint,
                    key,
                    responder,
                }) => {
                    self.prompt = Some(HostKeyPrompt { endpoint, key });
                    self.responder = Some(responder);
                }
                Ok(SessionEvent::HostKeyChanged {
                    endpoint,
                    presented,
                    known,
                }) => {
                    self.alert = Some(HostKeyAlert {
                        endpoint,
                        presented,
                        known,
                    });
                }
                Ok(SessionEvent::Closed { error }) => {
                    self.prompt = None;
                    self.responder = None;
                    self.closed = Some(error);
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
                ListenerMsg::ColorRequest(idx, f) => {
                    let c = resolve_index(idx, self.term.colors(), &self.theme);
                    f(alacritty_terminal::vte::ansi::Rgb {
                        r: c.r(),
                        g: c.g(),
                        b: c.b(),
                    })
                }
                ListenerMsg::SizeRequest(f) => f(WindowSize {
                    num_lines: self.rows,
                    num_cols: self.cols,
                    cell_width: self.cell_px.0,
                    cell_height: self.cell_px.1,
                }),
                ListenerMsg::Title(t) => {
                    let t = sanitize_title(&t);
                    self.title = (!t.is_empty()).then_some(t);
                    continue;
                }
                ListenerMsg::ResetTitle => {
                    self.title = None;
                    continue;
                }
            };
            self.send_input(reply.into_bytes());
        }
    }

    /// Envia bytes ao servidor sem bloquear a UI (descarta se o canal estiver cheio ou fechado).
    pub fn send_input(&self, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() {
            return true;
        }
        self.handle
            .cmd_tx
            .try_send(SessionCommand::Input(bytes))
            .is_ok()
    }

    /// Cola `text` (já vindo do clipboard por ação explícita do usuário). Se a confirmação de
    /// multilinha estiver ligada e o texto for multilinha, devolve `Some(texto)` para a UI
    /// confirmar (depois, [`Self::paste_confirmed`]), mesmo com bracketed paste ativo.
    pub fn paste(&mut self, text: &str) -> Option<String> {
        if self.confirm_multiline && is_multiline(text) {
            return Some(text.to_string());
        }
        self.paste_confirmed(text);
        None
    }

    /// Cola sem perguntar (após confirmação do usuário).
    pub fn paste_confirmed(&mut self, text: &str) {
        let bracketed = self.term.mode().contains(TermMode::BRACKETED_PASTE);
        self.send_input(sanitize_paste(text, bracketed));
        self.term.scroll_display(Scroll::Bottom);
    }

    /// Redimensiona o `Term` e avisa o servidor (`window_change`).
    pub fn resize(&mut self, cols: u16, rows: u16, cell_px: (u16, u16)) {
        self.cell_px = cell_px;
        let (cols, rows) = (cols.max(2), rows.max(1));
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.resize(TermSize {
            cols: cols as usize,
            rows: rows as usize,
        });
        let _ = self.handle.cmd_tx.try_send(SessionCommand::Resize(PtySize {
            cols,
            rows,
            pix_w: cols.saturating_mul(cell_px.0),
            pix_h: rows.saturating_mul(cell_px.1),
        }));
    }

    pub fn disconnect(&self) {
        let _ = self.handle.cmd_tx.try_send(SessionCommand::Disconnect);
    }

    /// Linhas atualmente guardadas no histórico de rolagem.
    pub fn history_len(&self) -> usize {
        self.term.grid().history_size()
    }

    pub fn is_alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    // ------------------------------------------------------------ seleção

    /// Começa uma seleção. `Simple` = arrastar; `Semantic` = palavra; `Lines` = linha.
    pub fn begin_selection(&mut self, kind: SelectionType, p: Point, side: Direction) {
        self.term.selection = Some(Selection::new(kind, p, side));
    }

    pub fn update_selection(&mut self, p: Point, side: Direction) {
        if let Some(s) = self.term.selection.as_mut() {
            s.update(p, side);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection_text().is_some_and(|t| !t.is_empty())
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.selection_to_string()
    }

    // ------------------------------------------------------------ texto da tela

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

    pub fn overrides(&self) -> &Colors {
        self.term.colors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    /// Sessão sem rede: o teste injeta eventos pelo `evt_tx`.
    fn fake() -> (
        TermSession,
        mpsc::Sender<SessionEvent>,
        mpsc::Receiver<SessionCommand>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (evt_tx, evt_rx) = mpsc::channel(64);
        let s = TermSession::new(SessionHandle { cmd_tx, evt_rx }, 40, 6);
        (s, evt_tx, cmd_rx)
    }

    fn feed(s: &mut TermSession, tx: &mpsc::Sender<SessionEvent>, data: &[u8]) {
        tx.try_send(SessionEvent::Data(data.to_vec())).unwrap();
        s.pump(1 << 20);
    }

    fn drain(rx: &mut mpsc::Receiver<SessionCommand>) -> Vec<u8> {
        let mut out = vec![];
        while let Ok(SessionCommand::Input(b)) = rx.try_recv() {
            out.extend(b);
        }
        out
    }

    fn pt(line: i32, col: usize) -> Point {
        Point::new(Line(line), Column(col))
    }

    #[test]
    fn selecao_arrastada_vira_texto() {
        let (mut s, tx, _rx) = fake();
        feed(&mut s, &tx, b"hello world\r\nsegunda linha");
        s.begin_selection(SelectionType::Simple, pt(0, 0), Direction::Left);
        s.update_selection(pt(0, 4), Direction::Right);
        assert_eq!(s.selection_text().as_deref(), Some("hello"));
        // multilinha
        s.begin_selection(SelectionType::Simple, pt(0, 6), Direction::Left);
        s.update_selection(pt(1, 6), Direction::Right);
        assert_eq!(s.selection_text().as_deref(), Some("world\nsegunda"));
        s.clear_selection();
        assert!(!s.has_selection());
    }

    #[test]
    fn duplo_clique_seleciona_palavra() {
        let (mut s, tx, _rx) = fake();
        feed(&mut s, &tx, b"hello world foo");
        s.begin_selection(SelectionType::Semantic, pt(0, 7), Direction::Left);
        assert_eq!(s.selection_text().as_deref(), Some("world"));
    }

    #[test]
    fn titulo_remoto_e_sanitizado_e_limitado() {
        let (mut s, tx, _rx) = fake();
        feed(&mut s, &tx, b"\x1b]0;meu\x07t\xc3\xadtulo\x07");
        assert!(s.title().is_some());
        feed(&mut s, &tx, "\x1b]2;a\u{202E}b\x07".as_bytes());
        assert_eq!(s.title(), Some("ab"));
        let longo = format!("\x1b]2;{}\x07", "x".repeat(5000));
        feed(&mut s, &tx, longo.as_bytes());
        assert_eq!(
            s.title().unwrap().chars().count(),
            crate::title::MAX_TITLE_CHARS
        );
    }

    #[test]
    fn osc52_nao_escreve_clipboard_nem_responde() {
        let (mut s, tx, mut rx) = fake();
        // OSC 52: tentativa de ler (?) e de escrever no clipboard.
        feed(&mut s, &tx, b"\x1b]52;c;?\x07\x1b]52;c;aGVsbG8=\x07");
        assert!(drain(&mut rx).is_empty(), "OSC 52 não pode gerar resposta");
        assert!(s.title().is_none());
    }

    #[test]
    fn colar_respeita_bracketed_paste() {
        let (mut s, tx, mut rx) = fake();
        assert_eq!(s.paste("ls"), None);
        assert_eq!(drain(&mut rx), b"ls");
        feed(&mut s, &tx, b"\x1b[?2004h");
        assert_eq!(s.paste("a\nb\x1b"), None);
        assert_eq!(drain(&mut rx), b"\x1b[200~a\rb\x1b[201~");
    }

    #[test]
    fn colar_multilinha_pede_confirmacao_quando_configurado() {
        let (mut s, _tx, mut rx) = fake();
        assert_eq!(s.paste("a\nb"), None, "desligado por padrão");
        assert_eq!(drain(&mut rx), b"a\rb");
        s.set_confirm_multiline(true);
        assert_eq!(s.paste("uma linha"), None);
        assert_eq!(drain(&mut rx), b"uma linha");
        assert_eq!(s.paste("a\nb"), Some("a\nb".to_string()));
        assert!(drain(&mut rx).is_empty(), "nada enviado antes de confirmar");
        s.paste_confirmed("a\nb");
        assert_eq!(drain(&mut rx), b"a\rb");
    }

    #[test]
    fn pump_respeita_orcamento() {
        let (mut s, tx, _rx) = fake();
        for _ in 0..4 {
            tx.try_send(SessionEvent::Data(vec![b'x'; 1000])).unwrap();
        }
        assert!(s.pump(1000), "ainda há dados pendentes");
        assert_eq!(s.bytes_received(), 1000);
        assert!(!s.pump(1 << 20));
        assert_eq!(s.bytes_received(), 4000);
    }

    #[test]
    fn scrollback_e_limitado() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(8);
        let (evt_tx, evt_rx) = mpsc::channel(8);
        let mut s = TermSession::with_scrollback(SessionHandle { cmd_tx, evt_rx }, 20, 4, 50);
        for i in 0..500 {
            feed(&mut s, &evt_tx, format!("linha {i}\r\n").as_bytes());
        }
        assert!(s.history_len() <= 50, "{}", s.history_len());
    }

    #[test]
    fn alerta_de_chave_alterada() {
        let (mut s, tx, _rx) = fake();
        // O responder só é construível dentro do crate ssh: aqui só o alerta; o prompt é
        // coberto no teste de integração.
        let hk = HostKey {
            algorithm: "ssh-ed25519".into(),
            key_base64: "AAAA".into(),
            fingerprint_sha256: "SHA256:x".into(),
        };
        tx.try_send(SessionEvent::HostKeyChanged {
            endpoint: HostEndpoint::new("h", 22),
            presented: hk.clone(),
            known: vec![],
        })
        .unwrap();
        s.pump(10);
        assert_eq!(s.host_key_alert().unwrap().presented, hk);
    }
}
