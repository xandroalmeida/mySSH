//! Validação e derivação de nomes: traversal, nomes inválidos no sistema local, colisões por
//! caixa/normalização, política "nome (1).ext" e nomes de temporário/sidecar.

use crate::error::TransferError;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Sistema de arquivos local para fins de validação de nomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalOs {
    Windows,
    MacOs,
    Linux,
}

impl LocalOs {
    pub fn current() -> Self {
        if cfg!(windows) {
            LocalOs::Windows
        } else if cfg!(target_os = "macos") {
            LocalOs::MacOs
        } else {
            LocalOs::Linux
        }
    }

    /// Padrão do sistema: APFS e NTFS ignoram caixa por padrão (volumes podem divergir; ver
    /// `TransferManager::set_local_case_insensitive`).
    pub fn default_case_insensitive(self) -> bool {
        !matches!(self, LocalOs::Linux)
    }
}

/// Máximo de bytes de um componente (NAME_MAX).
pub const NAME_MAX: usize = 255;

/// Nome vindo do servidor: não pode escapar da raiz (`/`, `..`, `.`, vazio ou NUL).
pub fn validate_remote_name(name: &str) -> Result<(), TransferError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(TransferError::OutsideRoot(display_name(name)));
    }
    Ok(())
}

/// Caractere que não deve aparecer em nomes de arquivo exibidos ou criados: controle C0
/// (U+0000–U+001F), DEL, C1 (U+0080–U+009F), marcas/controles bidirecionais (U+061C, U+200E/F,
/// U+202A–202E, U+2066–2069) e invisíveis (U+200B–200D, U+2060, U+FEFF). Servem para enganar a
/// extensão exibida (`fatura\u{202E}fdp.exe`) ou injetar sequências de terminal.
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// O nome contém algum caractere de [`is_unsafe_char`].
pub fn has_unsafe_chars(name: &str) -> bool {
    name.chars().any(is_unsafe_char)
}

const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Nome válido como componente de caminho no sistema local `os` (sem renomear silenciosamente).
pub fn validate_local_name(os: LocalOs, name: &str) -> Result<(), TransferError> {
    let bad = || Err(TransferError::InvalidLocalName(display_name(name)));
    if name.is_empty() || name == "." || name == ".." || name.contains('\0') || name.contains('/') {
        return bad();
    }
    if name.len() > NAME_MAX {
        return bad();
    }
    // Em todos os SOs (não só no Windows): controle, bidi e invisíveis.
    if has_unsafe_chars(name) {
        return Err(TransferError::UnsafeName(display_name(name)));
    }
    if os == LocalOs::Windows {
        if name.chars().any(|c| {
            matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20
        }) {
            return bad();
        }
        if name.ends_with(' ') || name.ends_with('.') {
            return bad();
        }
        let stem = name.split('.').next().unwrap_or(name).trim_end();
        if WINDOWS_RESERVED
            .iter()
            .any(|r| r.eq_ignore_ascii_case(stem))
        {
            return bad();
        }
    }
    Ok(())
}

/// Chave de comparação de nomes para sistemas que ignoram caixa (e, no macOS, a forma de
/// normalização Unicode: APFS trata NFC e NFD como o mesmo nome).
pub fn fold_key(os: LocalOs, case_insensitive: bool, name: &str) -> String {
    if !case_insensitive && os == LocalOs::Linux {
        return name.to_string();
    }
    let n: String = name.nfc().collect();
    if case_insensitive {
        n.to_lowercase()
    } else {
        n
    }
}

/// Grupos de nomes que colidem entre si (tamanho >= 2) segundo [`fold_key`].
pub fn collisions(os: LocalOs, case_insensitive: bool, names: &[String]) -> Vec<Vec<String>> {
    let mut by: HashMap<String, Vec<String>> = HashMap::new();
    for n in names {
        by.entry(fold_key(os, case_insensitive, n))
            .or_default()
            .push(n.clone());
    }
    let mut out: Vec<Vec<String>> = by
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort();
            g
        })
        .collect();
    out.sort();
    out
}

/// "nome.ext" -> "nome (n).ext"; sem extensão (ou arquivo oculto) -> "nome (n)".
pub fn rename_candidate(name: &str, n: u32) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => {
            format!("{} ({n}){}", &name[..i], &name[i..])
        }
        _ => format!("{name} ({n})"),
    }
}

