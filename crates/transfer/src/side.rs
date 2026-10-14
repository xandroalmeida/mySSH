//! Um lado da transferência (disco local ou SFTP remoto) atrás de uma interface única,
//! para que upload e download compartilhem o mesmo motor de cópia.

use crate::error::TransferError;
use crate::names;
use crate::{Kind, RemoteEntry};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::File as SftpFile;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use std::io::{self, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Metadados sem seguir symlinks (`lstat`).
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub kind: Kind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    /// Bits de permissão (`& 0o7777`), quando conhecidos.
    pub perms: Option<u32>,
}

pub fn secs_to_time(secs: u32) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs as u64)
}

pub fn time_to_secs(t: SystemTime) -> u32 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

pub fn meta_from_attrs(a: &FileAttributes) -> Meta {
    let kind = if a.is_dir() {
        Kind::Dir
    } else if a.is_symlink() {
        Kind::Symlink
    } else if a.is_regular() {
        Kind::File
    } else {
        Kind::Other
    };
    Meta {
        kind,
        size: a.size.unwrap_or(0),
        mtime: a.mtime.map(secs_to_time),
        perms: a.permissions.map(|p| p & 0o7777),
    }
}

pub fn entry_from_attrs(dir: &str, name: &str, a: &FileAttributes) -> RemoteEntry {
    let m = meta_from_attrs(a);
    RemoteEntry {
        name: name.to_string(),
        path: names::remote_join(dir, name),
        kind: m.kind,
        size: if m.kind == Kind::File { m.size } else { 0 },
        mtime: m.mtime,
        perms: m.perms,
    }
}

pub fn is_no_such_file(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile)
}

pub fn sftp_err(e: SftpError) -> TransferError {
    TransferError::from_sftp(&e)
}

fn io_err(e: io::Error) -> TransferError {
    TransferError::from_io(&e)
}

fn local_meta(md: &std::fs::Metadata) -> Meta {
    let ft = md.file_type();
    let kind = if ft.is_symlink() {
        Kind::Symlink
    } else if ft.is_dir() {
        Kind::Dir
    } else if ft.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    #[cfg(unix)]
    let perms = {
        use std::os::unix::fs::PermissionsExt;
        Some(md.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let perms = None;
    Meta {
        kind,
        size: if kind == Kind::File { md.len() } else { 0 },
        mtime: md.modified().ok(),
        perms,
    }
}

/// Lê um sidecar local: ausente → `None`; symlink, não regular ou grande demais → `None`
/// (recusado); nunca abre através de symlink (`O_NOFOLLOW`).
async fn read_small_local(path: &str) -> Result<Option<Vec<u8>>, TransferError> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(md) if md.file_type().is_file() && md.len() <= 4096 => {}
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(e)),
    }
    let mut o = tokio::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    o.custom_flags(libc::O_NOFOLLOW);
    match o.open(path).await {
        Ok(mut f) => {
            let mut b = Vec::new();
            f.read_to_end(&mut b).await.map_err(io_err)?;
            Ok(Some(b))
        }
        // Trocado por um symlink entre o lstat e o open (ELOOP): recusa.
        #[cfg(unix)]
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}

/// Grava um sidecar local: temporário novo (`create_new`, 0600, nome `.…myssh-part-…`, que a
/// varredura ignora) + `rename` sobre o destino. O rename substitui o nome: um symlink plantado
/// no lugar do sidecar é trocado, nunca seguido.
async fn write_small_local(path: &str, data: &[u8]) -> Result<(), TransferError> {
    let tmp = format!(
        "{path}{}{}",
        crate::names::PART_MARK,
        crate::engine::rand_token()
    );
    let mut o = tokio::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    o.mode(0o600);
    let r: Result<(), TransferError> = async {
        let mut f = o.open(&tmp).await.map_err(io_err)?;
        f.write_all(data).await.map_err(io_err)?;
        f.flush().await.map_err(io_err)?;
        drop(f);
        tokio::fs::rename(&tmp, path).await.map_err(io_err)
    }
    .await;
    if r.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    r
}

