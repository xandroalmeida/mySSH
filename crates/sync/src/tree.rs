//! Abstração de árvore de arquivos (local, remota, em memória).

use crate::relpath::RelPath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    /// Granularidade do relógio de arquivos deste sistema (ex.: 2 s em FAT).
    pub mtime_resolution: Duration,
    pub perms: Option<u32>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TreeError {
    #[error("não encontrado: {0}")]
    NotFound(String),
    #[error("não é um diretório: {0}")]
    NotADirectory(String),
    #[error("erro de E/S: {0}")]
    Io(String),
}

/// Árvore de arquivos somente leitura. Nunca segue symlinks.
///
/// Nomes devolvidos por `list_dir` NÃO são confiáveis (podem vir de um
/// servidor remoto); o planejador os valida com [`RelPath::join`].
pub trait FileTree: Sync {
    fn list_dir(
        &self,
        path: &RelPath,
    ) -> impl Future<Output = Result<Vec<Entry>, TreeError>> + Send;
    fn stat(&self, path: &RelPath)
    -> impl Future<Output = Result<Option<Entry>, TreeError>> + Send;
}

// ---------------------------------------------------------------- LocalTree

/// Árvore no disco local sob uma raiz. Usa `symlink_metadata` (não segue links).
/// Nomes não-UTF-8 são devolvidos com `kind = Other` e nome "lossy".
#[derive(Debug, Clone)]
pub struct LocalTree {
    root: PathBuf,
}

impl LocalTree {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalTree { root: root.into() }
    }

    fn fs_path(&self, rel: &RelPath) -> PathBuf {
        let mut p = self.root.clone();
        for c in rel.components() {
            p.push(c);
        }
        p
    }
}

#[cfg(unix)]
const LOCAL_RES: Duration = Duration::from_nanos(1);
#[cfg(not(unix))]
const LOCAL_RES: Duration = Duration::from_nanos(100);