/// Texto seguro para mensagens e para a UI (caracteres de [`is_unsafe_char`] viram U+FFFD,
/// tamanho limitado).
pub fn display_name(name: &str) -> String {
    name.chars()
        .map(|c| if is_unsafe_char(c) { '\u{FFFD}' } else { c })
        .take(120)
        .collect()
}

// ------------------------------------------------------------- remoto: strings

/// Junta pai e nome em caminho remoto POSIX.
pub fn remote_join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Último componente de um caminho remoto (ignora `/` finais). `None` para raiz/vazio.
pub fn remote_basename(path: &str) -> Option<&str> {
    let t = path.trim_end_matches('/');
    let n = t.rsplit('/').next()?;
    if n.is_empty() { None } else { Some(n) }
}

/// Diretório pai de um caminho remoto.
pub fn remote_dirname(path: &str) -> String {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => t[..i].to_string(),
        None => ".".to_string(),
    }
}

// -------------------------------------------------------- temporário e sidecar

const MAX_BASE: usize = 150;
pub const PART_MARK: &str = ".myssh-part-";
pub const RESUME_SUFFIX: &str = ".myssh-resume";

/// Prefixo seguro (limita nomes enormes para o temporário caber em NAME_MAX).
fn short(final_name: &str) -> &str {
    if final_name.len() <= MAX_BASE {
        return final_name;
    }
    let mut i = MAX_BASE;
    while !final_name.is_char_boundary(i) {
        i -= 1;
    }
    &final_name[..i]
}

pub fn part_prefix(final_name: &str) -> String {
    format!(".{}{PART_MARK}", short(final_name))
}

/// `.<nome>.myssh-part-<rand>`
pub fn part_name(final_name: &str, rand: &str) -> String {
    format!("{}{rand}", part_prefix(final_name))
}

/// `.<nome>.myssh-resume`
pub fn sidecar_name(final_name: &str) -> String {
    format!(".{}{RESUME_SUFFIX}", short(final_name))
}

/// `temp` é um nome de temporário legítimo para `final_name` (o sidecar vem do servidor/disco:
/// não é confiável e não pode apontar para outro caminho).
pub fn is_part_name_for(final_name: &str, temp: &str) -> bool {
    let p = part_prefix(final_name);
    temp.starts_with(&p)
        && temp.len() <= NAME_MAX
        && temp.len() > p.len()
        && !temp.contains('/')
        && !temp.contains('\0')
        && !temp.contains('\\')
}

