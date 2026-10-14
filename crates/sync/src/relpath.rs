//! Caminho relativo normalizado e validado.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("nome vazio")]
    Empty,
    #[error("nome '.' ou '..' não permitido")]
    DotComponent,
    #[error("nome contém separador ('/' ou '\\')")]
    Separator,
    #[error("nome contém NUL")]
    Nul,
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
    fn parse_valido_e_serde() {
        let p = RelPath::parse("a/b c/d").unwrap();
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, "\"a/b c/d\"");
        assert_eq!(serde_json::from_str::<RelPath>(&j).unwrap(), p);
        assert!(serde_json::from_str::<RelPath>("\"../x\"").is_err());
        assert!(RelPath::parse("").unwrap().is_root());
    }
}