fn entry_from_meta(name: String, md: &std::fs::Metadata, valid_name: bool) -> Entry {
    let ft = md.file_type();
    let kind = if !valid_name {
        EntryKind::Other
    } else if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    #[cfg(unix)]
    let perms = {
        use std::os::unix::fs::PermissionsExt;
        Some(md.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let perms = None;
    Entry {
        name,
        kind,
        size: if kind == EntryKind::File { md.len() } else { 0 },
        mtime: md.modified().ok(),
        mtime_resolution: LOCAL_RES,
        perms,
    }
}

impl FileTree for LocalTree {
    async fn list_dir(&self, path: &RelPath) -> Result<Vec<Entry>, TreeError> {
        let dir = self.fs_path(path);
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(TreeError::NotFound(path.to_string()));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => {
                return Err(TreeError::NotADirectory(path.to_string()));
            }
            Err(e) => return Err(TreeError::Io(e.to_string())),
        };
        let mut out = Vec::new();
        while let Some(de) = rd
            .next_entry()
            .await
            .map_err(|e| TreeError::Io(e.to_string()))?
        {
            let md = match tokio::fs::symlink_metadata(de.path()).await {
                Ok(m) => m,
                // Removido entre listar e consultar: ignora.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(TreeError::Io(e.to_string())),
            };
            let (name, valid) = match de.file_name().into_string() {
                Ok(s) => (s, true),
                Err(os) => (os.to_string_lossy().into_owned(), false),
            };
            out.push(entry_from_meta(name, &md, valid));
        }
        Ok(out)
    }

    async fn stat(&self, path: &RelPath) -> Result<Option<Entry>, TreeError> {
        let p = self.fs_path(path);
        match tokio::fs::symlink_metadata(&p).await {
            Ok(md) => Ok(Some(entry_from_meta(
                path.name().unwrap_or("").to_string(),
                &md,
                true,
            ))),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(TreeError::Io(e.to_string())),
        }
    }
}

// ------------------------------------------------------------------ MemTree

/// Árvore em memória para testes.
#[derive(Debug, Clone)]
pub struct MemTree {
    dirs: HashMap<String, Vec<Entry>>,
    resolution: Duration,
}

impl Default for MemTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MemTree {
    pub fn new() -> Self {
        let mut dirs = HashMap::new();
        dirs.insert(String::new(), Vec::new());
        MemTree {
            dirs,
            resolution: Duration::from_nanos(1),
        }
    }

    /// Define a resolução de mtime das entradas adicionadas depois.
    pub fn with_resolution(mut self, r: Duration) -> Self {
        self.resolution = r;
        self
    }

    fn split(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("", path),
        }
    }

    fn ensure_dir(&mut self, path: &str) {
        if path.is_empty() || self.dirs.contains_key(path) {
            return;
        }
        let (parent, name) = Self::split(path);
        let parent = parent.to_string();
        let name = name.to_string();
        self.ensure_dir(&parent);
        let e = Entry::dir(&name, self.resolution);
        self.put(&parent, e);
        self.dirs.insert(path.to_string(), Vec::new());
    }

    fn put(&mut self, dir: &str, e: Entry) {
        let list = self.dirs.entry(dir.to_string()).or_default();
        if let Some(slot) = list.iter_mut().find(|x| x.name == e.name) {
            *slot = e;
        } else {
            list.push(e);
        }
    }

    /// Insere uma entrada sem validar o nome (simula servidor malicioso).
    pub fn add_raw(&mut self, dir: &str, e: Entry) -> &mut Self {
        self.ensure_dir(dir);
        self.put(dir, e);
        self
    }

    pub fn add_dir(&mut self, path: &str) -> &mut Self {
        self.ensure_dir(path);
        self
    }

    pub fn add_file(&mut self, path: &str, size: u64, mtime_secs: Option<u64>) -> &mut Self {
        let (parent, name) = Self::split(path);
        self.ensure_dir(parent);
        let e = Entry {
            name: name.to_string(),
            kind: EntryKind::File,
            size,
            mtime: mtime_secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
            mtime_resolution: self.resolution,
            perms: Some(0o644),
        };
        self.put(parent, e);
        self
    }

    pub fn add_symlink(&mut self, path: &str) -> &mut Self {
        let (parent, name) = Self::split(path);
        self.ensure_dir(parent);
        let mut e = Entry::dir(name, self.resolution);
        e.kind = EntryKind::Symlink;
        self.put(parent, e);
        self
    }

    pub fn remove(&mut self, path: &str) -> &mut Self {
        let (parent, name) = Self::split(path);
        if let Some(l) = self.dirs.get_mut(parent) {
            l.retain(|e| e.name != name);
        }
        let prefix = format!("{path}/");
        self.dirs
            .retain(|k, _| k != path && !k.starts_with(&prefix));
        self
    }
}

impl Entry {
    fn dir(name: &str, res: Duration) -> Entry {
        Entry {
            name: name.to_string(),
            kind: EntryKind::Dir,
            size: 0,
            mtime: None,
            mtime_resolution: res,
            perms: Some(0o755),
        }
    }
}

impl FileTree for MemTree {
    async fn list_dir(&self, path: &RelPath) -> Result<Vec<Entry>, TreeError> {
        if let Some(l) = self.dirs.get(path.as_str()) {
            return Ok(l.clone());
        }
        match self.stat(path).await? {
            Some(_) => Err(TreeError::NotADirectory(path.to_string())),
            None => Err(TreeError::NotFound(path.to_string())),
        }
    }

    async fn stat(&self, path: &RelPath) -> Result<Option<Entry>, TreeError> {
        if path.is_root() {
            return Ok(Some(Entry::dir("", self.resolution)));
        }
        let (parent, name) = Self::split(path.as_str());
        Ok(self
            .dirs
            .get(parent)
            .and_then(|l| l.iter().find(|e| e.name == name))
            .cloned())
    }
}
