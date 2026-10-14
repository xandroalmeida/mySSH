//! Aba de terminal: resolve credenciais (fora da thread de UI), conecta via `myssh-ssh`, desenha o
//! terminal (`myssh-term`) e mostra os diálogos de confiança de host, senha e colagem.
//!
//! Fluxo: `Idle` → `Resolving` (cofre/arquivo da chave, em `spawn_blocking`) → [`Prompt` |
//! `VaultLocked`] → `Live` (há um `TermSession`) ou `Failed` (erro antes de existir sessão).
//! Reconectar sempre cria uma sessão nova (limpa a tela) e **nunca reenvia comandos**.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{Color32, FontId, Id, Key, RichText};
use myssh_core::trust::{HostEndpoint, HostTrustStore};
use myssh_core::{AuthMethod, Credential, CredentialId, CredentialKind, Profile, SessionState};
use myssh_secrets::{SecretError, SecretString, Vault, new_secret_ref};
use myssh_ssh::{
    AUTH_REJECTED, AuthMaterial, ConnectRequest, HostKeyDecision, KEY_DECODE, Notify, PtySize,
    key_requires_passphrase, spawn,
};
use myssh_store::Store;
use myssh_term::{DEFAULT_PUMP_BUDGET, HostKeyAlert, TermSession, TermStyle, TermTheme};
use zeroize::Zeroize;

use crate::app::secret_error_text;
use crate::i18n;
use crate::tabs::{StatusBadge, TabBody, TabEnv, TabEvent};
use crate::view::warn_color;

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

/// Tamanho inicial do PTY; o widget ajusta ao desenhar e o resize chega antes do shell.
const INITIAL_COLS: u16 = 80;
const INITIAL_ROWS: u16 = 24;
const PREVIEW_MAX_LINES: usize = 8;
const PREVIEW_MAX_COLS: usize = 100;

const GREEN: Color32 = Color32::from_rgb(60, 170, 90);
const AMBER: Color32 = Color32::from_rgb(230, 170, 30);
const RED: Color32 = Color32::from_rgb(210, 60, 55);
const GRAY: Color32 = Color32::from_rgb(140, 140, 140);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretKind {
    Password,
    Passphrase,
}

struct SecretPrompt {
    kind: SecretKind,
    text: String,
    save: bool,
    can_save: bool,
    note: Option<String>,
}

impl Drop for SecretPrompt {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

/// Resultado da preparação de credenciais (calculado fora da thread de UI).
enum Resolved {
    Ready(AuthMaterial),
    Prompt {
        kind: SecretKind,
        note: Option<String>,
    },
    VaultLocked,
    Failed(String),
}

type UnlockRx = Receiver<Result<(), SecretError>>;

enum Phase {
    Idle,
    Resolving(Receiver<Resolved>),
    VaultLocked {
        error: Option<String>,
        unlocking: Option<UnlockRx>,
    },
    Prompt(SecretPrompt),
    Live,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    Paste,
    Secret,
    Vault,
    HostKey,
}

enum Act {
    Connect,
    Disconnect,
    OpenTrusted,
}

pub struct TerminalTab {
    uid: u64,
    profile: Profile,
    saved: bool,
    jump_name: Option<String>,
    phase: Phase,
    sess: Option<TermSession>,
    started: bool,
    /// A próxima conexão deve perguntar o segredo (o anterior foi recusado).
    force_prompt: bool,
    error_seen: bool,
    simulate_busy: bool,
    paste: Option<String>,
    save_rx: Option<Receiver<Result<AuthMethod, String>>>,
    notice: Option<String>,
    events: Vec<TabEvent>,
    modal_kind: Option<ModalKind>,
}

impl TerminalTab {
    pub fn new(profile: Profile, saved: bool, jump_name: Option<String>) -> Self {
        Self {
            uid: NEXT_UID.fetch_add(1, Ordering::Relaxed),
            profile,
            saved,
            jump_name,
            phase: Phase::Idle,
            sess: None,
            started: false,
            force_prompt: false,
            error_seen: false,
            simulate_busy: false,
            paste: None,
            save_rx: None,
            notice: None,
            events: Vec::new(),
            modal_kind: None,
        }
    }

    // ------------------------------------------------------------ conexão

    /// Inicia uma conexão nova (descarta a sessão anterior, se houver).
    fn connect(&mut self, env: &TabEnv) {
        self.sess = None; // soltar o handle desconecta
        self.notice = None;
        self.paste = None;
        self.error_seen = false;
        // Jump host: NUNCA conectar ignorando o salto (M2).
        if self.profile.jump.is_some() {
            self.phase = Phase::Failed(i18n::ERR_JUMP.into());
            return;
        }
        if matches!(self.profile.auth, AuthMethod::KeyboardInteractive) {
            self.phase = Phase::Failed(i18n::ERR_KI.into());
            return;
        }
        let force = std::mem::take(&mut self.force_prompt);
        self.start_resolve(env, force);
    }

