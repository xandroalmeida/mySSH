//! Configurações globais, persistidas na tabela `settings` do store.

use myssh_store::Store;

use crate::files::contract::Overwrite;
use crate::files::plan::{overwrite_key, parse_overwrite};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

impl ThemePref {
    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
    pub fn to_egui(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemePref,
    /// Tamanho da fonte do terminal, em pontos.
    pub terminal_font_size: f32,
    pub confirm_multiline_paste: bool,
    /// Minutos de inatividade para bloquear o cofre; 0 = nunca.
    pub vault_lock_minutes: u32,
    /// Faixa/borda de aviso em abas de produção.
    pub prod_warning: bool,
    /// Bloquear o cofre ao detectar suspensão do computador (ver `suspend`).
    pub vault_lock_on_suspend: bool,
    /// Política padrão de sobrescrita nas transferências de arquivos.
    pub files_overwrite: Overwrite,
}

pub const FONT_MIN: f32 = 8.0;
pub const FONT_MAX: f32 = 32.0;
pub const LOCK_MAX_MINUTES: u32 = 24 * 60;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePref::System,
            terminal_font_size: 14.0,
            confirm_multiline_paste: true,
            vault_lock_minutes: 15,
            prod_warning: true,
            vault_lock_on_suspend: true,
            files_overwrite: Overwrite::Ask,
        }
    }
}

const K_THEME: &str = "ui.theme";
const K_FONT: &str = "terminal.font_size";
const K_PASTE: &str = "terminal.confirm_multiline_paste";
const K_LOCK: &str = "vault.lock_minutes";
const K_PROD: &str = "ui.prod_warning";
const K_SUSPEND: &str = "vault.lock_on_suspend";
const K_FILES_OVERWRITE: &str = "files.overwrite";

impl Settings {
    /// Lê valores via `get`; chaves ausentes ou inválidas caem no padrão.
    pub fn from_getter(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        Self {
            theme: get(K_THEME)
                .and_then(|s| ThemePref::parse(&s))
                .unwrap_or(d.theme),
            terminal_font_size: get(K_FONT)
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(d.terminal_font_size, |v| v.clamp(FONT_MIN, FONT_MAX)),
            confirm_multiline_paste: get(K_PASTE)
                .and_then(|s| s.parse().ok())
                .unwrap_or(d.confirm_multiline_paste),
            vault_lock_minutes: get(K_LOCK)
                .and_then(|s| s.parse::<u32>().ok())
                .map_or(d.vault_lock_minutes, |v| v.min(LOCK_MAX_MINUTES)),
            prod_warning: get(K_PROD)
                .and_then(|s| s.parse().ok())
                .unwrap_or(d.prod_warning),
            vault_lock_on_suspend: get(K_SUSPEND)
                .and_then(|s| s.parse().ok())
                .unwrap_or(d.vault_lock_on_suspend),
            files_overwrite: get(K_FILES_OVERWRITE)
                .and_then(|s| parse_overwrite(&s))
                .unwrap_or(d.files_overwrite),
        }
    }

    pub fn load(store: &Store) -> Self {
        Self::from_getter(|k| store.get_setting(k).ok().flatten())
    }

    pub fn pairs(&self) -> [(&'static str, String); 7] {
        [
            (K_THEME, self.theme.key().to_string()),
            (K_FONT, format!("{}", self.terminal_font_size)),
            (K_PASTE, self.confirm_multiline_paste.to_string()),
            (K_LOCK, self.vault_lock_minutes.to_string()),
            (K_PROD, self.prod_warning.to_string()),
            (K_SUSPEND, self.vault_lock_on_suspend.to_string()),
            (
                K_FILES_OVERWRITE,
                overwrite_key(self.files_overwrite).to_string(),
            ),
        ]
    }

    pub fn save(&self, store: &Store) -> myssh_store::Result<()> {
        for (k, v) in self.pairs() {
            store.set_setting(k, &v)?;
        }
        Ok(())
    }

    pub fn vault_timeout(&self) -> Option<std::time::Duration> {
        (self.vault_lock_minutes > 0)
            .then(|| std::time::Duration::from_secs(u64::from(self.vault_lock_minutes) * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padroes_quando_vazio() {
        assert_eq!(Settings::from_getter(|_| None), Settings::default());
    }

    #[test]
    fn valores_invalidos_caem_no_padrao_e_sao_limitados() {
        let s = Settings::from_getter(|k| {
            Some(
                match k {
                    K_THEME => "roxo",
                    K_FONT => "999",
                    K_PASTE => "talvez",
                    K_LOCK => "9999999",
                    _ => "false",
                }
                .to_string(),
            )
        });
        assert_eq!(s.theme, ThemePref::System);
        assert_eq!(s.terminal_font_size, FONT_MAX);
        assert!(s.confirm_multiline_paste);
        assert_eq!(s.vault_lock_minutes, LOCK_MAX_MINUTES);
        assert!(!s.prod_warning);
        assert!(!s.vault_lock_on_suspend);
        // valor inválido da política cai no padrão
        assert_eq!(s.files_overwrite, Overwrite::Ask);
    }

    #[test]
    fn tema_sistema_segue_o_so() {
        assert_eq!(ThemePref::System.to_egui(), egui::ThemePreference::System);
        assert_eq!(ThemePref::Light.to_egui(), egui::ThemePreference::Light);
        assert_eq!(ThemePref::Dark.to_egui(), egui::ThemePreference::Dark);
        assert_eq!(Settings::default().theme, ThemePref::System);
    }

    #[test]
    fn persiste_e_recarrega() {
        let store = Store::open_in_memory().unwrap();
        let s = Settings {
            theme: ThemePref::Dark,
            terminal_font_size: 17.5,
            confirm_multiline_paste: false,
            vault_lock_minutes: 0,
            prod_warning: false,
            vault_lock_on_suspend: false,
            files_overwrite: Overwrite::Skip,
        };
        s.save(&store).unwrap();
        assert_eq!(Settings::load(&store), s);
        assert_eq!(s.vault_timeout(), None);
    }
}
