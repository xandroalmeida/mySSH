//! Indicação visual de ambiente: sempre cor + texto + ícone (nunca só cor).

use egui::Color32;
use myssh_core::Environment;

#[derive(Debug, Clone, Copy)]
pub struct EnvStyle {
    pub label: &'static str,
    pub icon: &'static str,
    pub fill: Color32,
    pub text: Color32,
}

pub fn style(env: Environment) -> Option<EnvStyle> {
    match env {
        Environment::None => None,
        Environment::Production => Some(EnvStyle {
            label: crate::i18n::ENV_PROD,
            icon: "⚠",
            fill: Color32::from_rgb(190, 30, 30),
            text: Color32::WHITE,
        }),
        Environment::Staging => Some(EnvStyle {
            label: crate::i18n::ENV_STAGING,
            icon: "🔶",
            fill: Color32::from_rgb(230, 180, 20),
            text: Color32::BLACK,
        }),
        Environment::Development => Some(EnvStyle {
            label: crate::i18n::ENV_DEV,
            icon: "🔧",
            fill: Color32::from_rgb(40, 150, 70),
            text: Color32::WHITE,
        }),
    }
}

/// Nome do ambiente para listas/filtros.
pub fn name(env: Environment) -> &'static str {
    match env {
        Environment::None => crate::i18n::ENV_NONE,
        Environment::Production => crate::i18n::ENV_PROD,
        Environment::Staging => crate::i18n::ENV_STAGING,
        Environment::Development => crate::i18n::ENV_DEV,
    }
}