    fn start_resolve(&mut self, env: &TabEnv, force: bool) {
        let (tx, rx) = channel();
        let (store, vault, ctx) = (env.store.clone(), env.vault.clone(), env.ctx.clone());
        let auth = self.profile.auth.clone();
        env.rt.spawn_blocking(move || {
            let _ = tx.send(resolve(&store, &vault, &auth, force));
            ctx.request_repaint();
        });
        self.phase = Phase::Resolving(rx);
    }

    fn on_resolved(&mut self, env: &TabEnv, r: Resolved) {
        match r {
            Resolved::Ready(auth) => self.launch(env, auth),
            Resolved::Prompt { kind, note } => {
                let can_save = self.saved
                    && matches!(
                        (&self.profile.auth, kind),
                        (
                            AuthMethod::Password { credential: None },
                            SecretKind::Password
                        ) | (
                            AuthMethod::KeyFile {
                                passphrase: None,
                                ..
                            },
                            SecretKind::Passphrase
                        )
                    );
                self.phase = Phase::Prompt(SecretPrompt {
                    kind,
                    text: String::new(),
                    save: false,
                    can_save,
                    note,
                });
            }
            Resolved::VaultLocked => {
                self.phase = Phase::VaultLocked {
                    error: None,
                    unlocking: None,
                }
            }
            Resolved::Failed(m) => self.phase = Phase::Failed(m),
        }
    }

    fn launch(&mut self, env: &TabEnv, auth: AuthMaterial) {
        let p = &self.profile;
        let req = ConnectRequest {
            endpoint: HostEndpoint::new(&p.host, p.port),
            username: p.username.clone(),
            auth,
            pty: PtySize {
                cols: INITIAL_COLS,
                rows: INITIAL_ROWS,
                pix_w: 0,
                pix_h: 0,
            },
            term: "xterm-256color".into(),
            connect_timeout: std::time::Duration::from_secs(u64::from(
                p.connect_timeout_secs.max(1),
            )),
            keepalive: p
                .keepalive_secs
                .filter(|s| *s > 0)
                .map(|s| std::time::Duration::from_secs(u64::from(s))),
        };
        let ctx = env.ctx.clone();
        let notify: Notify = Arc::new(move || ctx.request_repaint());
        let trust: Arc<dyn HostTrustStore> = env.store.clone();
        let handle = spawn(env.rt, req, trust, notify);
        self.sess = Some(TermSession::new(handle, INITIAL_COLS, INITIAL_ROWS));
        self.phase = Phase::Live;
    }

    fn submit_prompt(&mut self, env: &TabEnv) {
        let Phase::Prompt(mut p) = std::mem::replace(&mut self.phase, Phase::Idle) else {
            return;
        };
        let secret = SecretString::from(std::mem::take(&mut p.text));
        if p.save && p.can_save {
            self.spawn_save(env, p.kind, secret.clone());
        }
        let auth = match (p.kind, &self.profile.auth) {
            (SecretKind::Password, _) => AuthMaterial::Password(secret),
            (SecretKind::Passphrase, AuthMethod::KeyFile { path, .. }) => AuthMaterial::KeyFile {
                path: path.clone(),
                passphrase: Some(secret),
            },
            _ => {
                self.phase = Phase::Failed(i18n::ERR_INTERNAL.into());
                return;
            }
        };
        self.launch(env, auth);
    }

    /// Salva o segredo digitado no cofre e associa ao perfil (em segundo plano).
    fn spawn_save(&mut self, env: &TabEnv, kind: SecretKind, secret: SecretString) {
        let (tx, rx) = channel();
        self.save_rx = Some(rx);
        let (store, vault, ctx) = (env.store.clone(), env.vault.clone(), env.ctx.clone());
        let (pid, name) = (self.profile.id, self.profile.name.clone());
        env.rt.spawn_blocking(move || {
            let _ = tx.send(save_secret(&store, &vault, pid, &name, kind, secret));
            ctx.request_repaint();
        });
    }

    fn disconnect(&mut self) {
        match &self.phase {
            Phase::Live => {
                if let Some(s) = &self.sess {
                    s.disconnect();
                }
            }
            Phase::Resolving(_) | Phase::Prompt(_) | Phase::VaultLocked { .. } => {
                self.phase = Phase::Idle;
                self.notice = Some(i18n::TAB_CANCELLED.into());
            }
            _ => {}
        }
    }

    fn session_live(&self) -> bool {
        self.sess.as_ref().is_some_and(|s| s.closed().is_none())
    }

    fn unlock(&mut self, env: &TabEnv) {
        let (tx, rx) = channel();
        let (vault, ctx) = (env.vault.clone(), env.ctx.clone());
        env.rt.spawn_blocking(move || {
            let _ = tx.send(vault.unlock());
            ctx.request_repaint();
        });
        self.phase = Phase::VaultLocked {
            error: None,
            unlocking: Some(rx),
        };
    }

