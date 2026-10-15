//! Caminho relativo normalizado e validado.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("{}", myssh_i18n::tr!("nome vazio", "empty name", "nombre vacío"))]
    Empty,
    #[error("{}", myssh_i18n::tr!("nome '.' ou '..' não permitido", "name '.' or '..' not allowed", "nombre '.' o '..' no permitido"))]
    DotComponent,
    #[error("{}", myssh_i18n::tr!("nome contém separador ('/' ou '\\')", "name contains a separator ('/' or '\\')", "el nombre contiene un separador ('/' o '\\')"))]
    Separator,
    #[error("{}", myssh_i18n::tr!("nome contém NUL", "name contains NUL", "el nombre contiene NUL"))]
    Nul,
    #[error("{}", myssh_i18n::tr!("nome com caracteres de controle ou invisíveis", "name with control or invisible characters", "nombre con caracteres de control o invisibles"))]
    Unsafe,
}

/// Caractere proibido em nomes: controle C0 (U+0000–U+001F), DEL, C1 (U+0080–U+009F), marcas e
/// controles bidirecionais (U+061C, U+200E/F, U+202A–202E, U+2066–2069) e invisíveis
/// (U+200B–200D, U+2060, U+FEFF). Enganam a extensão exibida ou injetam sequências de terminal.
pub fn is_unsafe_name_char(c: char) -> bool {
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

/// Caminho relativo à raiz da sincronização, separado por `/`.
///
/// A raiz é o caminho vazio. Só pode ser construído por [`RelPath::root`],
/// [`RelPath::join`] (valida um único nome) ou [`RelPath::parse`] (que apenas
/// divide em `/` e valida cada componente via `join`). Assim nunca contém
/// `..`, `.`, caminho absoluto, componentes vazios, NUL nem `\` (que, vindo de
/// um servidor remoto, não é tratado como separador e é rejeitado).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RelPath(String);

impl RelPath {
    pub fn root() -> Self {
        RelPath(String::new())
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Valida um nome de componente único.
    pub fn validate_name(name: &str) -> Result<(), PathError> {
        if name.is_empty() {
            return Err(PathError::Empty);
        }
        if name == "." || name == ".." {
            return Err(PathError::DotComponent);
        }
        if name.contains('/') || name.contains('\\') {
            return Err(PathError::Separator);
        }
        if name.contains('\0') {
            return Err(PathError::Nul);
        }
        if name.chars().any(is_unsafe_name_char) {
            return Err(PathError::Unsafe);
        }
        Ok(())
    }

    pub fn join(&self, name: &str) -> Result<RelPath, PathError> {
        Self::validate_name(name)?;
        if self.0.is_empty() {
            Ok(RelPath(name.to_string()))
        } else {
            Ok(RelPath(format!("{}/{}", self.0, name)))
        }
    }

    /// Divide em `/` e junta componente a componente (`""` = raiz).
    pub fn parse(s: &str) -> Result<RelPath, PathError> {
        let mut p = RelPath::root();
        if s.is_empty() {
            return Ok(p);
        }
        for c in s.split('/') {
            p = p.join(c)?;
        }
        Ok(p)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|c| !c.is_empty())
    }

    pub fn depth(&self) -> usize {
        self.components().count()
    }

    pub fn name(&self) -> Option<&str> {
        self.components().last()
    }

    pub fn parent(&self) -> Option<RelPath> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rfind('/') {
            Some(i) => RelPath(self.0[..i].to_string()),
            None => RelPath::root(),
        })
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.0)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RelPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        RelPath::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_ok() {
        let p = RelPath::root().join("a b").unwrap().join("ção").unwrap();
        assert_eq!(p.as_str(), "a b/ção");
        assert_eq!(p.depth(), 2);
        assert_eq!(p.name(), Some("ção"));
        assert_eq!(p.parent().unwrap().as_str(), "a b");
        assert!(p.parent().unwrap().parent().unwrap().is_root());
        assert!(RelPath::root().parent().is_none());
    }

    #[test]
    fn traversal_rejeitado() {
        for bad in [
            "../x",
            "a/../../b",
            "a/..",
            "/etc/passwd",
            "a//b",
            "a/",
            "./a",
            "a/./b",
            "C:\\x",
            "a\\b",
            "a\0b",
        ] {
            assert!(RelPath::parse(bad).is_err(), "deveria rejeitar {bad:?}");
        }
        let r = RelPath::root();
        for bad in ["", ".", "..", "a/b", "a\\b", "C:\\x", "x\0"] {
            assert!(r.join(bad).is_err(), "join deveria rejeitar {bad:?}");
        }
    }

    #[test]
    fn nomes_com_controle_bidi_e_invisiveis_rejeitados() {
        let r = RelPath::root();
        for bad in [
            "a\u{1}b",
            "a\nb",
            "a\u{1b}[2Jb",
            "a\u{7f}",
            "a\u{85}b",
            "a\u{9f}",
            "fatura\u{202E}fdp.exe",
            "a\u{202A}b",
            "a\u{2066}b",
            "a\u{2069}",
            "a\u{200E}",
            "a\u{200F}",
            "a\u{061C}",
            "a\u{200B}b",
            "a\u{200C}",
            "a\u{200D}",
            "a\u{2060}",
            "\u{FEFF}a",
        ] {
            assert_eq!(r.join(bad), Err(PathError::Unsafe), "join {bad:?}");
            assert!(
                RelPath::parse(&format!("ok/{bad}")).is_err(),
                "parse {bad:?}"
            );
            assert!(
                serde_json::from_str::<RelPath>(&serde_json::to_string(bad).unwrap()).is_err(),
                "serde {bad:?}"
            );
        }
        for ok in ["ação", "日本語", "a b", "e\u{301}", "😀", "a\u{a0}b"] {
            assert!(r.join(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn parse_valido_e_serde() {
        let p = RelPath::parse("a/b c/d").unwrap();
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, "\"a/b c/d\"");
        assert_eq!(serde_json::from_str::<RelPath>(&j).unwrap(), p);
        assert!(serde_json::from_str::<RelPath>("\"../x\"").is_err());
        assert!(RelPath::parse("").unwrap().is_root());
    }
}

#[cfg(test)]
mod i18n_tests {
    use super::*;

    #[test]
    fn erros_traduzidos() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(PathError::Empty.to_string(), "empty name");
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(PathError::Empty.to_string(), "nombre vacío");
        });
        assert_eq!(PathError::Empty.to_string(), "nome vazio");
    }
}