/// Artefatos do próprio mySSH (ignorados ao varrer origens).
pub fn is_artifact(name: &str) -> bool {
    name.starts_with('.') && (name.contains(PART_MARK) || name.ends_with(RESUME_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_remoto() {
        for n in ["..", ".", "", "a/b", "../x", "a\0b", "/etc"] {
            assert!(
                matches!(validate_remote_name(n), Err(TransferError::OutsideRoot(_))),
                "{n:?}"
            );
        }
        for n in ["a b", "ação.txt", "..a", "a..", "日本語", ".bashrc", "a\\b"] {
            assert!(validate_remote_name(n).is_ok(), "{n:?}");
        }
    }

    #[test]
    fn nomes_windows() {
        let w = LocalOs::Windows;
        for n in [
            "a:b", "a<b", "a>b", "a\"b", "a|b", "a?b", "a*b", "a\\b", "CON", "con.txt", "Nul",
            "COM1.log", "LPT9", "fim.", "fim ", "a\u{1}b",
        ] {
            assert!(validate_local_name(w, n).is_err(), "{n:?}");
        }
        for n in ["CONSOLE", "com10", "ok.txt", "a b", "ação"] {
            assert!(validate_local_name(w, n).is_ok(), "{n:?}");
        }
    }

    #[test]
    fn nomes_macos_e_linux() {
        for os in [LocalOs::MacOs, LocalOs::Linux] {
            assert!(validate_local_name(os, "a:b").is_ok());
            assert!(validate_local_name(os, "CON").is_ok());
            assert!(validate_local_name(os, "a/b").is_err());
            assert!(validate_local_name(os, "..").is_err());
            assert!(validate_local_name(os, "a\0b").is_err());
            assert!(validate_local_name(os, &"x".repeat(255)).is_ok());
            assert!(validate_local_name(os, &"x".repeat(256)).is_err());
            // 128 x "é" = 256 bytes
            assert!(validate_local_name(os, &"é".repeat(128)).is_err());
        }
    }

    #[test]
    fn nomes_com_controle_bidi_e_invisiveis_rejeitados_em_todos_os_sos() {
        let ruins = [
            "a\u{1}b",
            "a\u{1b}[31mb",
            "a\nb",
            "a\u{7f}b",
            "a\u{80}b",
            "a\u{9f}b",
            "fatura\u{202E}fdp.exe",
            "a\u{202A}b",
            "a\u{2066}b",
            "a\u{2069}b",
            "a\u{200E}b",
            "a\u{200F}b",
            "a\u{061C}b",
            "a\u{200B}b",
            "a\u{200C}b",
            "a\u{200D}b",
            "a\u{2060}b",
            "\u{FEFF}a",
        ];
        for os in [LocalOs::Windows, LocalOs::MacOs, LocalOs::Linux] {
            for n in ruins {
                assert!(
                    matches!(
                        validate_local_name(os, n),
                        Err(TransferError::UnsafeName(_))
                    ),
                    "{os:?} {n:?}"
                );
                assert!(!display_name(n).chars().any(is_unsafe_char), "{n:?}");
            }
            for n in [
                "ação.txt",
                "日本語",
                "a b",
                "e\u{301}",
                "😀.txt",
                "a\u{a0}b",
            ] {
                assert!(validate_local_name(os, n).is_ok(), "{os:?} {n:?}");
            }
        }
        let msg = TransferError::UnsafeName("x".into()).to_string();
        assert!(
            msg.contains("caracteres de controle ou invisíveis"),
            "{msg}"
        );
    }

    #[test]
    fn colisao_por_caixa_e_normalizacao() {
        let names: Vec<String> = ["Foo.txt", "foo.txt", "bar", "e\u{301}.txt", "\u{e9}.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let g = collisions(LocalOs::MacOs, true, &names);
        assert_eq!(g.len(), 2, "{g:?}");
        assert!(g.contains(&vec!["Foo.txt".to_string(), "foo.txt".to_string()]));
        // Linux sensível a caixa: sem colisões.
        assert!(collisions(LocalOs::Linux, false, &names).is_empty());
        // Volume sensível a caixa no macOS: só a normalização colide.
        let g = collisions(LocalOs::MacOs, false, &names);
        assert_eq!(g.len(), 1, "{g:?}");
    }

    #[test]
    fn renomear_nome_n_ext() {
        assert_eq!(rename_candidate("a.txt", 1), "a (1).txt");
        assert_eq!(rename_candidate("a.tar.gz", 2), "a.tar (2).gz");
        assert_eq!(rename_candidate("README", 1), "README (1)");
        assert_eq!(rename_candidate(".bashrc", 1), ".bashrc (1)");
        assert_eq!(rename_candidate("fim.", 3), "fim. (3)");
        assert_eq!(
            rename_candidate("relatório final.pdf", 10),
            "relatório final (10).pdf"
        );
    }

    #[test]
    fn caminhos_remotos() {
        assert_eq!(remote_join("/a", "b"), "/a/b");
        assert_eq!(remote_join("/", "b"), "/b");
        assert_eq!(remote_join("/a/", "b"), "/a/b");
        assert_eq!(remote_basename("/a/b/"), Some("b"));
        assert_eq!(remote_basename("/"), None);
        assert_eq!(remote_dirname("/a/b"), "/a");
        assert_eq!(remote_dirname("/a"), "/");
        assert_eq!(remote_dirname("a"), ".");
    }

    #[test]
    fn nomes_de_temporario() {
        let t = part_name("rel.pdf", "abc123");
        assert_eq!(t, ".rel.pdf.myssh-part-abc123");
        assert!(is_part_name_for("rel.pdf", &t));
        assert!(!is_part_name_for("outro.pdf", &t));
        assert!(!is_part_name_for("rel.pdf", ".rel.pdf.myssh-part-"));
        assert!(!is_part_name_for(
            "rel.pdf",
            ".rel.pdf.myssh-part-../../etc/passwd"
        ));
        assert!(is_artifact(&t));
        assert!(is_artifact(&sidecar_name("rel.pdf")));
        assert!(!is_artifact("rel.pdf"));
        let longo = "ç".repeat(120);
        let t = part_name(&longo, "0123456789abcdef");
        assert!(t.len() <= NAME_MAX, "{}", t.len());
        assert!(sidecar_name(&longo).len() <= NAME_MAX);
    }
}
