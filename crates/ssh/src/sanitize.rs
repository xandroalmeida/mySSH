//! Sanitização de texto remoto não confiável (títulos de terminal, prompts de keyboard-interactive).
//! Fonte única: `myssh-term` reutiliza esta função para o título.

/// Remove controles C0/C1 (exceto `\n` quando `keep_newlines`), marcas e overrides bidirecionais
/// (U+202A–202E, U+2066–2069, U+200E/F), zero-width (U+200B–200D, U+2060–2064) e BOM (U+FEFF);
/// limita a `max_chars` caracteres e apara espaços nas pontas.
pub fn sanitize_text(raw: &str, max_chars: usize, keep_newlines: bool) -> String {
    let out: String = raw
        .chars()
        .filter(|c| {
            (!c.is_control() || (keep_newlines && *c == '\n'))
                && !matches!(*c,
                    '\u{200B}'..='\u{200F}' // zero-width, marcas LTR/RTL
                    | '\u{202A}'..='\u{202E}' // embeddings/overrides bidi
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{2069}' // isolates bidi
                    | '\u{FEFF}')
        })
        .take(max_chars)
        .collect();
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_controles_bidi_e_invisiveis() {
        assert_eq!(sanitize_text("a\x1b[31m\x07b\u{9b}", 100, false), "a[31mb");
        assert_eq!(
            sanitize_text("a\u{202E}b\u{200B}c\u{2066}d\u{FEFF}e\u{200F}", 100, false),
            "abcde"
        );
        assert_eq!(sanitize_text("a\nb", 100, false), "ab");
        assert_eq!(sanitize_text("a\nb", 100, true), "a\nb");
    }

    #[test]
    fn limita_em_caracteres() {
        assert_eq!(
            sanitize_text(&"é".repeat(5000), 10, false).chars().count(),
            10
        );
    }
}
