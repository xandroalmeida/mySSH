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

/// Hash de no máximo `size` bytes (o tamanho declarado pelo `lstat`). Se a origem continuar
/// entregando dados além disso (servidor hostil ou arquivo crescendo), aborta em vez de ler sem
/// fim; a leitura extra é de 1 byte, só para detectar.
async fn digest<R: tokio::io::AsyncRead + Unpin>(
    mut r: R,
    size: u64,
) -> Result<Vec<u8>, TreeError> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut left = size;
    while left > 0 {
        let take = (buf.len() as u64).min(left) as usize;
        let n = r
            .read(&mut buf[..take])
            .await
            .map_err(|e| TreeError::Io(e.to_string()))?;
        if n == 0 {
            return Ok(h.finalize().to_vec());
        }
        h.update(&buf[..n]);
        left -= n as u64;
    }
    let mut probe = [0u8; 1];
    let extra = r
        .read(&mut probe)
        .await
        .map_err(|e| TreeError::Io(e.to_string()))?;
    if extra > 0 {
        return Err(TreeError::Io(
            myssh_i18n::tr!(
                "origem alterada durante a leitura (mais dados que o declarado)",
                "source changed while reading (more data than declared)",
                "origen modificado durante la lectura (más datos de los declarados)"
            )
            .into(),
        ));
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
                    return Err(TreeError::Io(myssh_i18n::tr!(
                        format!("não é arquivo regular: {rel}"),
                        format!("not a regular file: {rel}"),
                        format!("no es un archivo regular: {rel}")
                    )));
                }
                let f = tokio::fs::File::open(&p)
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                digest(f, md.len()).await
            }
            Hasher::Remote(sftp, root) => {
                let p = remote_path(root, rel).map_err(|e| TreeError::Io(e.to_string()))?;
                let a = sftp
                    .symlink_metadata(p.as_str())
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                if !a.is_regular() {
                    return Err(TreeError::Io(myssh_i18n::tr!(
                        format!("não é arquivo regular: {rel}"),
                        format!("not a regular file: {rel}"),
                        format!("no es un archivo regular: {rel}")
                    )));
                }
                let f = sftp
                    .open(p)
                    .await
                    .map_err(|e| TreeError::Io(e.to_string()))?;
                digest(f, a.size.unwrap_or(0)).await
            }
        }
    }
}

pub(crate) fn kind_name(k: EntryKind) -> &'static str {
    match k {
        EntryKind::File => myssh_i18n::tr!("arquivo", "a file", "un archivo"),
        EntryKind::Dir => myssh_i18n::tr!("diretório", "a directory", "un directorio"),
        EntryKind::Symlink => {
            myssh_i18n::tr!("link simbólico", "a symbolic link", "un enlace simbólico")
        }
        EntryKind::Other => {
            myssh_i18n::tr!("arquivo especial", "a special file", "un archivo especial")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Leitor que nunca termina (servidor que declarou pouco no `lstat`).
    struct Endless(u64);
    impl tokio::io::AsyncRead for Endless {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let n = buf.remaining();
            buf.put_slice(&vec![9u8; n]);
            self.0 += n as u64;
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn digest_respeita_o_tamanho_declarado() {
        // Mentiroso: aborta, lendo no máximo size + 1 byte.
        for size in [0u64, 1, 10, CHUNK as u64 + 7] {
            let mut r = Endless(0);
            let e = digest(&mut r, size).await.unwrap_err();
            assert!(e.to_string().contains("alterada"), "{e}");
            assert!(r.0 <= size + CHUNK as u64, "leu {} para size {size}", r.0);
        }
        // Honesto: mesmo hash que o SHA-256 direto; arquivo que encolheu: hash do que veio.
        let data = vec![5u8; 1000];
        let ok = digest(&data[..], 1000).await.unwrap();
        assert_eq!(ok, Sha256::digest(&data).to_vec());
        let menor = digest(&data[..], 5000).await.unwrap();
        assert_eq!(menor, ok);
        // Maior que o declarado (por 1 byte): erro.
        assert!(digest(&data[..], 999).await.is_err());
    }

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
