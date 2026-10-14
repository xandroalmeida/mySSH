//! Textos da janela "Sobre".

pub const ABOUT_BUTTON: &str = "Sobre";
pub const ABOUT_BUTTON_HELP: &str = "Versão, créditos e licenças do mySSH";
pub const ABOUT_TITLE: &str = "Sobre o mySSH";
pub const ABOUT_TAGLINE: &str = "Sessões SSH, arquivos e sincronização — local, seguro, em Rust.";
pub const ABOUT_VERSION: &str = "Versão";
pub const ABOUT_COMMIT: &str = "Commit";
pub const ABOUT_BUILD: &str = "Build";
pub const ABOUT_PLATFORM: &str = "Plataforma";
pub const ABOUT_DATA: &str = "Dados";
pub const ABOUT_UNKNOWN: &str = "desconhecido";
pub const ABOUT_DIRTY: &str = "+modificado";
pub const ABOUT_MADE_WITH: &str = "Feito com";
pub const ABOUT_LICENSE_LABEL: &str = "Licença";
pub const ABOUT_LICENSE: &str = "MIT OR Apache-2.0";
pub const ABOUT_THIRD_PARTY: &str = "Componentes de terceiros";
pub const ABOUT_COPY: &str = "Copiar informações";
pub const ABOUT_COPY_HELP: &str = "Copia versão, commit e plataforma para relatar problemas";
pub const ABOUT_COPIED: &str = "Copiado ✓";
pub const ABOUT_OPEN_DATA: &str = "Abrir pasta de dados";
pub const ABOUT_OPEN_DATA_HELP: &str = "Abre a pasta no gerenciador de arquivos do sistema";
pub const ABOUT_LINK_HELP: &str = "Abrir no navegador";

pub fn about_third_party(n: usize) -> String {
    format!("{ABOUT_THIRD_PARTY} ({n})")
}