/// Modo `0666 & !umask` do usuário (sondado criando um arquivo vazio no diretório), usado ao
/// publicar um temporário local `0600` quando `preserve_perms` está desligado.
#[cfg(unix)]
async fn user_default_mode(dir: &str) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    static MODE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    if let Some(m) = MODE.get() {
        return Some(*m);
    }
    let probe = format!(
        "{}/.myssh-umask{}{}",
        dir.trim_end_matches('/'),
        crate::names::PART_MARK,
        crate::engine::rand_token()
    );
    let mut o = tokio::fs::OpenOptions::new();
    o.write(true).create_new(true).mode(0o666);
    let f = o.open(&probe).await.ok()?;
    let mode = f
        .metadata()
        .await
        .ok()
        .map(|m| m.permissions().mode() & 0o777);
    drop(f);
    let _ = tokio::fs::remove_file(&probe).await;
    let mode = mode?;
    Some(*MODE.get_or_init(|| mode))
}

/// Lado de uma transferência.
#[derive(Clone, Copy)]
pub enum Side<'a> {
    Local,
    Remote(&'a SftpSession),
}

pub enum Reader {
    Local(tokio::fs::File),
    Remote(SftpFile),
}

impl Reader {
    pub async fn seek(&mut self, offset: u64) -> Result<(), TransferError> {
        let r = match self {
            Reader::Local(f) => f.seek(SeekFrom::Start(offset)).await,
            Reader::Remote(f) => f.seek(SeekFrom::Start(offset)).await,
        };
        r.map(|_| ()).map_err(io_err)
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransferError> {
        let r = match self {
            Reader::Local(f) => f.read(buf).await,
            Reader::Remote(f) => f.read(buf).await,
        };
        r.map_err(io_err)
    }
}

pub enum Writer {
    Local(tokio::fs::File),
    Remote(SftpFile),
}

impl Writer {
    pub async fn seek(&mut self, offset: u64) -> Result<(), TransferError> {
        let r = match self {
            Writer::Local(f) => f.seek(SeekFrom::Start(offset)).await,
            Writer::Remote(f) => f.seek(SeekFrom::Start(offset)).await,
        };
        r.map(|_| ()).map_err(io_err)
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        match self {
            Writer::Local(f) => f.write_all(buf).await,
            Writer::Remote(f) => f.write_all(buf).await,
        }
    }

    /// Descarrega e fecha. Local: `fsync` dos dados; remoto: aguarda todas as confirmações.
    pub async fn finish(self) -> Result<(), TransferError> {
        match self {
            Writer::Local(mut f) => {
                f.flush().await.map_err(io_err)?;
                f.sync_data().await.map_err(io_err)
            }
            Writer::Remote(mut f) => f.shutdown().await.map_err(io_err),
        }
    }
}

impl<'a> Side<'a> {
    pub fn is_remote(&self) -> bool {
        matches!(self, Side::Remote(_))
    }

    pub fn join(&self, parent: &str, name: &str) -> String {
        match self {
            Side::Local => {
                let mut p = PathBuf::from(parent);
                p.push(name);
                p.to_string_lossy().into_owned()
            }
            Side::Remote(_) => names::remote_join(parent, name),
        }
    }

    pub fn parent(&self, path: &str) -> String {
        match self {
            Side::Local => Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ".".into()),
            Side::Remote(_) => names::remote_dirname(path),
        }
    }

    pub fn file_name(&self, path: &str) -> String {
        match self {
            Side::Local => Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Side::Remote(_) => names::remote_basename(path).unwrap_or("").to_string(),
        }
    }

