//! Atalhos globais da aplicação.
//!
//! # Regra
//! * **macOS**: `Cmd` + tecla, sempre. O terminal nunca usa `Cmd` (ele usa `Ctrl`), então
//!   não há conflito, mesmo com o terminal focado.
//! * **Windows/Linux, foco fora do terminal**: `Ctrl` + tecla.
//! * **Windows/Linux, foco no terminal**: somente `Ctrl+Shift` + tecla. `Ctrl+T`, `Ctrl+W`,
//!   `Ctrl+N` e `Ctrl+F` pertencem ao programa remoto (readline, vim, tmux...) e passam
//!   direto para a sessão.
//!
//! Teclas: `T` nova aba do perfil selecionado, `W` fechar aba, `N` novo perfil,
//! `F` foco na busca. Atalhos ficam desativados enquanto há um diálogo modal aberto.
//!
//! Observação: `egui::Modifiers::matches_logically` ignora `Shift` extra, então quando o foco
//! está fora do terminal `Ctrl+Shift+T` também dispara — aceitável e coerente com a regra.

use egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NewTab,
    CloseTab,
    NewProfile,
    FocusSearch,
}

pub const ACTIONS: [Action; 4] = [
    Action::NewTab,
    Action::CloseTab,
    Action::NewProfile,
    Action::FocusSearch,
];

pub fn key(a: Action) -> Key {
    match a {
        Action::NewTab => Key::T,
        Action::CloseTab => Key::W,
        Action::NewProfile => Key::N,
        Action::FocusSearch => Key::F,
    }
}

/// Modificadores exigidos (ver regra no topo do módulo). `COMMAND` é Cmd no macOS e Ctrl nos demais.
pub fn required_modifiers(terminal_focused: bool, is_mac: bool) -> Modifiers {
    if !is_mac && terminal_focused {
        Modifiers::COMMAND | Modifiers::SHIFT
    } else {
        Modifiers::COMMAND
    }
}

/// Consome e devolve as ações acionadas neste frame.
pub fn poll(ctx: &egui::Context, terminal_focused: bool) -> Vec<Action> {
    let mods = required_modifiers(terminal_focused, cfg!(target_os = "macos"));
    ctx.input_mut(|i| {
        ACTIONS
            .into_iter()
            .filter(|a| i.consume_key(mods, key(*a)))
            .collect()
    })
}

/// Texto do atalho para exibir na UI (ex.: "Cmd+T" / "Ctrl+T").
pub fn label(a: Action) -> String {
    let m = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    format!("{m}+{}", key(a).name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_sempre_cmd() {
        for focused in [false, true] {
            assert_eq!(required_modifiers(focused, true), Modifiers::COMMAND);
        }
    }

    #[test]
    fn outros_so_ctrl_shift_no_terminal() {
        assert_eq!(required_modifiers(false, false), Modifiers::COMMAND);
        let m = required_modifiers(true, false);
        assert!(m.command && m.shift);
    }

    #[test]
    fn teclas_distintas() {
        let keys: Vec<_> = ACTIONS.iter().map(|a| key(*a)).collect();
        assert_eq!(keys, [Key::T, Key::W, Key::N, Key::F]);
    }

    #[test]
    fn ctrl_simples_nao_dispara_com_terminal_focado() {
        // Modifiers::matches_logically: o evento Ctrl+T não satisfaz Ctrl+Shift.
        let ev = Modifiers::COMMAND;
        assert!(!ev.matches_logically(required_modifiers(true, false)));
        assert!(ev.matches_logically(required_modifiers(false, false)));
        let ev = Modifiers::COMMAND | Modifiers::SHIFT;
        assert!(ev.matches_logically(required_modifiers(true, false)));
    }
}
