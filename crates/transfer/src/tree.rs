//! `FileTree` do remoto para a sincronização (M4): `lstat` (não segue symlinks), mtime com
//! resolução de 1 s (SFTP v3 só tem segundos).

use crate::error::TransferError;
use crate::side::{is_no_such_file, meta_from_attrs};
use crate::{Kind, names};
use myssh_ssh::{Connection, Sftp};
use myssh_sync::{Entry, EntryKind, FileTree, RelPath, TreeError};
use std::sync::Arc;
use std::time::Duration;

/// Árvore remota sob uma raiz (caminho absoluto no servidor). Usa um canal SFTP próprio.
pub struct RemoteTree {
    sftp: Sftp,
    root: String,
}

fn kind(k: Kind) -> EntryKind {
    match k {
        Kind::File => EntryKind::File,
        Kind::Dir => EntryKind::Dir,
        Kind::Symlink => EntryKind::Symlink,
        Kind::Other => EntryKind::Other,
    }
}

fn tree_err(e: russh_sftp::client::error::Error, what: &str) -> TreeError {
    match TransferError::from_sftp(&e) {
        TransferError::NotFound => TreeError::NotFound(what.to_string()),
        other => TreeError::Io(other.to_string()),
    }
}

impl RemoteTree {
    pub async fn open(
        conn: Arc<Connection>,
        root: impl Into<String>,
    ) -> Result<Self, TransferError> {
        Ok(RemoteTree {
            sftp: crate::browser::open_sftp(&conn).await?,
            root: root.into(),
        })
    }

    fn remote_path(&self, rel: &RelPath) -> String {
        let mut p = self.root.trim_end_matches('/').to_string();
        for c in rel.components() {
            p.push('/');
            p.push_str(c);
        }
        if p.is_empty() { "/".into() } else { p }
    }
}

const RES: Duration = Duration::from_secs(1);

impl FileTree for RemoteTree {
    async fn list_dir(&self, path: &RelPath) -> Result<Vec<Entry>, TreeError> {
        let dir = self.remote_path(path);
        let rd = match self.sftp.read_dir(dir.as_str()).await {
            Ok(rd) => rd,
            Err(e) if is_no_such_file(&e) => return Err(TreeError::NotFound(path.to_string())),
            Err(e) => return Err(tree_err(e, path.as_str())),
        };
        // Nomes inválidos (traversal etc.) são repassados como `Other`; o planejador os rejeita
        // via `RelPath::join`.
        let mut out: Vec<Entry> = rd
            .map(|e| {
                let name = e.file_name();
                let m = meta_from_attrs(&e.metadata());
                let valid = names::validate_remote_name(&name).is_ok();
                Entry {
                    name,
                    kind: if valid {
                        kind(m.kind)
                    } else {
                        EntryKind::Other
                    },
                    size: if m.kind == Kind::File { m.size } else { 0 },
                    mtime: m.mtime,
                    mtime_resolution: RES,
                    perms: m.perms,
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn stat(&self, path: &RelPath) -> Result<Option<Entry>, TreeError> {
        let p = self.remote_path(path);
        match self.sftp.symlink_metadata(p.as_str()).await {
            Ok(a) => {
                let m = meta_from_attrs(&a);
                Ok(Some(Entry {
                    name: path.name().unwrap_or("").to_string(),
                    kind: kind(m.kind),
                    size: if m.kind == Kind::File { m.size } else { 0 },
                    mtime: m.mtime,
                    mtime_resolution: RES,
                    perms: m.perms,
                }))
            }
            Err(e) if is_no_such_file(&e) => Ok(None),
            Err(e) => Err(tree_err(e, path.as_str())),
        }
    }
}
