//! Título remoto (OSC 0/2): nunca confiável. Exposto só como texto sem controles, com limite.

/// Máximo de caracteres do título exposto à UI.
pub const MAX_TITLE_CHARS: usize = 128;

/// Remove caracteres de controle (C0/C1), formatação bidirecional e invisíveis que permitiriam
/// disfarçar o título; limita o tamanho.
pub fn sanitize_title(raw: &str) -> String {
    myssh_ssh::sanitize_text(raw, MAX_TITLE_CHARS, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_controles_e_escapes() {
        assert_eq!(
            sanitize_title("ola\x1b[31m\x07\r\nmundo\u{9b}"),
            "ola[31mmundo"
        );
        assert_eq!(sanitize_title("  espaço  "), "espaço");
    }

    #[test]
    fn remove_bidi_e_invisiveis() {
        assert_eq!(sanitize_title("a\u{202E}b\u{200B}c\u{2066}d"), "abcd");
    }

    #[test]
    fn limita_tamanho_em_caracteres() {
        let t = sanitize_title(&"é".repeat(1000));
        assert_eq!(t.chars().count(), MAX_TITLE_CHARS);
    }
}
