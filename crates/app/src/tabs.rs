//! Abas. [`TerminalTab`] (módulo `terminal_tab`) implementa [`TabBody`] para [`TabKind::Terminal`].

use std::sync::Arc;

use egui::Color32;
use myssh_core::{Environment, Profile};
use myssh_secrets::Vault;
use myssh_store::Store;

use crate::files::contract::Overwrite;
use crate::i18n;
use crate::terminal_tab::TerminalTab;

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

/// Serviços e configurações que as abas recebem a cada frame (nada é copiado para dentro delas).
#[derive(Clone)]
pub struct TabEnv<'a> {
    pub rt: &'a tokio::runtime::Handle,
    pub store: &'a Arc<Store>,
    pub vault: &'a Arc<Vault>,
    pub ctx: &'a egui::Context,
    pub font_size: f32,
    pub confirm_paste: bool,
    /// Um diálogo modal (do app ou de outra aba) está aberto: o terminal não recebe teclado.
    pub input_blocked: bool,
    /// Conecta sozinha ao abrir (desligado em demos e testes sem rede).
    pub auto_connect: bool,
    /// Tema global escuro (o tema do perfil é resolvido sobre ele).
    pub dark: bool,
    /// Faixa/borda de aviso em abas de produção.
    pub prod_warning: bool,
    /// Paleta do terminal escolhida pelo perfil; `None` = a global (claro/escuro).
    /// Preenchido por `with_profile_theme` para cada aba/painel.
    pub theme: Option<myssh_term::TermTheme>,
    /// Política padrão de sobrescrita das transferências (Configurações).
    pub default_overwrite: Overwrite,
    /// Diretório de dados do app (journal da sincronização); `None` em testes.
    pub data_dir: Option<std::path::PathBuf>,
}

impl TabEnv<'_> {
    /// Cópia do ambiente com o tema (paleta e fonte) do perfil aplicado.
    pub fn with_profile_theme(&self, kind: &TabKind) -> Self {
        let mut e = self.clone();
        if let TabKind::Terminal { profile, .. } = kind {
            let r = crate::theming::resolve(&profile.theme, self.dark, self.font_size);
            e.font_size = r.font_size;
            e.theme = r.palette_overridden.then_some(r.term);
        }
        e
    }
}

/// Pedidos da aba para o app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabEvent {
    /// O perfil mudou no store (ex.: senha salva no cofre): recarregar a lista.
    ProfileChanged,
    /// Abrir a lista de hosts confiáveis (Configurações).
    OpenTrustedHosts,
    /// O usuário pediu para fechar um painel dividido (pelo botão do painel).
    ClosePane(crate::split::PaneId),
}

/// Indicador de estado na barra de abas: ícone + texto (nunca só cor).
#[derive(Debug, Clone, Copy)]
pub struct StatusBadge {
    pub icon: &'static str,
    pub text: &'static str,
    pub color: Color32,
}

/// Conteúdo de uma aba.
pub trait TabBody {
    fn title(&self) -> String;
    fn ui(&mut self, ui: &mut egui::Ui, env: &TabEnv);
    /// Chamado em todo frame para todas as abas (inclusive em segundo plano): drena a saída das
    /// sessões (evita backpressure) e avança a conexão.
    fn pump(&mut self, _env: &TabEnv) {}
    /// A aba tem um diálogo modal próprio a mostrar (senha, host key, colagem...).
    fn needs_modal(&self) -> bool {
        false
    }
    fn modal_ui(&mut self, _ctx: &egui::Context, _env: &TabEnv) {}
    /// Drena a saída pendente sem precisar de UI (usado ao encerrar o app).
    fn drain(&mut self) {}
    fn status(&self) -> Option<StatusBadge> {
        None
    }
    fn take_events(&mut self) -> Vec<TabEvent> {
        Vec::new()
    }
    /// A aba gerencia o próprio tamanho (terminal): o app não a envolve em `ScrollArea`.
    fn fills_space(&self) -> bool {
        false
    }
    /// Transferências de arquivos em andamento (fechar/sair pede confirmação mencionando o número).
    fn active_transfers(&self) -> usize {
        0
    }
    /// Sincronizações em execução (fechar/sair pede confirmação).
    fn active_syncs(&self) -> usize {
        0
    }
    /// Somente testes: o painel de arquivos da aba (ou do painel focado).
    #[cfg(test)]
    fn files_panel(&self) -> Option<&crate::files::FilesPanel> {
        None
    }
    #[cfg(test)]
    fn files_panel_mut(&mut self) -> Option<&mut crate::files::FilesPanel> {
        None
    }
    /// Mostra/oculta o painel de arquivos (atalho e botão).
    fn toggle_files(&mut self) {}
    /// Há sessão ativa: fechar exige confirmação.
    fn is_busy(&self) -> bool;
    /// Pede o encerramento da sessão (não bloqueia). Chamado ao fechar a aba ou a aplicação.
    fn close(&mut self);
    /// `true` quando nada mais está vivo (usado para esperar o desligamento ao sair).
    fn is_closed(&self) -> bool {
        true
    }
    /// O teclado está capturado pelo terminal (muda a regra de atalhos; ver `shortcuts`).
    fn has_terminal_focus(&self) -> bool {
        false
    }
    /// Aba dividida em painéis (`None` nas demais).
    fn as_split(&mut self) -> Option<&mut crate::split_body::SplitBody> {
        None
    }
    fn as_split_ref(&self) -> Option<&crate::split_body::SplitBody> {
        None
    }
    /// A aba desenha o próprio aviso de ambiente e as bordas (painéis divididos).
    fn manages_chrome(&self) -> bool {
        false
    }
    /// Somente testes: texto da tela do terminal.
    #[cfg(test)]
    fn debug_screen(&self) -> String {
        String::new()
    }
    /// Somente testes: bytes de saída remota já processados.
    #[cfg(test)]
    fn debug_bytes(&self) -> u64 {
        0
    }
    /// Somente testes: envia bytes à sessão (como se digitados).
    #[cfg(test)]
    fn debug_send(&self, _bytes: &[u8]) {}
    /// Somente demo (`MYSSH_DEMO_SSH`): responde "aceitar e salvar" ao diálogo de host key.
    #[cfg(any(test, debug_assertions, feature = "demo"))]
    fn demo_accept_host_key(&mut self) {}
    /// Somente protótipo/demo: força `is_busy` para exercitar a confirmação ao fechar.
    #[cfg(any(test, debug_assertions, feature = "demo"))]
    fn simulate_busy(&mut self, _busy: bool) {}
}

pub struct Tab {
    pub id: u64,
    pub kind: TabKind,
    pub body: Box<dyn TabBody>,
}

/// Cria o corpo da aba.
pub fn make_body(kind: &TabKind, jump_name: Option<String>) -> Box<dyn TabBody> {
    match kind {
        TabKind::Terminal { profile, saved } => {
            Box::new(TerminalTab::new((**profile).clone(), *saved, jump_name))
        }
        TabKind::Welcome => Box::new(WelcomeBody),
    }
}

pub struct WelcomeBody;

impl TabBody for WelcomeBody {
    fn title(&self) -> String {
        i18n::WELCOME_TITLE.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _env: &TabEnv) {
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
