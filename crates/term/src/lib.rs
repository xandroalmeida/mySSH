//! Terminal do mySSH: `alacritty_terminal` (emulação) + widget egui sobre uma sessão `myssh-ssh`.
//!
//! - [`TermSession`]: junta a sessão de rede com o emulador. Não decide host key sozinha.
//! - [`widget::show`]: desenha e trata entrada (teclado, mouse, seleção, colar, rolagem).
//! - [`TermTheme`] / [`TermStyle`]: aparência (o tema por perfil virá daqui).
//!
//! Segurança: OSC 52 (clipboard) e hyperlinks da saída remota são ignorados; o título remoto é
//! exposto apenas como texto sanitizado e limitado.

pub mod keys;
pub mod session;
pub mod theme;
pub mod title;
pub mod widget;

pub use session::{
    AuthPromptInfo, DEFAULT_PUMP_BUDGET, DEFAULT_SCROLLBACK, HostKeyAlert, HostKeyPrompt,
    TermSession,
};
pub use theme::{TermStyle, TermTheme};
pub use widget::{TermResponse, show};