    // ------------------------------------------------------------ estado

    fn badge(&self) -> StatusBadge {
        let b = |icon, text, color| StatusBadge { icon, text, color };
        match &self.phase {
            Phase::Idle => b("○", i18n::ST_IDLE, GRAY),
            Phase::Resolving(_) => b("⏳", i18n::ST_PREPARING, AMBER),
            Phase::VaultLocked { .. } | Phase::Prompt(_) => b("🔑", i18n::ST_NEEDS_INPUT, AMBER),
            Phase::Failed(_) => b("✖", i18n::ST_ERROR, RED),
            Phase::Live => match &self.sess {
                None => b("○", i18n::ST_IDLE, GRAY),
                Some(s) if s.host_key_alert().is_some() => b("⛔", i18n::ST_BLOCKED, RED),
                Some(s) => match s.closed() {
                    Some(Some(_)) => b("✖", i18n::ST_ERROR, RED),
                    Some(None) => b("○", i18n::ST_DISCONNECTED, GRAY),
                    None => match s.state() {
                        SessionState::Connecting | SessionState::Reconnecting => {
                            b("⏳", i18n::ST_CONNECTING, AMBER)
                        }
                        SessionState::AwaitingHostTrust => b("🔒", i18n::ST_AWAITING_TRUST, AMBER),
                        SessionState::AwaitingAuth => b("🔑", i18n::ST_AUTHENTICATING, AMBER),
                        SessionState::Connected => b("✔", i18n::ST_CONNECTED, GREEN),
                        SessionState::Disconnected => b("○", i18n::ST_DISCONNECTED, GRAY),
                        SessionState::Error(_) => b("✖", i18n::ST_ERROR, RED),
                    },
                },
            },
        }
    }

    fn pending_modal(&self) -> Option<ModalKind> {
        if self.paste.is_some() {
            return Some(ModalKind::Paste);
        }
        match &self.phase {
            Phase::Prompt(_) => Some(ModalKind::Secret),
            Phase::VaultLocked { .. } => Some(ModalKind::Vault),
            Phase::Live
                if self
                    .sess
                    .as_ref()
                    .is_some_and(|s| s.host_key_prompt().is_some()) =>
            {
                Some(ModalKind::HostKey)
            }
            _ => None,
        }
    }

    /// Texto de erro amigável; marca a próxima tentativa para perguntar o segredo se for o caso.
    fn friendly_error(&self, msg: &str) -> String {
        friendly_error(&self.profile.auth, msg)
    }

    // ------------------------------------------------------------ UI

    fn toolbar(&mut self, ui: &mut egui::Ui, act: &mut Option<Act>) {
        let badge = self.badge();
        let live = self.session_live();
        let connecting_pre = matches!(
            self.phase,
            Phase::Resolving(_) | Phase::Prompt(_) | Phase::VaultLocked { .. }
        );
        let can_reconnect = !live && !matches!(self.phase, Phase::Resolving(_));
        let label = if matches!(self.phase, Phase::Idle) && self.sess.is_none() {
            i18n::TAB_CONNECT
        } else if self.force_prompt {
            i18n::TAB_RETRY
        } else {
            i18n::TAB_RECONNECT
        };
        let p = &self.profile;
        ui.horizontal(|ui| {
            let mut tip = format!("{}: {}", i18n::AUTH_LABEL, auth_text(&p.auth));
            if !self.saved {
                tip.push_str(&format!("\n{}", i18n::QUICK_NOT_SAVED));
            }
            ui.label(RichText::new(format!("{}@{}:{}", p.username, p.host, p.port)).monospace())
                .on_hover_text(tip);
            if let Some(j) = &self.jump_name {
                ui.weak(format!("{}: {j}", i18n::JUMP_LABEL));
            }
            ui.colored_label(badge.color, format!("{} {}", badge.icon, badge.text));
            if let Some(n) = &self.notice {
                ui.weak(n);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(can_reconnect, egui::Button::new(label))
                    .clicked()
                {
                    *act = Some(Act::Connect);
                }
                if ui
                    .add_enabled(
                        live || connecting_pre,
                        egui::Button::new(i18n::TAB_DISCONNECT),
                    )
                    .clicked()
                {
                    *act = Some(Act::Disconnect);
                }
            });
        });
        ui.separator();
    }

