//! Textos da janela "Sobre".

pub fn ABOUT_BUTTON() -> &'static str {
    "Sobre"
}
pub fn ABOUT_BUTTON_HELP() -> &'static str {
    "Versão, créditos e licenças do mySSH"
}
pub fn ABOUT_TITLE() -> &'static str {
    "Sobre o mySSH"
}
pub fn ABOUT_TAGLINE() -> &'static str {
    "Sessões SSH, arquivos e sincronização — local, seguro, em Rust."
}
pub fn ABOUT_VERSION() -> &'static str {
    "Versão"
}
pub fn ABOUT_COMMIT() -> &'static str {
    "Commit"
}
pub fn ABOUT_BUILD() -> &'static str {
    "Build"
}
pub fn ABOUT_PLATFORM() -> &'static str {
    "Plataforma"
}
pub fn ABOUT_DATA() -> &'static str {
    "Dados"
}
pub fn ABOUT_UNKNOWN() -> &'static str {
    "desconhecido"
}
pub fn ABOUT_DIRTY() -> &'static str {
    "+modificado"
}
pub fn ABOUT_MADE_WITH() -> &'static str {
    "Feito com"
}
pub fn ABOUT_LICENSE_LABEL() -> &'static str {
    "Licença"
}
pub fn ABOUT_LICENSE() -> &'static str {
    "MIT OR Apache-2.0"
}
pub fn ABOUT_THIRD_PARTY() -> &'static str {
    "Componentes de terceiros"
}
pub fn ABOUT_COPY() -> &'static str {
    "Copiar informações"
}
pub fn ABOUT_COPY_HELP() -> &'static str {
    "Copia versão, commit e plataforma para relatar problemas"
}
pub fn ABOUT_COPIED() -> &'static str {
    "Copiado ✓"
}
pub fn ABOUT_OPEN_DATA() -> &'static str {
    "Abrir pasta de dados"
}
pub fn ABOUT_OPEN_DATA_HELP() -> &'static str {
    "Abre a pasta no gerenciador de arquivos do sistema"
}
pub fn ABOUT_LINK_HELP() -> &'static str {
    "Abrir no navegador"
}

pub fn about_third_party(n: usize) -> String {
    format!("{} ({n})", ABOUT_THIRD_PARTY())
}
