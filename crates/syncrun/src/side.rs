//! Árvores, hashers e caminhos confinados às raízes (local e remoto).

use crate::SyncError;
use myssh_ssh::{Connection, Sftp};
use myssh_sync::{Entry, EntryKind, FileTree, LocalTree, RelPath, TreeError};
use myssh_transfer::RemoteTree;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncReadExt;

/// Árvore local ou remota atrás de um único tipo (evita genéricos por direção).
pub(crate) enum AnyTree {
    Local(LocalTree),
    Remote(RemoteTree),
}

impl FileTree for AnyTree {
    async fn list_dir(&self, path: &RelPath) -> Result<Vec<Entry>, TreeError> {
        match self {
            AnyTree::Local(t) => t.list_dir(path).await,
            AnyTree::Remote(t) => t.list_dir(path).await,
        }
    }

    async fn stat(&self, path: &RelPath) -> Result<Option<Entry>, TreeError> {
        match self {
            AnyTree::Local(t) => t.stat(path).await,
            AnyTree::Remote(t) => t.stat(path).await,
        }
    }
}

/// Caminho local `root/rel`, recusando qualquer componente que não seja um nome simples.
pub(crate) fn local_path(root: &Path, rel: &RelPath) -> Result<PathBuf, SyncError> {
    let mut p = root.to_path_buf();
    for c in rel.components() {
        let mut it = Path::new(c).components();
        let ok = matches!(it.next(), Some(Component::Normal(_))) && it.next().is_none();
        if !ok || RelPath::validate_name(c).is_err() {
            return Err(SyncError::OutsideRoot(rel.to_string()));
        }
        p.push(c);
    }
    if !p.starts_with(root) {
        return Err(SyncError::OutsideRoot(rel.to_string()));
    }
    Ok(p)
}

/// Caminho remoto `root/rel` (POSIX).
pub(crate) fn remote_path(root: &str, rel: &RelPath) -> Result<String, SyncError> {
    let mut p = root.trim_end_matches('/').to_string();
    for c in rel.components() {
        if RelPath::validate_name(c).is_err() {
            return Err(SyncError::OutsideRoot(rel.to_string()));
        }
        p.push('/');
        p.push_str(c);
    }
    Ok(if p.is_empty() { "/".into() } else { p })
}

/// Hash SHA-256 do conteúdo de um lado.
///
/// Custo: o lado remoto lê o arquivo inteiro pelo SFTP (nada é executado no servidor), ou seja,
/// o tráfego é do tamanho dos arquivos comparados.
pub(crate) enum Hasher {
    Local(PathBuf),
    Remote(Sftp, String),
}

const CHUNK: usize = 256 * 1024;

async fn digest<R: tokio::io::AsyncRead + Unpin>(mut r: R) -> Result<Vec<u8>, TreeError> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = r
            .read(&mut buf)
            .await
            .map_err(|e| TreeError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_vec())
}

impl Hasher {
    pub(crate) async fn open(
        conn: &Arc<Connection>,
        remote: bool,
        local_root: &Path,
        remote_root: &str,
    ) -> Result<Hasher, SyncError> {
        if remote {
            let sftp = conn
                .open_sftp()
                .await
                .map_err(|m| SyncError::Io(format!("SFTP: {m}")))?;
            Ok(Hasher::Remote(sftp, remote_root.to_string()))
        } else {
            Ok(Hasher::Local(local_root.to_path_buf()))
        }
    }

    pub(crate) async fn hash(&self, rel: &RelPath) -> Result<Vec<u8>, TreeError> {
        match self {
            Hasher::Local(root) => {
                let p = local_path(root, rel).map_err(|e| TreeError::Io(e.to_string()))?;
                // Não segue symlink (verificação antes de abrir; janela de corrida documentada).
                let md = tokio::fs::symlink_metadata(&p)
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                if !md.file_type().is_file() {
                    return Err(TreeError::Io(format!("não é arquivo regular: {rel}")));
                }
                let f = tokio::fs::File::open(&p)
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                digest(f).await
            }
            Hasher::Remote(sftp, root) => {
                let p = remote_path(root, rel).map_err(|e| TreeError::Io(e.to_string()))?;
                let a = sftp
                    .symlink_metadata(p.as_str())
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                if !a.is_regular() {
                    return Err(TreeError::Io(format!("não é arquivo regular: {rel}")));
                }
                let f = sftp
                    .open(p)
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                digest(f).await
            }
        }
    }
}

pub(crate) fn kind_name(k: EntryKind) -> &'static str {
    match k {
        EntryKind::File => "arquivo",
        EntryKind::Dir => "diretório",
        EntryKind::Symlink => "link simbólico",
        EntryKind::Other => "arquivo especial",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminhos_ficam_dentro_da_raiz() {
        let root = Path::new("/tmp/raiz");
        let r = RelPath::parse("a b/ção.txt").unwrap();
        assert_eq!(
            local_path(root, &r).unwrap(),
            root.join("a b").join("ção.txt")
        );
        assert_eq!(local_path(root, &RelPath::root()).unwrap(), root);
        assert_eq!(remote_path("/srv/x/", &r).unwrap(), "/srv/x/a b/ção.txt");
        assert_eq!(remote_path("/", &RelPath::root()).unwrap(), "/");
        // RelPath já rejeita traversal na construção.
        for bad in ["../x", "a/../b", "a\\b", "/etc"] {
            assert!(RelPath::parse(bad).is_err(), "{bad}");
        }
    }
}