    fn message_panel(&self, ui: &mut egui::Ui, msg: &str, is_error: bool) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            let text = RichText::new(msg);
            if is_error {
                ui.label(text.color(error_text(ui)));
            } else {
                ui.label(text);
            }
        });
    }

    fn live_ui(&mut self, ui: &mut egui::Ui, env: &TabEnv, act: &mut Option<Act>) {
        let Some(sess) = self.sess.as_mut() else {
            return;
        };
        if let Some(alert) = sess.host_key_alert().cloned() {
            alert_panel(ui, &alert, act);
            return;
        }
        if let Some(Some(msg)) = sess.closed() {
            let text = friendly_error(&self.profile.auth, msg);
            egui::Frame::new()
                .fill(Color32::from_rgb(120, 25, 25))
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!("⚠ {text}"))
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
            ui.add_space(2.0);
        } else if sess.closed().is_some() {
            ui.weak(i18n::TAB_SESSION_ENDED);
        }
        sess.set_input_enabled(!env.input_blocked);
        sess.set_confirm_multiline(env.confirm_paste);
        // Paleta do perfil (se houver); senão a do tema global.
        let theme = env.theme.clone().unwrap_or_else(|| {
            if ui.visuals().dark_mode {
                TermTheme::dark()
            } else {
                TermTheme::light()
            }
        });
        let style = TermStyle::new(FontId::monospace(env.font_size), theme);
        let r = myssh_term::show(ui, sess, &style);
        if let Some(text) = r.paste_needs_confirm {
            self.paste = Some(text);
        }
    }

    // ------------------------------------------------------------ modais

    fn secret_modal(&mut self, ctx: &egui::Context, env: &TabEnv, fresh: bool) {
        let (user, host) = (self.profile.username.clone(), self.profile.host.clone());
        let key_file = match &self.profile.auth {
            AuthMethod::KeyFile { path, .. } => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let Phase::Prompt(p) = &mut self.phase else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let resp = egui::Modal::new(Id::new(("tab-modal", self.uid))).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(match p.kind {
                SecretKind::Password => i18n::ask_password_title(&user, &host),
                SecretKind::Passphrase => i18n::ask_passphrase_title(&key_file),
            });
            ui.add_space(6.0);
            if let Some(n) = &p.note {
                ui.colored_label(warn_color(ui), n);
            }
            let hint = match p.kind {
                SecretKind::Password => i18n::ASK_PASSWORD_HINT,
                SecretKind::Passphrase => i18n::ASK_PASSPHRASE_HINT,
            };
            let te = ui.add(
                egui::TextEdit::singleline(&mut p.text)
                    .password(true)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if fresh {
                te.request_focus();
            }
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if p.can_save {
                ui.add_space(4.0);
                ui.checkbox(&mut p.save, i18n::ASK_SAVE);
                ui.weak(i18n::ASK_SAVE_HELP);
            }
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = ui
                    .add_enabled(!p.text.is_empty(), egui::Button::new(i18n::CONNECT))
                    .clicked();
                if ok || (enter && !p.text.is_empty()) {
                    submit = true;
                }
                if ui.button(i18n::CANCEL).clicked() {
                    cancel = true;
                }
            });
        });
        if resp.should_close() {
            cancel = true;
        }
        if cancel {
            self.phase = Phase::Idle; // descarta (e zeroiza) o buffer
            self.notice = Some(i18n::TAB_CANCELLED.into());
        } else if submit {
            self.submit_prompt(env);
        }
    }

    fn vault_modal(&mut self, ctx: &egui::Context, env: &TabEnv) {
        let Phase::VaultLocked { error, unlocking } = &self.phase else {
            return;
        };
        let (err, busy) = (error.clone(), unlocking.is_some());
        let mut unlock = false;
        let mut type_now = false;
        let mut cancel = false;
        let resp = egui::Modal::new(Id::new(("tab-modal", self.uid))).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(i18n::VAULT_LOCKED_TITLE);
            ui.add_space(6.0);
            ui.label(i18n::VAULT_LOCKED_BODY);
            if let Some(e) = &err {
                ui.colored_label(error_text(ui), e);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if busy {
                    i18n::VAULT_UNLOCKING
                } else {
                    i18n::UNLOCK_VAULT
                };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    unlock = true;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(i18n::TYPE_NOW))
                    .clicked()
                {
                    type_now = true;
                }
                if ui.button(i18n::CANCEL).clicked() {
                    cancel = true;
                }
            });
        });
        if resp.should_close() {
            cancel = true;
        }
        if cancel {
            self.disconnect();
        } else if unlock {
            self.unlock(env);
        } else if type_now {
            let kind = match self.profile.auth {
                AuthMethod::KeyFile { .. } => SecretKind::Passphrase,
                _ => SecretKind::Password,
            };
            self.phase = Phase::Prompt(SecretPrompt {
                kind,
                text: String::new(),
                save: false,
                can_save: false,
                note: None,
            });
        }
    }

    fn host_key_modal(&mut self, ctx: &egui::Context, fresh: bool) {
        let Some(sess) = self.sess.as_mut() else {
            return;
        };
        let Some(p) = sess.host_key_prompt().cloned() else {
            return;
        };
        let mut decision = None;
        let resp = egui::Modal::new(Id::new(("tab-modal", self.uid))).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.heading(i18n::HK_TITLE);
            ui.add_space(6.0);
            ui.label(i18n::HK_BODY);
            ui.add_space(8.0);
            egui::Grid::new(("hk-grid", self.uid))
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(i18n::HK_HOST);
                    ui.add(
                        egui::Label::new(RichText::new(fmt_endpoint(&p.endpoint)).monospace())
                            .selectable(true),
                    );
                    ui.end_row();
                    ui.label(i18n::HK_ALGO);
                    ui.add(
                        egui::Label::new(RichText::new(&p.key.algorithm).monospace())
                            .selectable(true),
                    );
                    ui.end_row();
                    ui.label(i18n::HK_FINGERPRINT);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&p.key.fingerprint_sha256)
                                .monospace()
                                .strong(),
                        )
                        .selectable(true),
                    );
                    ui.end_row();
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Rejeitar é o padrão (foco inicial e Enter).
                let reject = ui.button(i18n::HK_REJECT);
                if fresh {
                    reject.request_focus();
                }
                if reject.clicked() {
                    decision = Some(HostKeyDecision::Reject);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(i18n::HK_ACCEPT_SAVE).clicked() {
                        decision = Some(HostKeyDecision::AcceptAndSave);
                    }
                    if ui.button(i18n::HK_ACCEPT_ONCE).clicked() {
                        decision = Some(HostKeyDecision::AcceptOnce);
                    }
                });
            });
        });
        if resp.should_close() {
            decision = Some(HostKeyDecision::Reject);
        }
        if let Some(d) = decision {
            sess.answer_host_key(d);
        }
    }

    fn paste_modal(&mut self, ctx: &egui::Context, fresh: bool) {
        let Some(text) = self.paste.clone() else {
            return;
        };
        let (preview, lines) = paste_preview(&text);
        let mut confirm = false;
        let mut cancel = false;
        let resp = egui::Modal::new(Id::new(("tab-modal", self.uid))).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(i18n::PASTE_TITLE);
            ui.add_space(6.0);
            ui.label(i18n::PASTE_BODY);
            ui.strong(i18n::paste_lines(lines));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(&preview).monospace()).selectable(true));
            });
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(i18n::PASTE_CONFIRM).clicked() {
                    confirm = true;
                }
                // Cancelar é o padrão (foco inicial e Enter).
                let c = ui.button(i18n::CANCEL);
                if fresh {
                    c.request_focus();
                }
                if c.clicked() {
                    cancel = true;
                }
            });
        });
        if resp.should_close() {
            cancel = true;
        }
        if confirm {
            if let (Some(mut t), Some(s)) = (self.paste.take(), self.sess.as_mut()) {
                s.paste_confirmed(&t);
                t.zeroize();
            }
        } else if cancel && let Some(mut t) = self.paste.take() {
            t.zeroize();
        }
    }
}

