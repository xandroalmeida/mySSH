//! Abas e ponto de extensão para o terminal.
//!
//! O crate de terminal/SSH (a ser integrado) implementa [`TabBody`] e é instanciado em
//! [`make_body`] para [`TabKind::Terminal`]. Enquanto isso, [`PlaceholderTerminal`] ocupa o lugar.

use myssh_core::{AuthMethod, Environment, Profile};

use crate::i18n;

/// O que a aba abre. Conexão rápida = perfil transitório (`saved == false`).
#[derive(Debug, Clone)]
pub enum TabKind {
    Terminal { profile: Box<Profile>, saved: bool },
    Welcome,
}

impl TabKind {
    pub fn environment(&self) -> Environment {
        match self {
            Self::Terminal { profile, .. } => profile.environment,
            Self::Welcome => Environment::None,
        }
    }
}

/// Conteúdo de uma aba.
pub trait TabBody {
    fn title(&self) -> String;
    fn ui(&mut self, ui: &mut egui::Ui);
    /// Há sessão ativa: fechar exige confirmação.
    fn is_busy(&self) -> bool;
    /// Encerra a sessão/recursos. Chamado ao fechar a aba ou a aplicação.
    fn close(&mut self);
    /// O teclado está capturado pelo terminal (muda a regra de atalhos; ver `shortcuts`).
    fn has_terminal_focus(&self) -> bool {
        false
    }
    /// Somente protótipo/demo: força `is_busy` para exercitar a confirmação ao fechar.
    fn simulate_busy(&mut self, _busy: bool) {}
}

pub struct Tab {
    pub id: u64,
    pub kind: TabKind,
    pub body: Box<dyn TabBody>,
}

/// Cria o corpo da aba. Ponto único a trocar quando o terminal real existir.
pub fn make_body(kind: &TabKind, jump_name: Option<String>) -> Box<dyn TabBody> {
    match kind {
        TabKind::Terminal { profile, saved } => Box::new(PlaceholderTerminal {
            profile: (**profile).clone(),
            saved: *saved,
            jump_name,
            simulate_busy: false,
        }),
        TabKind::Welcome => Box::new(WelcomeBody),
    }
}

pub struct PlaceholderTerminal {
    profile: Profile,
    saved: bool,
    jump_name: Option<String>,
    simulate_busy: bool,
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

impl TabBody for PlaceholderTerminal {
    fn title(&self) -> String {
        self.profile.name.clone()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let p = &self.profile;
        ui.add_space(8.0);
        ui.heading(&p.name);
        ui.label(format!("{}@{}:{}", p.username, p.host, p.port));
        ui.label(format!("{}: {}", i18n::AUTH_LABEL, auth_text(&p.auth)));
        if let Some(j) = &self.jump_name {
            ui.label(format!("{}: {j}", i18n::JUMP_LABEL));
        }
        if !self.saved {
            ui.weak(i18n::QUICK_NOT_SAVED);
        }
        ui.add_space(12.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(ui.available_width(), 160.0));
            ui.centered_and_justified(|ui| ui.weak(i18n::TERMINAL_PLACEHOLDER));
        });
        ui.add_space(8.0);
        ui.checkbox(&mut self.simulate_busy, i18n::SIMULATE_BUSY);
    }

    fn is_busy(&self) -> bool {
        self.simulate_busy
    }

    fn close(&mut self) {
        self.simulate_busy = false;
    }

    fn simulate_busy(&mut self, busy: bool) {
        self.simulate_busy = busy;
    }
}

pub struct WelcomeBody;

impl TabBody for WelcomeBody {
    fn title(&self) -> String {
        i18n::WELCOME_TITLE.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.heading(i18n::WELCOME_HEADING);
            ui.add_space(8.0);
            ui.label(i18n::WELCOME_BODY);
            ui.add_space(8.0);
            for a in crate::shortcuts::ACTIONS {
                ui.weak(format!(
                    "{}  {}",
                    crate::shortcuts::label(a),
                    i18n::shortcut_name(a)
                ));
            }
        });
    }

    fn is_busy(&self) -> bool {
        false
    }

    fn close(&mut self) {}
}
