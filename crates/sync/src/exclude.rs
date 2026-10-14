//! Exclusões por padrões glob (estilo simplificado de .gitignore).

use crate::relpath::RelPath;
use globset::{GlobBuilder, GlobMatcher};

struct Rule {
    matcher: GlobMatcher,
    dir_only: bool,
}

/// Conjunto de padrões de exclusão.
///
/// Sintaxe: `nome/` casa só diretórios; padrão sem `/` interno casa o nome em
/// qualquer profundidade (`*.log`); padrão com `/` interno ou iniciado em `/`
/// é ancorado na raiz (`build/out`). `*` não cruza `/`; `**` cruza. Não há
/// negação (`!`). Diretório excluído não é percorrido (nem na origem nem no
/// destino; itens excluídos no destino nunca são apagados no modo espelho).
pub struct Exclusions {
    rules: Vec<Rule>,
}

impl Default for Exclusions {
    /// Padrão: apenas `.git/`.
    fn default() -> Self {
        let mut e = Exclusions::none();
        e.add(".git/").expect("padrão fixo válido");
        e
    }
}

impl std::fmt::Debug for Exclusions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Exclusions({} regras)", self.rules.len())
    }
}

impl Exclusions {
    /// Sem nenhuma exclusão (nem `.git/`).
    pub fn none() -> Self {
        Exclusions { rules: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<&mut Self, globset::Error> {
        let mut p = pattern.trim();
        let dir_only = p.ends_with('/');
        p = p.trim_end_matches('/');
        let anchored = p.starts_with('/');
        p = p.trim_start_matches('/');
        let glob = if !anchored && !p.contains('/') {
            format!("**/{p}")
        } else {
            p.to_string()
        };
        let matcher = GlobBuilder::new(&glob)
            .literal_separator(true)
            .build()?
            .compile_matcher();
        self.rules.push(Rule { matcher, dir_only });
        Ok(self)
    }

    pub fn is_excluded(&self, path: &RelPath, is_dir: bool) -> bool {
        self.rules
            .iter()
            .any(|r| (!r.dir_only || is_dir) && r.matcher.is_match(path.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RelPath {
        RelPath::parse(s).unwrap()
    }

    #[test]
    fn padrao_exclui_git_so_diretorio() {
        let e = Exclusions::default();
        assert!(e.is_excluded(&p(".git"), true));
        assert!(e.is_excluded(&p("sub/.git"), true));
        assert!(!e.is_excluded(&p(".git"), false));
        assert!(!e.is_excluded(&p(".gitignore"), false));
    }

    #[test]
    fn extensoes_e_ancorados() {
        let mut e = Exclusions::none();
        e.add("*.log").unwrap();
        e.add("/build").unwrap();
        e.add("node_modules/").unwrap();
        assert!(e.is_excluded(&p("a/b/x.log"), false));
        assert!(e.is_excluded(&p("x.log"), false));
        assert!(e.is_excluded(&p("build"), true));
        assert!(!e.is_excluded(&p("src/build"), true));
        assert!(e.is_excluded(&p("a/node_modules"), true));
        assert!(!e.is_excluded(&p("a/node_modules"), false));
        assert!(!Exclusions::none().is_excluded(&p(".git"), true));
    }
}