impl TabBody for TerminalTab {
    fn title(&self) -> String {
        self.profile.name.clone()
    }

    fn status(&self) -> Option<StatusBadge> {
        Some(self.badge())
    }

    fn pump(&mut self, env: &TabEnv) {
        if !self.started && env.auto_connect && matches!(self.phase, Phase::Idle) {
            self.started = true;
            self.connect(env);
        }

        // Credenciais prontas?
        let resolved = match &self.phase {
            Phase::Resolving(rx) => match rx.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Resolved::Failed(i18n::ERR_INTERNAL.into()))
                }
            },
            _ => None,
        };
        if let Some(r) = resolved {
            self.on_resolved(env, r);
        }

        // Desbloqueio do cofre concluído?
        let unlocked = match &self.phase {
            Phase::VaultLocked {
                unlocking: Some(rx),
                ..
            } => match rx.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(SecretError::Locked)),
            },
            _ => None,
        };
        match unlocked {
            Some(Ok(())) => self.start_resolve(env, false),
            Some(Err(e)) => {
                self.phase = Phase::VaultLocked {
                    error: Some(secret_error_text(&e)),
                    unlocking: None,
                }
            }
            None => {}
        }

        // Salvamento do segredo no cofre concluído?
        if let Some(rx) = &self.save_rx {
            match rx.try_recv() {
                Ok(Ok(auth)) => {
                    self.profile.auth = auth;
                    self.notice = Some(i18n::SAVED_TO_VAULT.into());
                    self.events.push(TabEvent::ProfileChanged);
                    self.save_rx = None;
                }
                Ok(Err(why)) => {
                    self.notice = Some(i18n::save_failed(&why));
                    self.save_rx = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.save_rx = None,
            }
        }

        // Drena a sessão (também em segundo plano: evita backpressure).
        if let Some(s) = self.sess.as_mut() {
            if s.pump(DEFAULT_PUMP_BUDGET) {
                env.ctx.request_repaint();
            }
            if !self.error_seen
                && let Some(Some(msg)) = s.closed()
            {
                self.error_seen = true;
                let ask_again = match &self.profile.auth {
                    AuthMethod::Password { .. } => msg == AUTH_REJECTED,
                    AuthMethod::KeyFile { .. } => msg == KEY_DECODE,
                    _ => false,
                };
                if ask_again {
                    self.force_prompt = true;
                }
            }
        }

        if self.pending_modal().is_none() {
            self.modal_kind = None;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, env: &TabEnv) {
        let mut act = None;
        self.toolbar(ui, &mut act);
        match &self.phase {
            Phase::Idle => {
                let msg = if self.sess.is_some() {
                    i18n::TAB_SESSION_ENDED
                } else {
                    i18n::TAB_NOT_CONNECTED
                };
                self.message_panel(ui, msg, false);
            }
            Phase::Resolving(_) => self.message_panel(ui, i18n::TAB_PREPARING, false),
            Phase::VaultLocked { .. } | Phase::Prompt(_) => {
                self.message_panel(ui, i18n::TAB_WAITING_INPUT, false)
            }
            Phase::Failed(m) => {
                let m = self.friendly_error(m);
                self.message_panel(ui, &m, true);
            }
            Phase::Live => self.live_ui(ui, env, &mut act),
        }
        match act {
            Some(Act::Connect) => self.connect(env),
            Some(Act::Disconnect) => self.disconnect(),
            Some(Act::OpenTrusted) => self.events.push(TabEvent::OpenTrustedHosts),
            None => {}
        }
    }

    fn needs_modal(&self) -> bool {
        self.pending_modal().is_some()
    }

    fn modal_ui(&mut self, ctx: &egui::Context, env: &TabEnv) {
        let Some(kind) = self.pending_modal() else {
            return;
        };
        let fresh = self.modal_kind != Some(kind);
        self.modal_kind = Some(kind);
        match kind {
            ModalKind::Paste => self.paste_modal(ctx, fresh),
            ModalKind::Secret => self.secret_modal(ctx, env, fresh),
            ModalKind::Vault => self.vault_modal(ctx, env),
            ModalKind::HostKey => self.host_key_modal(ctx, fresh),
        }
    }

    #[cfg(test)]
    fn debug_screen(&self) -> String {
        let Some(s) = self.sess.as_ref() else {
            return String::new();
        };
        match s.closed() {
            Some(Some(e)) => format!("{}\n[erro: {e}]", s.screen_text()),
            _ => s.screen_text(),
        }
    }

    #[cfg(test)]
    fn debug_bytes(&self) -> u64 {
        self.sess.as_ref().map_or(0, |s| s.bytes_received())
    }

    #[cfg(test)]
    fn debug_send(&self, bytes: &[u8]) {
        if let Some(s) = &self.sess {
            s.send_input(bytes.to_vec());
        }
    }

    fn drain(&mut self) {
        if let Some(s) = self.sess.as_mut() {
            s.pump(DEFAULT_PUMP_BUDGET);
        }
    }

    fn take_events(&mut self) -> Vec<TabEvent> {
        std::mem::take(&mut self.events)
    }

    fn fills_space(&self) -> bool {
        true
    }

    fn is_busy(&self) -> bool {
        self.simulate_busy || self.session_live()
    }

    fn close(&mut self) {
        if let Some(s) = &self.sess {
            s.disconnect();
        }
        if let Some(mut t) = self.paste.take() {
            t.zeroize();
        }
        if matches!(
            self.phase,
            Phase::Resolving(_) | Phase::Prompt(_) | Phase::VaultLocked { .. }
        ) {
            self.phase = Phase::Idle;
        }
        self.simulate_busy = false;
    }

    fn is_closed(&self) -> bool {
        !self.session_live()
    }

    fn has_terminal_focus(&self) -> bool {
        matches!(self.phase, Phase::Live) && self.sess.as_ref().is_some_and(|s| s.is_connected())
    }

    fn demo_accept_host_key(&mut self) {
        if let Some(s) = self.sess.as_mut()
            && s.host_key_prompt().is_some()
        {
            s.answer_host_key(HostKeyDecision::AcceptAndSave);
        }
    }

    fn simulate_busy(&mut self, busy: bool) {
        self.simulate_busy = busy;
    }
}

