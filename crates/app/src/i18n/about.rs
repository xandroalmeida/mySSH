//! Textos da janela "Sobre".

use myssh_i18n::tr;

pub fn ABOUT_BUTTON() -> &'static str {
    tr!("Sobre", "About", "Acerca de")
}
pub fn ABOUT_BUTTON_HELP() -> &'static str {
    tr!(
        "Versão, créditos e licenças do mySSH",
        "mySSH version, credits and licenses",
        "Versión, créditos y licencias de mySSH"
    )
}
pub fn ABOUT_TITLE() -> &'static str {
    tr!("Sobre o mySSH", "About mySSH", "Acerca de mySSH")
}
pub fn ABOUT_TAGLINE() -> &'static str {
    tr!(
        "Sessões SSH, arquivos e sincronização — local, seguro, em Rust.",
        "SSH sessions, files and sync — local, secure, in Rust.",
        "Sesiones SSH, archivos y sincronización — local, seguro, en Rust."
    )
}
pub fn ABOUT_VERSION() -> &'static str {
    tr!("Versão", "Version", "Versión")
}
pub fn ABOUT_COMMIT() -> &'static str {
    "Commit"
}
pub fn ABOUT_BUILD() -> &'static str {
    "Build"
}
pub fn ABOUT_PLATFORM() -> &'static str {
    tr!("Plataforma", "Platform", "Plataforma")
}
pub fn ABOUT_DATA() -> &'static str {
    tr!("Dados", "Data", "Datos")
}
pub fn ABOUT_UNKNOWN() -> &'static str {
    tr!("desconhecido", "unknown", "desconocido")
}
pub fn ABOUT_DIRTY() -> &'static str {
    tr!("+modificado", "+modified", "+modificado")
}
pub fn ABOUT_MADE_WITH() -> &'static str {
    tr!("Feito com", "Made with", "Hecho con")
}
pub fn ABOUT_LICENSE_LABEL() -> &'static str {
    tr!("Licença", "License", "Licencia")
}
pub fn ABOUT_LICENSE() -> &'static str {
    "MIT OR Apache-2.0"
}
pub fn ABOUT_THIRD_PARTY() -> &'static str {
    tr!(
        "Componentes de terceiros",
        "Third-party components",
        "Componentes de terceros"
    )
}
pub fn ABOUT_COPY() -> &'static str {
    tr!(
        "Copiar informações",
        "Copy information",
        "Copiar información"
    )
}
pub fn ABOUT_COPY_HELP() -> &'static str {
    tr!(
        "Copia versão, commit e plataforma para relatar problemas",
        "Copies version, commit and platform to report issues",
        "Copia versión, commit y plataforma para reportar problemas"
    )
}
pub fn ABOUT_COPIED() -> &'static str {
    tr!("Copiado ✓", "Copied ✓", "Copiado ✓")
}
pub fn ABOUT_OPEN_DATA() -> &'static str {
    tr!(
        "Abrir pasta de dados",
        "Open data folder",
        "Abrir carpeta de datos"
    )
}
pub fn ABOUT_OPEN_DATA_HELP() -> &'static str {
    tr!(
        "Abre a pasta no gerenciador de arquivos do sistema",
        "Opens the folder in the system file manager",
        "Abre la carpeta en el gestor de archivos del sistema"
    )
}
pub fn ABOUT_LINK_HELP() -> &'static str {
    tr!(
        "Abrir no navegador",
        "Open in browser",
        "Abrir en el navegador"
    )
}

pub fn about_third_party(n: usize) -> String {
    format!("{} ({n})", ABOUT_THIRD_PARTY())
}
