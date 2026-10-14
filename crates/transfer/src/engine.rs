//! Motor de cópia de UM arquivo (serve upload e download): temporário + rename, retomada
//! validada, verificação de tamanho e detecção de origem alterada.

use crate::Kind;
use crate::error::TransferError;
use crate::names;
use crate::side::{Meta, Side};
use crate::sidecar::{RESUME_MIN_SIZE, Sidecar, TAIL_CHECK};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Tamanho do bloco de leitura. O arquivo SFTP o fatia em pacotes de 32 KiB e mantém até 16
/// requisições em voo (pipelining); na leitura, 16 blocos de ~255 KiB.
pub const CHUNK: usize = 256 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Contadores globais do gerenciador (diagnóstico e testes).
#[derive(Debug, Default)]
pub struct Stats {
    pub bytes_transferred: AtomicU64,
    pub files_resumed: AtomicU64,
    pub bytes_resumed: AtomicU64,
}

/// Contadores de progresso de um job.
#[derive(Debug, Default)]
pub struct Meter {
    /// Bytes considerados concluídos (inclui retomados e arquivos pulados).
    pub credited: AtomicU64,
    /// Bytes realmente movidos agora (base da taxa).
    pub transferred: AtomicU64,
}

pub struct CopyArgs<'a> {
    pub src: Side<'a>,
    pub dst: Side<'a>,
    pub src_path: &'a str,
    pub dst_path: &'a str,
    pub preserve_mtime: bool,
    pub preserve_perms: bool,
    pub cancel: &'a CancellationToken,
    pub meter: &'a Meter,
    pub stats: &'a Stats,
    /// Conexão: se estiver encerrada, qualquer falha é tratada como perda de conexão.
    pub conn: &'a myssh_ssh::Connection,
    /// Bytes já creditados a este arquivo (o chamador completa o restante ao terminar).
    pub contrib: &'a AtomicU64,
}

impl CopyArgs<'_> {
    fn credit(&self, n: u64) {
        self.contrib.fetch_add(n, Ordering::Relaxed);
        self.meter.credited.fetch_add(n, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Cleanup {
    temp: Option<String>,
    sidecar: Option<String>,
}

/// Token aleatório curto (hex) para nomes de temporário. Não é criptográfico: a unicidade vem de
/// `create_new`/`EXCLUDE` na criação.
pub fn rand_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(N.fetch_add(1, Ordering::Relaxed));
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:016x}", h.finish())
}

/// Erros "brandos" (leitura/escrita do sidecar e limpeza) só interrompem se a conexão caiu.
fn soft<T>(r: Result<T, TransferError>) -> Result<Option<T>, TransferError> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_connection_lost() => Err(e),
        Err(_) => Ok(None),
    }
}

async fn tails_match(
    src: &Side<'_>,
    src_path: &str,
    dst: &Side<'_>,
    dst_path: &str,
    offset: u64,
) -> Result<bool, TransferError> {
    let n = offset.min(TAIL_CHECK);
    let start = offset - n;
    let a = src.read_range(src_path, start, n as usize).await?;
    let b = dst.read_range(dst_path, start, n as usize).await?;
    Ok(a.len() as u64 == n && a == b)
}

/// Em falha genérica do servidor (`Failure`, que é como o OpenSSH reporta ENOSPC), consulta o
/// espaço livre para dar a mensagem certa.
async fn refine(dst: &Side<'_>, parent: &str, remaining: u64, e: TransferError) -> TransferError {
    if matches!(e, TransferError::Remote(_))
        && dst.is_remote()
        && let Some(avail) = dst.free_space(parent).await
        && avail < remaining
    {
        return TransferError::NoSpace;
    }
    e
}

/// Copia `src_path` para `dst_path` (nome final). Devolve um aviso não fatal (ex.: não foi
/// possível preservar data/permissões) ou o erro. Em erro que não seja perda de conexão, o
/// temporário e o sidecar são removidos; na perda de conexão ficam para uma retomada.
pub async fn copy_file(a: CopyArgs<'_>) -> Result<Option<TransferError>, TransferError> {
    let mut st = Cleanup::default();
    match copy_inner(&a, &mut st).await {
        Ok(w) => Ok(w),
        Err(mut e) => {
            if e != TransferError::Cancelled && a.conn.is_closed() {
                e = TransferError::ConnectionLost;
            }
            if !e.is_connection_lost() {
                for p in [st.temp.as_deref(), st.sidecar.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    let _ = tokio::time::timeout(CLEANUP_TIMEOUT, a.dst.remove_file(p)).await;
                }
            }
            Err(e)
        }
    }
}