// ---------------------------------------------------------------- funções puras / blocking

fn error_text(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 120, 110)
    } else {
        Color32::from_rgb(180, 30, 30)
    }
}

/// `host:porta` (IPv6 entre colchetes).
pub(crate) fn fmt_endpoint(e: &HostEndpoint) -> String {
    if e.host.contains(':') {
        format!("[{}]:{}", e.host, e.port)
    } else {
        format!("{}:{}", e.host, e.port)
    }
}

fn auth_text(a: &AuthMethod) -> &'static str {
    match a {
        AuthMethod::Password {
            credential: Some(_),
        } => i18n::AUTH_SUMMARY_PASSWORD_SAVED,
        AuthMethod::Password { credential: None } => i18n::AUTH_SUMMARY_PASSWORD_ASK,
        AuthMethod::KeyFile { .. } => i18n::AUTH_SUMMARY_KEY,
        AuthMethod::Agent => i18n::AUTH_SUMMARY_AGENT,
        AuthMethod::KeyboardInteractive => i18n::AUTH_SUMMARY_KI,
    }
}

fn friendly_error(auth: &AuthMethod, msg: &str) -> String {
    if msg == AUTH_REJECTED {
        match auth {
            AuthMethod::Password { .. } => return i18n::ERR_AUTH_PASSWORD.into(),
            AuthMethod::KeyFile { .. } => return i18n::ERR_AUTH_KEY.into(),
            _ => {}
        }
    }
    msg.to_string()
}

