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
//! Painéis divididos: `Cmd/Ctrl+Shift+D` divide lado a lado, `+E` empilhado (mesmo perfil do
//! painel focado), `+W` fecha o painel focado (ou a aba, se não estiver dividida) e `+J` passa
//! o foco ao próximo painel. Estes exigem **sempre** `Shift` (também no macOS e fora do
//! terminal): `Ctrl+D` é fim de arquivo no shell remoto e `Ctrl+E`/`Ctrl+J` são do readline.
//! Eles são consultados antes dos demais para que `Ctrl+Shift+W` não acione também "fechar aba".
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
    SplitSide,
    SplitStack,
    ClosePane,
    NextPane,
}

/// Ordem de consulta: os que exigem `Shift` vêm primeiro (ver nota no topo do módulo).
pub const ACTIONS: [Action; 8] = [
    Action::SplitSide,
    Action::SplitStack,
    Action::ClosePane,
    Action::NextPane,
    Action::NewTab,
    Action::CloseTab,
    Action::NewProfile,
    Action::FocusSearch,
];

/// Ações que exigem `Shift` em qualquer plataforma/foco.
pub fn needs_shift(a: Action) -> bool {
    matches!(
        a,
        Action::SplitSide | Action::SplitStack | Action::ClosePane | Action::NextPane
    )
}

pub fn key(a: Action) -> Key {
    match a {
        Action::NewTab => Key::T,
        Action::CloseTab => Key::W,
        Action::NewProfile => Key::N,
        Action::FocusSearch => Key::F,
        Action::SplitSide => Key::D,
        Action::SplitStack => Key::E,
        Action::ClosePane => Key::W,
        Action::NextPane => Key::J,
    }
}

/// Modificadores exigidos (ver regra no topo do módulo). `COMMAND` é Cmd no macOS e Ctrl nos demais.
pub fn required_modifiers(a: Action, terminal_focused: bool, is_mac: bool) -> Modifiers {
    if needs_shift(a) || (!is_mac && terminal_focused) {
        Modifiers::COMMAND | Modifiers::SHIFT
    } else {
        Modifiers::COMMAND
    }
}

/// Consome e devolve as ações acionadas neste frame.
pub fn poll(ctx: &egui::Context, terminal_focused: bool) -> Vec<Action> {
    let mac = cfg!(target_os = "macos");
    ctx.input_mut(|i| {
        ACTIONS
            .into_iter()
            .filter(|a| i.consume_key(required_modifiers(*a, terminal_focused, mac), key(*a)))
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
    let shift = if needs_shift(a) { "Shift+" } else { "" };
    format!("{m}+{shift}{}", key(a).name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_sempre_cmd() {
        for focused in [false, true] {
            assert_eq!(
                required_modifiers(Action::NewTab, focused, true),
                Modifiers::COMMAND
            );
        }
    }

    #[test]
    fn outros_so_ctrl_shift_no_terminal() {
        assert_eq!(
            required_modifiers(Action::NewTab, false, false),
            Modifiers::COMMAND
        );
        let m = required_modifiers(Action::NewTab, true, false);
        assert!(m.command && m.shift);
    }

    #[test]
    fn divisao_exige_shift_sempre() {
        for a in ACTIONS.into_iter().filter(|a| needs_shift(*a)) {
            for (focused, mac) in [(false, false), (true, false), (false, true), (true, true)] {
                let m = required_modifiers(a, focused, mac);
                assert!(m.command && m.shift, "{a:?} focused={focused} mac={mac}");
            }
        }
        // Ctrl+D puro (EOF no shell) nunca divide
        let ev = Modifiers::COMMAND;
        assert!(!ev.matches_logically(required_modifiers(Action::SplitSide, false, false)));
    }

    #[test]
    fn teclas_distintas_por_exigencia() {
        // mesma tecla só entre ações com exigências diferentes (W: fechar painel x fechar aba)
        for (i, a) in ACTIONS.iter().enumerate() {
            for b in &ACTIONS[i + 1..] {
                if key(*a) == key(*b) {
                    assert_ne!(needs_shift(*a), needs_shift(*b));
                }
            }
        }
        // os que exigem Shift são consultados primeiro
        let first_plain = ACTIONS.iter().position(|a| !needs_shift(*a)).unwrap();
        assert!(ACTIONS[first_plain..].iter().all(|a| !needs_shift(*a)));
    }

    #[test]
    fn ctrl_simples_nao_dispara_com_terminal_focado() {
        // Modifiers::matches_logically: o evento Ctrl+T não satisfaz Ctrl+Shift.
        let ev = Modifiers::COMMAND;
        assert!(!ev.matches_logically(required_modifiers(Action::NewTab, true, false)));
        assert!(ev.matches_logically(required_modifiers(Action::NewTab, false, false)));
        let ev = Modifiers::COMMAND | Modifiers::SHIFT;
        assert!(ev.matches_logically(required_modifiers(Action::NewTab, true, false)));
    }

    #[test]
    fn rotulos() {
        assert!(label(Action::SplitSide).ends_with("Shift+D"));
        assert!(label(Action::NewTab).ends_with("+T"));
    }
}