    /// `lstat`; `None` se não existir.
    pub async fn lstat(&self, path: &str) -> Result<Option<Meta>, TransferError> {
        match self {
            Side::Local => match tokio::fs::symlink_metadata(path).await {
                Ok(md) => Ok(Some(local_meta(&md))),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    Ok(None)
                }
                Err(e) => Err(io_err(e)),
            },
            Side::Remote(s) => match s.symlink_metadata(path).await {
                Ok(a) => Ok(Some(meta_from_attrs(&a))),
                Err(e) if is_no_such_file(&e) => Ok(None),
                Err(e) => Err(sftp_err(e)),
            },
        }
    }

    /// Cria o diretório se não existir. Devolve `true` se criou, `false` se já era diretório
    /// (não segue symlink: um link no lugar do diretório é erro).
    pub async fn ensure_dir(&self, path: &str) -> Result<bool, TransferError> {
        if let Some(m) = self.lstat(path).await? {
            return match m.kind {
                Kind::Dir => Ok(false),
                Kind::Symlink => Err(TransferError::DestinationKind("um link simbólico")),
                _ => Err(TransferError::DestinationKind("um arquivo")),
            };
        }
        let r = match self {
            Side::Local => tokio::fs::create_dir(path).await.map_err(io_err),
            Side::Remote(s) => s.create_dir(path).await.map_err(sftp_err),
        };
        match r {
            Ok(()) => Ok(true),
            Err(e) => {
                // Corrida: criado por outro (ou "Failure" genérico de "já existe").
                if let Some(m) = self.lstat(path).await.ok().flatten()
                    && m.kind == Kind::Dir
                {
                    return Ok(false);
                }
                Err(e)
            }
        }
    }

    pub async fn remove_file(&self, path: &str) -> Result<(), TransferError> {
        match self {
            Side::Local => tokio::fs::remove_file(path).await.map_err(io_err),
            Side::Remote(s) => s.remove_file(path).await.map_err(sftp_err),
        }
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), TransferError> {
        match self {
            Side::Local => tokio::fs::rename(from, to).await.map_err(io_err),
            Side::Remote(s) => s.rename(from, to).await.map_err(sftp_err),
        }
    }

    /// Publica `temp` como `dest` (substituindo um arquivo existente).
    ///
    /// Local: `rename` do SO (atômico, substitui). Remoto: o `SSH_FXP_RENAME` padrão falha se o
    /// destino existe e esta biblioteca não expõe `posix-rename@openssh.com`; então, havendo
    /// destino, ele é movido para um backup, o temporário assume o nome final e o backup é
    /// removido (restaurado se o segundo rename falhar). **Não é atômico** no remoto: há uma
    /// janela curta sem o arquivo no nome final (o conteúdo antigo continua no backup).
    pub async fn publish(&self, temp: &str, dest: &str, rand: &str) -> Result<(), TransferError> {
        match self.rename(temp, dest).await {
            Ok(()) => Ok(()),
            Err(first) => {
                if !self.is_remote() {
                    return Err(first);
                }
                match self.lstat(dest).await {
                    Ok(Some(m)) if m.kind != Kind::Dir => {}
                    _ => return Err(first),
                }
                let parent = self.parent(dest);
                let backup = self.join(
                    &parent,
                    &format!(".{}.myssh-old-{rand}", self.file_name(dest)),
                );
                self.rename(dest, &backup).await?;
                match self.rename(temp, dest).await {
                    Ok(()) => {
                        let _ = self.remove_file(&backup).await;
                        Ok(())
                    }
                    Err(e) => {
                        let _ = self.rename(&backup, dest).await;
                        Err(e)
                    }
                }
            }
        }
    }

    pub async fn open_read(&self, path: &str) -> Result<Reader, TransferError> {
        match self {
            Side::Local => tokio::fs::File::open(path)
                .await
                .map(Reader::Local)
                .map_err(io_err),
            Side::Remote(s) => s.open(path).await.map(Reader::Remote).map_err(sftp_err),
        }
    }

    /// Abre o temporário para escrita: novo (`create_new`) ou existente (retomada, sem truncar).
    pub async fn open_write(&self, path: &str, existing: bool) -> Result<Writer, TransferError> {
        match self {
            Side::Local => {
                let mut o = tokio::fs::OpenOptions::new();
                o.write(true);
                if existing {
                    o.create(false);
                } else {
                    o.create_new(true);
                }
                // Temporário só do usuário (0600) enquanto incompleto; o modo final é aplicado
                // ao publicar (`Side::default_final_mode`). Nunca segue symlink no último
                // componente.
                #[cfg(unix)]
                {
                    o.mode(0o600);
                    o.custom_flags(libc::O_NOFOLLOW);
                }
                o.open(path).await.map(Writer::Local).map_err(io_err)
            }
            Side::Remote(s) => {
                let flags = if existing {
                    OpenFlags::WRITE
                } else {
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE
                };
                s.open_with_flags(path, flags)
                    .await
                    .map(Writer::Remote)
                    .map_err(sftp_err)
            }
        }
    }

    /// Lê `len` bytes a partir de `start` (para verificar o trecho final de um temporário).
    pub async fn read_range(
        &self,
        path: &str,
        start: u64,
        len: usize,
    ) -> Result<Vec<u8>, TransferError> {
        let mut r = self.open_read(path).await?;
        r.seek(start).await?;
        let mut out = vec![0u8; len];
        let mut got = 0;
        while got < len {
            let n = r.read(&mut out[got..]).await?;
            if n == 0 {
                break;
            }
            got += n;
        }
        out.truncate(got);
        Ok(out)
    }

    pub async fn read_small(&self, path: &str) -> Result<Option<Vec<u8>>, TransferError> {
        match self {
            Side::Local => read_small_local(path).await,
            Side::Remote(s) => {
                // Evita ler um "sidecar" gigante: confere o tamanho antes.
                match s.symlink_metadata(path).await {
                    Ok(a) if a.size.unwrap_or(0) <= 4096 && a.is_regular() => {}
                    Ok(_) => return Ok(None),
                    Err(e) if is_no_such_file(&e) => return Ok(None),
                    Err(e) => return Err(sftp_err(e)),
                }
                match s.read(path).await {
                    Ok(b) => Ok(Some(b)),
                    Err(e) if is_no_such_file(&e) => Ok(None),
                    Err(e) => Err(sftp_err(e)),
                }
            }
        }
    }

    pub async fn write_small(&self, path: &str, data: &[u8]) -> Result<(), TransferError> {
        match self {
            Side::Local => write_small_local(path, data).await,
            Side::Remote(s) => {
                // Nunca escreve através de um link/arquivo pré-existente: remove o que houver
                // (unlink não segue symlink) e cria com EXCLUDE (O_EXCL também não segue).
                match s.symlink_metadata(path).await {
                    Ok(_) => s.remove_file(path).await.map_err(sftp_err)?,
                    Err(e) if is_no_such_file(&e) => {}
                    Err(e) => return Err(sftp_err(e)),
                }
                let mut f = s
                    .open_with_flags(
                        path,
                        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    )
                    .await
                    .map_err(sftp_err)?;
                f.write_all(data).await.map_err(io_err)?;
                f.shutdown().await.map_err(io_err)
            }
        }
    }

    /// Modo a aplicar ao publicar quando o chamador não pediu `preserve_perms`: no lado local
    /// (unix), o padrão do usuário (`0666 & !umask`), pois o temporário nasce `0600`; no remoto,
    /// `None` (o servidor já aplicou o seu umask na criação).
    pub async fn default_final_mode(&self, dir: &str) -> Option<u32> {
        #[cfg(unix)]
        if matches!(self, Side::Local) {
            return user_default_mode(dir).await;
        }
        let _ = dir;
        None
    }

    /// Aplica mtime (e atime) e, opcionalmente, permissões (já mascaradas a `0o777`).
    pub async fn set_times_perms(
        &self,
        path: &str,
        mtime: Option<SystemTime>,
        perms: Option<u32>,
    ) -> Result<(), TransferError> {
        if mtime.is_none() && perms.is_none() {
            return Ok(());
        }
        match self {
            Side::Local => {
                if let Some(t) = mtime {
                    let ft = filetime::FileTime::from_system_time(t);
                    let p = path.to_string();
                    tokio::task::spawn_blocking(move || filetime::set_file_mtime(&p, ft))
                        .await
                        .map_err(|_| TransferError::Io("tarefa interrompida".into()))?
                        .map_err(io_err)?;
                }
                #[cfg(unix)]
                if let Some(m) = perms {
                    use std::os::unix::fs::PermissionsExt;
                    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(m & 0o777))
                        .await
                        .map_err(io_err)?;
                }
                #[cfg(not(unix))]
                let _ = perms;
                Ok(())
            }
            Side::Remote(s) => {
                let mut a = FileAttributes::empty();
                if let Some(t) = mtime {
                    let secs = time_to_secs(t);
                    a.atime = Some(secs);
                    a.mtime = Some(secs);
                }
                a.permissions = perms.map(|m| m & 0o777);
                s.set_metadata(path, a).await.map_err(sftp_err)
            }
        }
    }

    /// Espaço livre (para usuários comuns) no diretório, se o lado souber informar
    /// (remoto: `statvfs@openssh.com`).
    pub async fn free_space(&self, dir: &str) -> Option<u64> {
        match self {
            Side::Local => None,
            Side::Remote(s) => {
                let v = s.fs_info(dir).await.ok().flatten()?;
                let unit = if v.fragment_size > 0 {
                    v.fragment_size
                } else {
                    v.block_size
                };
                Some(v.blocks_avail.saturating_mul(unit))
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[tokio::test]
    async fn sidecar_nao_segue_symlink_plantado() {
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("vitima.txt");
        std::fs::write(&victim, "intacto").unwrap();
        let sc = d.path().join(".a.myssh-resume");
        symlink(&victim, &sc).unwrap();
        let sc_s = sc.to_str().unwrap();

        // Escrita: troca o link por um arquivo regular, a vítima fica intacta.
        Side::Local.write_small(sc_s, b"{\"v\":1}").await.unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "intacto");
        let md = std::fs::symlink_metadata(&sc).unwrap();
        assert!(md.file_type().is_file(), "o link deve ter sido substituído");
        assert_eq!(mode(&sc), 0o600);
        assert_eq!(
            Side::Local.read_small(sc_s).await.unwrap().unwrap(),
            b"{\"v\":1}"
        );
        // Sem sobras do temporário.
        let n = std::fs::read_dir(d.path()).unwrap().count();
        assert_eq!(n, 2);

        // Leitura: link para dentro ou fora é recusado.
        std::fs::remove_file(&sc).unwrap();
        symlink(&victim, &sc).unwrap();
        assert_eq!(Side::Local.read_small(sc_s).await.unwrap(), None);
        std::fs::remove_file(&sc).unwrap();
        symlink(d.path().join("nao-existe"), &sc).unwrap();
        assert_eq!(Side::Local.read_small(sc_s).await.unwrap(), None);
    }

    #[tokio::test]
    async fn temporario_local_nasce_0600_e_nao_segue_symlink() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join(".f.myssh-part-1");
        let w = Side::Local
            .open_write(t.to_str().unwrap(), false)
            .await
            .unwrap();
        w.finish().await.unwrap();
        assert_eq!(mode(&t), 0o600);

        // Retomada sobre um symlink no lugar do temporário: recusada.
        let victim = d.path().join("vitima");
        std::fs::write(&victim, "x").unwrap();
        let l = d.path().join(".f.myssh-part-2");
        symlink(&victim, &l).unwrap();
        assert!(
            Side::Local
                .open_write(l.to_str().unwrap(), true)
                .await
                .is_err()
        );
        // Criação nova sobre um symlink: recusada (O_EXCL).
        assert!(
            Side::Local
                .open_write(l.to_str().unwrap(), false)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "x");
    }

    #[tokio::test]
    async fn modo_final_padrao_segue_o_umask_do_usuario() {
        let d = tempfile::tempdir().unwrap();
        let m = Side::Local
            .default_final_mode(d.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(m & 0o111, 0, "arquivo comum, sem bits de execução: {m:o}");
        assert_eq!(m & 0o600, 0o600, "dono lê e escreve: {m:o}");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
        assert_eq!(Side::Local.default_final_mode("/nao/existe").await, Some(m));
    }
}