/// Prévia para o diálogo de colagem: primeiras linhas, truncadas, com controles neutralizados.
/// Devolve (prévia, nº de linhas do texto completo).
pub(crate) fn paste_preview(text: &str) -> (String, usize) {
    let norm = text.replace("\r\n", "\n").replace('\r', "\n");
    let total = norm.lines().count().max(1);
    let mut out = String::new();
    for (i, line) in norm.lines().take(PREVIEW_MAX_LINES).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for (n, c) in line.chars().enumerate() {
            if n >= PREVIEW_MAX_COLS {
                out.push('…');
                break;
            }
            out.push(if c.is_control() { '·' } else { c });
        }
    }
    if total > PREVIEW_MAX_LINES {
        out.push_str(&format!("\n… (+{} linhas)", total - PREVIEW_MAX_LINES));
    }
    (out, total)
}

enum Fetch {
    Secret(SecretString),
    Locked,
    Missing(String),
}

fn fetch(store: &Store, vault: &Vault, cid: CredentialId) -> Fetch {
    let cred = match store.get_credential(cid) {
        Ok(Some(c)) => c,
        _ => return Fetch::Missing(i18n::NOTE_CRED_MISSING.into()),
    };
    match vault.get(&cred.secret) {
        Ok(s) => Fetch::Secret(s),
        Err(SecretError::Locked) => Fetch::Locked,
        Err(SecretError::NotFound) => Fetch::Missing(i18n::NOTE_SECRET_MISSING.into()),
        Err(SecretError::Unavailable(_)) => Fetch::Missing(i18n::NOTE_VAULT_UNAVAILABLE.into()),
        Err(e) => Fetch::Missing(secret_error_text(&e)),
    }
}

/// Prepara o material de autenticação. Roda em `spawn_blocking` (cofre do SO e disco).
fn resolve(store: &Store, vault: &Vault, auth: &AuthMethod, force: bool) -> Resolved {
    match auth {
        AuthMethod::Password {
            credential: Some(cid),
        } if !force => match fetch(store, vault, *cid) {
            Fetch::Secret(s) => Resolved::Ready(AuthMaterial::Password(s)),
            Fetch::Locked => Resolved::VaultLocked,
            Fetch::Missing(note) => Resolved::Prompt {
                kind: SecretKind::Password,
                note: Some(note),
            },
        },
        AuthMethod::Password { .. } => Resolved::Prompt {
            kind: SecretKind::Password,
            note: None,
        },
        AuthMethod::KeyFile { path, passphrase } => {
            if let (Some(cid), false) = (passphrase, force) {
                return match fetch(store, vault, *cid) {
                    Fetch::Secret(s) => Resolved::Ready(AuthMaterial::KeyFile {
                        path: path.clone(),
                        passphrase: Some(s),
                    }),
                    Fetch::Locked => Resolved::VaultLocked,
                    Fetch::Missing(note) => Resolved::Prompt {
                        kind: SecretKind::Passphrase,
                        note: Some(note),
                    },
                };
            }
            match key_requires_passphrase(path) {
                Ok(false) => Resolved::Ready(AuthMaterial::KeyFile {
                    path: path.clone(),
                    passphrase: None,
                }),
                Ok(true) => Resolved::Prompt {
                    kind: SecretKind::Passphrase,
                    note: None,
                },
                Err(m) => Resolved::Failed(m),
            }
        }
        AuthMethod::Agent => Resolved::Ready(AuthMaterial::Agent),
        AuthMethod::KeyboardInteractive => Resolved::Failed(i18n::ERR_KI.into()),
    }
}

