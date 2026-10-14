//! Acesso a arquivos de `Include`, abstraído para testes sem disco.

use crate::paths::{has_wildcard, wildcard_match};
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::PathBuf;

/// Limite de tamanho por arquivo lido (1 MiB).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Fonte de arquivos para `Include`. Somente leitura.
pub trait IncludeSource {
    /// Caminhos que casam com o padrão (já absoluto/expandido), ordenados.
    fn expand(&self, pattern: &str) -> Vec<String>;
    fn read(&self, path: &str) -> io::Result<String>;
}

/// Sistema de arquivos em memória (testes e pré-visualizações).
#[derive(Default, Clone)]
pub struct MemFs {
    files: BTreeMap<String, String>,
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, path: &str, content: &str) -> Self {
        self.files.insert(path.to_string(), content.to_string());
        self
    }
}

impl IncludeSource for MemFs {
    fn expand(&self, pattern: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|k| wildcard_match(pattern, k))
            .cloned()
            .collect()
    }
    fn read(&self, path: &str) -> io::Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

/// Disco real, somente leitura, com limite de 1 MiB por arquivo.
pub struct DiskFs;

impl IncludeSource for DiskFs {
    fn expand(&self, pattern: &str) -> Vec<String> {
        let mut comps = pattern.split(['/', '\\']).filter(|c| !c.is_empty());
        let mut cur: Vec<PathBuf> = Vec::new();
        if pattern.starts_with(['/', '\\']) {
            cur.push(PathBuf::from(std::path::MAIN_SEPARATOR_STR));
        } else if let Some(first) = comps.next() {
            if first.ends_with(':') {
                cur.push(PathBuf::from(format!("{first}\\")));
            } else {
                cur.push(PathBuf::from(first));
            }
        }
        for c in comps {
            let mut next = Vec::new();
            for base in &cur {
                if has_wildcard(c) {
                    if let Ok(rd) = std::fs::read_dir(base) {
                        for e in rd.flatten() {
                            if let Some(n) = e.file_name().to_str()
                                && wildcard_match(c, n)
                            {
                                next.push(base.join(n));
                            }
                        }
                    }
                } else {
                    next.push(base.join(c));
                }
            }
            cur = next;
        }
        let mut res: Vec<String> = cur
            .into_iter()
            .filter(|p| p.is_file())
            .filter_map(|p| p.to_str().map(str::to_string))
            .collect();
        res.sort();
        res
    }

    fn read(&self, path: &str) -> io::Result<String> {
        read_limited(path)
    }
}

pub fn read_limited(path: &str) -> io::Result<String> {
    let f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    f.take(MAX_FILE_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "arquivo maior que 1 MiB",
        ));
    }
    String::from_utf8(buf).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "não é UTF-8"))
}