async fn copy_inner(
    a: &CopyArgs<'_>,
    st: &mut Cleanup,
) -> Result<Option<TransferError>, TransferError> {
    let (src, dst) = (&a.src, &a.dst);
    let cancelled = || a.cancel.is_cancelled();
    if cancelled() {
        return Err(TransferError::Cancelled);
    }
    let src_meta: Meta = src
        .lstat(a.src_path)
        .await?
        .ok_or(TransferError::SourceChanged)?;
    if src_meta.kind != Kind::File {
        return Err(TransferError::SourceChanged);
    }
    let size = src_meta.size;
    let parent = dst.parent(a.dst_path);
    let fname = dst.file_name(a.dst_path);

    // --- retomada validada
    let mut offset = 0u64;
    let mut temp_path: Option<String> = None;
    if size >= RESUME_MIN_SIZE {
        let sc_path = dst.join(&parent, &names::sidecar_name(&fname));
        st.sidecar = Some(sc_path.clone());
        if let Some(Some(bytes)) = soft(dst.read_small(&sc_path).await)?
            && let Some(sc) = Sidecar::parse(&bytes)
            && sc.name == fname
            && names::is_part_name_for(&fname, &sc.temp)
        {
            let tp = dst.join(&parent, &sc.temp);
            let mut resume_len = None;
            if sc.matches(&fname, size, src_meta.mtime) {
                let tm = soft(dst.lstat(&tp).await)?.flatten();
                if let Some(m) = tm
                    && m.kind == Kind::File
                    && m.size > 0
                    && m.size <= size
                    && soft(tails_match(src, a.src_path, dst, &tp, m.size).await)?.unwrap_or(false)
                {
                    resume_len = Some(m.size);
                }
            }
            match resume_len {
                Some(len) => {
                    offset = len;
                    temp_path = Some(tp);
                }
                None => {
                    // Origem ou parcial incompatíveis: descarta o parcial e recomeça do zero.
                    let _ = soft(dst.remove_file(&tp).await)?;
                }
            }
        }
    }

    let resumed = temp_path.is_some();
    let tp = match temp_path {
        Some(p) => p,
        None => {
            let tname = names::part_name(&fname, &rand_token());
            let tp = dst.join(&parent, &tname);
            if size >= RESUME_MIN_SIZE {
                let sc = Sidecar::new(&fname, &tname, size, src_meta.mtime);
                let sc_path = dst.join(&parent, &names::sidecar_name(&fname));
                let _ = soft(dst.write_small(&sc_path, &sc.to_bytes()).await)?;
                st.sidecar = Some(sc_path);
            }
            tp
        }
    };
    if let Some(avail) = dst.free_space(&parent).await
        && avail < size - offset
    {
        return Err(TransferError::NoSpace);
    }
    st.temp = Some(tp.clone());

    let mut reader = src.open_read(a.src_path).await?;
    let mut writer = dst.open_write(&tp, resumed).await?;
    if resumed {
        reader.seek(offset).await?;
        writer.seek(offset).await?;
        a.credit(offset);
        a.stats.files_resumed.fetch_add(1, Ordering::Relaxed);
        a.stats.bytes_resumed.fetch_add(offset, Ordering::Relaxed);
    }

    // --- cópia em blocos
    let mut buf = vec![0u8; CHUNK];
    let mut pos = offset;
    loop {
        let n = tokio::select! {
            biased;
            _ = a.cancel.cancelled() => return Err(TransferError::Cancelled),
            r = reader.read(&mut buf) => r?,
        };
        if n == 0 {
            break;
        }
        let w = tokio::select! {
            biased;
            _ = a.cancel.cancelled() => return Err(TransferError::Cancelled),
            r = writer.write_all(&buf[..n]) => r,
        };
        if let Err(e) = w {
            let te = TransferError::from_io(&e);
            return Err(refine(dst, &parent, size.saturating_sub(pos), te).await);
        }
        pos += n as u64;
        a.credit(n as u64);
        a.meter.transferred.fetch_add(n as u64, Ordering::Relaxed);
        a.stats
            .bytes_transferred
            .fetch_add(n as u64, Ordering::Relaxed);
    }
    let fin = tokio::select! {
        biased;
        _ = a.cancel.cancelled() => return Err(TransferError::Cancelled),
        r = writer.finish() => r,
    };
    if let Err(e) = fin {
        return Err(refine(dst, &parent, size.saturating_sub(pos), e).await);
    }
    drop(reader);

    // --- origem alterada durante a transferência?
    let after = src.lstat(a.src_path).await?;
    let same = after
        .as_ref()
        .is_some_and(|m| m.kind == Kind::File && m.size == size && m.mtime == src_meta.mtime);
    if !same || pos != size {
        return Err(TransferError::SourceChanged);
    }

    // --- tamanho final antes de publicar
    let tm = dst.lstat(&tp).await?.ok_or(TransferError::NotFound)?;
    if tm.size != size {
        return Err(TransferError::SizeMismatch {
            expected: size,
            actual: tm.size,
        });
    }

    // --- mtime / permissões (nunca setuid/setgid/sticky: máscara 0o777)
    let mtime = a.preserve_mtime.then_some(src_meta.mtime).flatten();
    let perms = a
        .preserve_perms
        .then_some(src_meta.perms)
        .flatten()
        .map(|p| p & 0o777);
    let mut warning = None;
    if let Err(e) = dst.set_times_perms(&tp, mtime, perms).await {
        if e.is_connection_lost() {
            return Err(e);
        }
        warning = Some(e);
    }

    // --- publica
    if cancelled() {
        return Err(TransferError::Cancelled);
    }
    dst.publish(&tp, a.dst_path, &rand_token()).await?;
    st.temp = None;
    if let Some(sc) = st.sidecar.take() {
        let _ = dst.remove_file(&sc).await;
    }
    Ok(warning)
}