/// Cria credencial + segredo no cofre e liga ao perfil. Devolve o novo `AuthMethod`.
fn save_secret(
    store: &Store,
    vault: &Vault,
    pid: myssh_core::ProfileId,
    name: &str,
    kind: SecretKind,
    secret: SecretString,
) -> Result<AuthMethod, String> {
    let sref = new_secret_ref();
    vault
        .put(&sref, secret)
        .map_err(|e| secret_error_text(&e))?;
    let undo = |store: &Store, cid: Option<CredentialId>| {
        if let Some(c) = cid {
            let _ = store.delete_credential(c);
        }
        let _ = vault.delete(&sref);
    };
    let cred = Credential {
        id: CredentialId::new(),
        label: match kind {
            SecretKind::Password => i18n::cred_label_password(name),
            SecretKind::Passphrase => i18n::cred_label_passphrase(name),
        },
        kind: match kind {
            SecretKind::Password => CredentialKind::Password,
            SecretKind::Passphrase => CredentialKind::KeyPassphrase,
        },
        secret: sref.clone(),
    };
    if let Err(e) = store.create_credential(&cred) {
        undo(store, None);
        return Err(e.to_string());
    }
    let mut p = match store.get_profile(pid) {
        Ok(Some(p)) => p,
        _ => {
            undo(store, Some(cred.id));
            return Err(i18n::ERR_INTERNAL.into());
        }
    };
    match (&mut p.auth, kind) {
        (AuthMethod::Password { credential }, SecretKind::Password) => *credential = Some(cred.id),
        (AuthMethod::KeyFile { passphrase, .. }, SecretKind::Passphrase) => {
            *passphrase = Some(cred.id)
        }
        _ => {
            undo(store, Some(cred.id));
            return Err(i18n::ERR_INTERNAL.into());
        }
    }
    if let Err(e) = store.update_profile(&p) {
        undo(store, Some(cred.id));
        return Err(e.to_string());
    }
    Ok(p.auth)
}

/// Painel vermelho e bloqueante de chave de servidor alterada. Sem opção de aceitar.
fn alert_panel(ui: &mut egui::Ui, alert: &HostKeyAlert, act: &mut Option<Act>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(140, 20, 20))
                .stroke(egui::Stroke::new(2.0, Color32::from_rgb(255, 90, 80)))
                .corner_radius(4.0)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                    ui.label(
                        RichText::new(format!("⛔ {}", i18n::HKC_TITLE))
                            .heading()
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(i18n::HKC_BODY);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            i18n::HK_HOST,
                            fmt_endpoint(&alert.endpoint)
                        ))
                        .monospace(),
                    );
                    ui.add_space(6.0);
                    ui.strong(i18n::HKC_PRESENTED);
                    key_line(
                        ui,
                        &alert.presented.algorithm,
                        &alert.presented.fingerprint_sha256,
                    );
                    ui.add_space(6.0);
                    if alert.known.is_empty() {
                        ui.label(i18n::HKC_REVOKED);
                    } else {
                        ui.strong(i18n::HKC_KNOWN);
                        for k in &alert.known {
                            key_line(ui, &k.algorithm, &k.fingerprint_sha256);
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(i18n::HKC_HINT);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(i18n::HKC_OPEN_TRUSTED).clicked() {
                            *act = Some(Act::OpenTrusted);
                        }
                        if ui.button(i18n::TAB_RETRY).clicked() {
                            *act = Some(Act::Connect);
                        }
                    });
                });
        });
}

fn key_line(ui: &mut egui::Ui, algo: &str, fp: &str) {
    ui.add(egui::Label::new(RichText::new(format!("{algo}  {fp}")).monospace()).selectable(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previa_trunca_e_conta_linhas() {
        let (p, n) = paste_preview("a\nb\nc");
        assert_eq!((p.as_str(), n), ("a\nb\nc", 3));
        let longo: String = (0..20).map(|i| format!("linha {i}\n")).collect();
        let (p, n) = paste_preview(&longo);
        assert_eq!(n, 20);
        assert_eq!(p.lines().count(), PREVIEW_MAX_LINES + 1);
        assert!(p.ends_with("… (+12 linhas)"));
        let (p, _) = paste_preview(&"x".repeat(500));
        assert!(p.chars().count() <= PREVIEW_MAX_COLS + 1);
    }

    #[test]
    fn previa_neutraliza_controles_e_normaliza_cr() {
        let (p, n) = paste_preview("a\x1b[31m\rb\r\nc");
        assert_eq!(n, 3);
        assert!(!p.contains('\x1b'));
        assert!(p.contains("a·[31m"));
    }

    #[test]
    fn endpoint_ipv6_entre_colchetes() {
        assert_eq!(fmt_endpoint(&HostEndpoint::new("::1", 22)), "[::1]:22");
        assert_eq!(
            fmt_endpoint(&HostEndpoint::new("H.com", 2222)),
            "h.com:2222"
        );
    }

    #[test]
    fn erro_de_autenticacao_vira_mensagem_clara() {
        let pw = AuthMethod::Password { credential: None };
        assert_eq!(friendly_error(&pw, AUTH_REJECTED), i18n::ERR_AUTH_PASSWORD);
        assert_eq!(friendly_error(&pw, "outro"), "outro");
    }
}
