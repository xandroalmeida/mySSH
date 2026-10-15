//! Textos do seletor de idioma.

use myssh_i18n::tr;

pub fn S_LANGUAGE() -> &'static str {
    tr!("Idioma", "Language", "Idioma")
}
pub fn LANG_AUTO() -> &'static str {
    tr!(
        "Automático (sistema)",
        "Automatic (system)",
        "Automático (sistema)"
    )
}
pub fn LANG_HELP() -> &'static str {
    tr!(
        "“Automático” usa o idioma do sistema; se não for suportado, inglês.",
        "“Automatic” follows the system language, falling back to English.",
        "“Automático” usa el idioma del sistema; si no es compatible, inglés."
    )
}
