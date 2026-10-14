//! Testes de integração contra o servidor de teste (`./testenv/up.sh`, 127.0.0.1:2222).
//!
//! ```text
//! MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys \
//!   cargo test -p myssh-transfer -- --ignored --nocapture --test-threads=1
//! ```
//! A host key esperada vem do `ssh-keyscan` (fora do código sob teste); a validação nunca é
//! desabilitada. O sshd aplica `PerSourcePenalties`: `ready()` espera a penalidade expirar.

use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore};
use myssh_ssh::{
    AuthMaterial, ConnectRequest, Connection, EventSink, PtySize, host_key_from_public,
};
use myssh_store::Store;
use myssh_sync::{EntryKind, FileTree, RelPath};
use myssh_transfer::*;
use russh::keys::PublicKey;
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------- infraestrutura

fn target() -> (String, u16) {
    let v = std::env::var("MYSSH_TEST_SSH").expect("defina MYSSH_TEST_SSH=host:porta");
    let (h, p) = v.rsplit_once(':').expect("host:porta");
    (h.to_string(), p.parse().expect("porta"))
}

fn endpoint() -> HostEndpoint {
    let (h, p) = target();
    HostEndpoint::new(&h, p)
}

fn server_key() -> HostKey {
    static KEY: std::sync::OnceLock<HostKey> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        let (h, p) = target();
        let out = Command::new("ssh-keyscan")
            .args(["-p", &p.to_string(), "-t", "ed25519", &h])
            .output()
            .expect("ssh-keyscan");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let line = text
            .lines()
            .find(|l| !l.starts_with('#'))
            .expect("saída do ssh-keyscan");
        let key = line.split_once(' ').map(|x| x.1).expect("linha de chave");
        host_key_from_public(&PublicKey::from_openssh(key).expect("chave")).expect("hostkey")
    })
    .clone()
}

/// Espera a penalidade por origem do sshd expirar (ver `crates/ssh/tests`).
async fn ready() {
    use tokio::io::AsyncReadExt;
    let (h, p) = target();
    for _ in 0..60 {
        if let Ok(mut s) = tokio::net::TcpStream::connect((h.as_str(), p)).await {
            let mut b = [0u8; 4];
            if let Ok(Ok(4)) = tokio::time::timeout(Duration::from_secs(2), s.read(&mut b)).await
                && &b == b"SSH-"
            {
                drop(s);
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let _ = server_key();
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    panic!("servidor de teste indisponível (penalidade do sshd?)");
}

fn sink() -> EventSink {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    EventSink::new(tx, Arc::new(|| {}))
}

async fn connect() -> Arc<Connection> {
    ready().await;
    let store = Arc::new(Store::open_in_memory().expect("store"));
    store.trust(&endpoint(), &server_key()).expect("trust");
    let trust: Arc<dyn myssh_core::trust::HostTrustStore> = store;
    let req = ConnectRequest {
        endpoint: endpoint(),
        username: "tester".into(),
        auth: AuthMaterial::Password(SecretString::from("tester-pass".to_string())),
        pty: PtySize {
            cols: 80,
            rows: 24,
            pix_w: 640,
            pix_h: 384,
        },
        term: "xterm-256color".into(),
        connect_timeout: Duration::from_secs(10),
        keepalive: Some(Duration::from_secs(15)),
        jumps: vec![],
    };
    Connection::connect(&req, &trust, &sink())
        .await
        .expect("connect")
}

fn manager(conn: &Arc<Connection>, conc: usize) -> TransferManager {
    TransferManager::new(
        &tokio::runtime::Handle::current(),
        conn.clone(),
        conc,
        Arc::new(|| {}),
    )
}

/// Pasta remota limpa e exclusiva do teste.
struct Remote {
    base: String,
    browser: RemoteBrowser,
}

impl Remote {
    async fn new(conn: &Arc<Connection>, name: &str) -> Self {
        let browser = RemoteBrowser::open(conn.clone()).await.expect("browser");
        let base = format!("/home/tester/xfer-{name}");
        let _ = browser.remove(&base, true).await;
        browser.mkdir(&base).await.expect("mkdir base");
        Remote { base, browser }
    }

    fn p(&self, rel: &str) -> String {
        format!("{}/{}", self.base, rel)
    }

    async fn names(&self, rel: &str) -> Vec<String> {
        let dir = if rel.is_empty() {
            self.base.clone()
        } else {
            self.p(rel)
        };
        self.browser
            .list(&dir)
            .await
            .expect("list")
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    async fn assert_no_leftovers(&self, rel: &str) {
        let n = self.names(rel).await;
        assert!(
            !n.iter().any(|x| x.contains("myssh-")),
            "sobras de temporário/sidecar em '{rel}': {n:?}"
        );
    }

    async fn cleanup(self) {
        let _ = self.browser.remove(&self.base, true).await;
    }
}

#[derive(Default, Debug)]
struct Run {
    errors: Vec<(String, String)>,
    finished: Option<(u64, u64, u64)>,
    cancelled: bool,
    conflicts: Vec<String>,
    scanning: Vec<(u64, u64)>,
    progress: Vec<(u64, u64, u64)>,
    queued: Option<String>,
}

enum Hook<'a> {
    Progress { done: u64 },
    Conflict { path: &'a str },
}

/// Drena eventos até `Finished`/`Cancelled`. O gancho decide as respostas a conflitos.
async fn drive(
    mgr: &mut TransferManager,
    secs: u64,
    mut hook: impl FnMut(&TransferManager, Hook) -> Option<ConflictDecision>,
) -> Run {
    let mut run = Run::default();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        let Some(ev) = mgr.try_recv() else {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        };
        match ev {
            TransferEvent::Queued { label, .. } => run.queued = Some(label),
            TransferEvent::Scanning { files, bytes, .. } => run.scanning.push((files, bytes)),
            TransferEvent::Progress {
                bytes_done,
                bytes_total,
                rate_bps,
                ..
            } => {
                run.progress.push((bytes_done, bytes_total, rate_bps));
                hook(mgr, Hook::Progress { done: bytes_done });
            }
            TransferEvent::Conflict {
                path, responder, ..
            } => {
                run.conflicts.push(path.clone());
                let d = hook(mgr, Hook::Conflict { path: &path })
                    .expect("conflito sem decisão do teste");
                responder.answer(d);
            }
            TransferEvent::FileError { path, message, .. } => run.errors.push((path, message)),
            TransferEvent::Finished {
                ok,
                failed,
                skipped,
                ..
            } => {
                run.finished = Some((ok, failed, skipped));
                return run;
            }
            TransferEvent::Cancelled { .. } => {
                run.cancelled = true;
                return run;
            }
        }
    }
    panic!("transferência não terminou em {secs}s: {run:#?}");
}

async fn run_simple(mgr: &mut TransferManager, req: TransferRequest) -> Run {
    let _id = mgr.enqueue(req);
    drive(mgr, 120, |_, h| match h {
        Hook::Conflict { path } => panic!("conflito inesperado: {path}"),
        _ => None,
    })
    .await
}

fn req(dir: Direction, local: &Path, remote: &str) -> TransferRequest {
    TransferRequest::new(dir, local, remote)
}

/// Bytes pseudo-aleatórios (incompressíveis) determinísticos.
fn fill(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn write_big(path: &Path, seed: u64, mib: usize) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    for i in 0..mib {
        f.write_all(&fill(seed + i as u64, 1024 * 1024)).unwrap();
    }
}

fn set_mtime(path: &Path, secs: u64) {
    filetime::set_file_mtime(
        path,
        filetime::FileTime::from_system_time(UNIX_EPOCH + Duration::from_secs(secs)),
    )
    .unwrap();
}

fn mtime_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Mapa caminho relativo -> conteúdo (diretórios com `None`), sem seguir symlinks.
fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let rel = e
                .path()
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let ft = e.file_type().unwrap();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                out.insert(rel, None);
                walk(base, &e.path(), out);
            } else {
                out.insert(rel, Some(std::fs::read(e.path()).unwrap()));
            }
        }
    }
    let mut m = BTreeMap::new();
    walk(root, root, &mut m);
    m
}

async fn rsftp(conn: &Arc<Connection>) -> myssh_ssh::Sftp {
    conn.open_sftp().await.expect("sftp")
}

/// `SftpSession::write` exige que o arquivo já exista: este cria/trunca.
trait Put {
    async fn put(
        &self,
        path: impl Into<String>,
        data: &[u8],
    ) -> russh_sftp::client::rawsession::SftpResult<()>;
}

impl Put for myssh_ssh::Sftp {
    async fn put(
        &self,
        path: impl Into<String>,
        data: &[u8],
    ) -> russh_sftp::client::rawsession::SftpResult<()> {
        use tokio::io::AsyncWriteExt;
        let mut f = self.create(path).await?;
        f.write_all(data).await?;
        f.shutdown().await?;
        Ok(())
    }
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

// -------------------------------------------------------------------------------- testes

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn arvore_unicode_espacos_profunda_roundtrip() {
    let conn = connect().await;
    let r = Remote::new(&conn, "arvore").await;
    let mut mgr = manager(&conn, 3);
    let src = tmp();
    let root = src.path().join("projeto ação 日本語");
    std::fs::create_dir_all(root.join("pasta com espaço/sub dir")).unwrap();
    std::fs::create_dir_all(root.join("日本語")).unwrap();
    std::fs::create_dir_all(root.join("vazio")).unwrap();
    std::fs::write(root.join("pasta com espaço/ação ñ.txt"), "olá, mundo").unwrap();
    std::fs::write(
        root.join("pasta com espaço/sub dir/a b c.bin"),
        fill(1, 300_000),
    )
    .unwrap();
    std::fs::write(root.join("日本語/ファイル.txt"), "unicode").unwrap();
    std::fs::write(root.join("zero.txt"), "").unwrap();
    std::fs::write(root.join("raiz.bin"), fill(2, 5_000_000)).unwrap();
    // diretório profundo
    let mut deep = root.join("fundo");
    for i in 0..40 {
        deep = deep.join(format!("n{i:02}"));
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("folha.txt"), "no fundo").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", root.join("link-para-fora")).unwrap();
        std::os::unix::fs::symlink("raiz.bin", root.join("link-relativo")).unwrap();
    }
    set_mtime(&root.join("raiz.bin"), 1_600_000_000);
    set_mtime(&root.join("pasta com espaço/ação ñ.txt"), 1_500_000_000);

    let run = run_simple(&mut mgr, req(Direction::Upload, &root, &r.base)).await;
    assert_eq!(run.queued.as_deref(), Some("projeto ação 日本語"));
    let (ok, failed, skipped) = run.finished.expect("finished");
    assert_eq!(failed, 0, "{run:#?}");
    assert_eq!(ok, 6, "{run:#?}");
    #[cfg(unix)]
    {
        assert_eq!(skipped, 2, "{run:#?}");
        assert!(run.errors.iter().all(|(_, m)| m.contains("simbólico")));
    }
    #[cfg(not(unix))]
    assert_eq!(skipped, 0);
    let last_scan = run.scanning.last().copied().unwrap();
    assert_eq!(last_scan.0, 6, "contagem prévia de arquivos");
    assert_eq!(last_scan.1, 11 + 300_000 + 7 + 5_000_000 + 8);
    let (done, total, _) = *run.progress.last().expect("progress");
    assert_eq!((done, total), (last_scan.1, last_scan.1));

    let base = format!("{}/projeto ação 日本語", r.base);
    let names = r.browser.list(&base).await.unwrap();
    let n: Vec<&str> = names.iter().map(|e| e.name.as_str()).collect();
    assert!(n.contains(&"raiz.bin") && n.contains(&"vazio") && n.contains(&"日本語"));
    assert!(!n.contains(&"link-para-fora") && !n.contains(&"link-relativo"));
    let st = r
        .browser
        .stat(&format!("{base}/raiz.bin"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(st.size, 5_000_000);
    assert_eq!(
        st.mtime
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_600_000_000,
        "mtime preservado no upload"
    );
    r.assert_no_leftovers("projeto ação 日本語").await;
    r.assert_no_leftovers("projeto ação 日本語/pasta com espaço")
        .await;

    // volta
    let dst = tmp();
    let run = run_simple(&mut mgr, req(Direction::Download, dst.path(), &base)).await;
    assert_eq!(run.finished, Some((6, 0, 0)), "{run:#?}");
    let got = dst.path().join("projeto ação 日本語");
    let mut expected = snapshot(&root);
    expected.retain(|k, _| !k.starts_with("link-"));
    assert_eq!(snapshot(&got), expected);
    assert_eq!(
        mtime_secs(&got.join("raiz.bin")),
        1_600_000_000,
        "mtime preservado no download"
    );
    assert_eq!(mgr.active(), 0);
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn permissoes_so_com_preserve_perms_e_sem_setuid() {
    let conn = connect().await;
    let r = Remote::new(&conn, "perms").await;
    let mut mgr = manager(&conn, 2);
    let src = tmp();
    let f = src.path().join("exec.sh");
    std::fs::write(&f, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o4750)).unwrap();
    }
    // sem preserve_perms: permissões do servidor (umask), nunca 0o4750
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &r.base)).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let st = r.browser.stat(&r.p("exec.sh")).await.unwrap().unwrap();
    assert_ne!(st.perms.unwrap() & 0o7777, 0o750 | 0o4000);
    // com preserve_perms: 0o750, sem setuid
    let mut q = req(Direction::Upload, &f, &r.base);
    q.preserve_perms = true;
    q.overwrite = Overwrite::Overwrite;
    let run = run_simple(&mut mgr, q).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let st = r.browser.stat(&r.p("exec.sh")).await.unwrap().unwrap();
    assert_eq!(st.perms.unwrap() & 0o7777, 0o750);
    // download com preserve_perms
    let dst = tmp();
    let mut q = req(Direction::Download, dst.path(), &r.p("exec.sh"));
    q.preserve_perms = true;
    let run = run_simple(&mut mgr, q).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(dst.path().join("exec.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(m & 0o7777, 0o750);
    }
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn conflitos_overwrite_skip_rename_ask() {
    let conn = connect().await;
    let r = Remote::new(&conn, "conflitos").await;
    let mut mgr = manager(&conn, 3);
    let src = tmp();
    let dir = src.path().join("d");
    std::fs::create_dir(&dir).unwrap();
    for (n, c) in [
        ("a.txt", "novo-a"),
        ("b.txt", "novo-b"),
        ("c.txt", "novo-c"),
    ] {
        std::fs::write(dir.join(n), c).unwrap();
    }
    let seed = |sftp: myssh_ssh::Sftp| async move {
        for n in ["a.txt", "b.txt", "c.txt"] {
            sftp.put(format!("/home/tester/xfer-conflitos/d/{n}"), b"antigo")
                .await
                .unwrap();
        }
    };
    let content = |n: &'static str| {
        let c = conn.clone();
        async move {
            let s = rsftp(&c).await;
            String::from_utf8(
                s.read(format!("/home/tester/xfer-conflitos/d/{n}"))
                    .await
                    .unwrap(),
            )
            .unwrap()
        }
    };
    let reset = || async {
        let s = rsftp(&conn).await;
        let _ = s.create_dir("/home/tester/xfer-conflitos/d").await;
        seed(s).await;
    };
    reset().await;

    // Overwrite
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Overwrite;
    let run = run_simple(&mut mgr, q).await;
    assert_eq!(run.finished, Some((3, 0, 0)), "{run:#?}");
    assert_eq!(content("a.txt").await, "novo-a");
    r.assert_no_leftovers("d").await;
    let n = r.names("d").await;
    assert!(!n.iter().any(|x| x.contains(".myssh-old")), "{n:?}");

    // Skip
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Skip;
    let run = run_simple(&mut mgr, q).await;
    assert_eq!(run.finished, Some((0, 0, 3)), "{run:#?}");
    assert_eq!(content("b.txt").await, "antigo");

    // Rename
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Rename;
    let run = run_simple(&mut mgr, q.clone()).await;
    assert_eq!(run.finished, Some((3, 0, 0)), "{run:#?}");
    assert_eq!(content("a.txt").await, "antigo");
    assert_eq!(content("a (1).txt").await, "novo-a");
    let run = run_simple(&mut mgr, q).await; // segunda vez: "(2)"
    assert_eq!(run.finished, Some((3, 0, 0)), "{run:#?}");
    assert_eq!(content("a (2).txt").await, "novo-a");

    // Ask: decisões individuais (ordem de chegada dos conflitos é livre; decide pelo nome)
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Ask;
    mgr.enqueue(q);
    let run = drive(&mut mgr, 60, |_, h| match h {
        Hook::Conflict { path } => Some(if path.ends_with("a.txt") {
            ConflictDecision::Overwrite
        } else if path.ends_with("b.txt") {
            ConflictDecision::Skip
        } else {
            ConflictDecision::Rename
        }),
        _ => None,
    })
    .await;
    assert_eq!(run.finished, Some((2, 0, 1)), "{run:#?}");
    assert_eq!(run.conflicts.len(), 3);
    assert_eq!(content("a.txt").await, "novo-a");
    assert_eq!(content("b.txt").await, "antigo");
    assert_eq!(content("c.txt").await, "antigo");
    assert_eq!(content("c (1).txt").await, "novo-c");

    // Ask: SkipAll responde uma vez só
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Ask;
    mgr.enqueue(q);
    let run = drive(&mut mgr, 60, |_, h| match h {
        Hook::Conflict { .. } => Some(ConflictDecision::SkipAll),
        _ => None,
    })
    .await;
    assert_eq!(run.finished, Some((0, 0, 3)), "{run:#?}");
    assert_eq!(run.conflicts.len(), 1, "SkipAll deve valer para os demais");

    // Ask: OverwriteAll
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Ask;
    mgr.enqueue(q);
    let run = drive(&mut mgr, 60, |_, h| match h {
        Hook::Conflict { .. } => Some(ConflictDecision::OverwriteAll),
        _ => None,
    })
    .await;
    assert_eq!(run.finished, Some((3, 0, 0)), "{run:#?}");
    assert_eq!(run.conflicts.len(), 1);
    assert_eq!(content("c.txt").await, "novo-c");

    // Ask: Cancel cancela tudo e não toca nos arquivos
    reset().await;
    let mut q = req(Direction::Upload, &dir, &r.base);
    q.overwrite = Overwrite::Ask;
    mgr.enqueue(q);
    let run = drive(&mut mgr, 60, |_, h| match h {
        Hook::Conflict { .. } => Some(ConflictDecision::Cancel),
        _ => None,
    })
    .await;
    assert!(run.cancelled, "{run:#?}");
    assert_eq!(content("a.txt").await, "antigo");
    r.assert_no_leftovers("d").await;

    // Download com conflito local e Rename. O remoto já tem "a (1).txt", "a (2).txt"... dos
    // passos anteriores: o nome livre escolhido para "a.txt" pode ser ocupado por outro arquivo
    // do mesmo lote durante a cópia, e nenhum dos dois pode sumir (publicação sem substituir).
    let dst = tmp();
    std::fs::create_dir(dst.path().join("d")).unwrap();
    std::fs::write(dst.path().join("d/a.txt"), "local-antigo").unwrap();
    let remote_names = r.names("d").await;
    let mut q = req(Direction::Download, dst.path(), &r.p("d"));
    q.overwrite = Overwrite::Rename;
    let run = run_simple(&mut mgr, q).await;
    assert!(run.finished.unwrap().1 == 0, "{run:#?}");
    assert_eq!(
        std::fs::read_to_string(dst.path().join("d/a.txt")).unwrap(),
        "local-antigo"
    );
    let local: Vec<String> = std::fs::read_dir(dst.path().join("d"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(
        local.len(),
        remote_names.len() + 1,
        "{local:?} {remote_names:?}"
    );
    // "a.txt", "b.txt" e "c.txt" remotos continuam "antigo": os três chegaram.
    assert_eq!(
        local.iter().filter(|c| *c == "antigo").count(),
        3,
        "{local:?}"
    );
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn cancelamento_remove_temporario_e_preserva_destino() {
    let conn = connect().await;
    let r = Remote::new(&conn, "cancel").await;
    let mut mgr = manager(&conn, 3);
    let src = tmp();
    let big = src.path().join("grande.bin");
    write_big(&big, 7, 200);

    // destino já existente (deve ficar intacto) + upload com overwrite, cancelado no meio
    rsftp(&conn)
        .await
        .put(r.p("grande.bin"), b"conteudo-antigo")
        .await
        .unwrap();
    let mut q = req(Direction::Upload, &big, &r.base);
    q.overwrite = Overwrite::Overwrite;
    mgr.enqueue(q);
    let mut cancelled = false;
    let run = drive(&mut mgr, 120, |m, h| {
        if let Hook::Progress { done, .. } = h
            && done > 2 * 1024 * 1024
            && !cancelled
        {
            cancelled = true;
            m.cancel(TransferId(1));
        }
        None
    })
    .await;
    assert!(run.cancelled, "{run:#?}");
    assert!(cancelled);
    assert_eq!(
        String::from_utf8(rsftp(&conn).await.read(r.p("grande.bin")).await.unwrap()).unwrap(),
        "conteudo-antigo",
        "destino intacto"
    );
    r.assert_no_leftovers("").await;
    assert_eq!(mgr.active(), 0);

    // download cancelado: sem temporário local, sem destino
    rsftp(&conn)
        .await
        .put(r.p("g.bin"), &fill(1, 1))
        .await
        .unwrap(); // marcador
    let mut up = req(Direction::Upload, &big, &r.base);
    up.overwrite = Overwrite::Overwrite;
    let run = run_simple(&mut mgr, up).await; // grande de verdade no servidor
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let dst = tmp();
    let id = mgr.enqueue(req(Direction::Download, dst.path(), &r.p("grande.bin")));
    let mut did = false;
    let run = drive(&mut mgr, 120, |m, h| {
        if let Hook::Progress { done, .. } = h
            && done > 2 * 1024 * 1024
            && !did
        {
            did = true;
            m.cancel(id);
        }
        None
    })
    .await;
    assert!(run.cancelled, "{run:#?}");
    let left: Vec<_> = std::fs::read_dir(dst.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(left.is_empty(), "restos locais: {left:?}");
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn permissao_negada() {
    let conn = connect().await;
    let r = Remote::new(&conn, "permissao").await;
    let mut mgr = manager(&conn, 2);
    let src = tmp();
    let f = src.path().join("x.txt");
    std::fs::write(&f, "x").unwrap();

    // diretório remoto 0555
    let ro = r.p("somente-leitura");
    r.browser.mkdir(&ro).await.unwrap();
    let mut a = russh_sftp::protocol::FileAttributes::empty();
    a.permissions = Some(0o555);
    rsftp(&conn)
        .await
        .set_metadata(ro.as_str(), a)
        .await
        .unwrap();
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &ro)).await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m.contains("Permissão negada")),
        "{run:#?}"
    );
    r.assert_no_leftovers("somente-leitura").await;

    // /root
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, "/root")).await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m.contains("Permissão negada")),
        "{run:#?}"
    );

    // download de arquivo ilegível (0000)
    rsftp(&conn)
        .await
        .put(r.p("secreto.txt"), b"segredo")
        .await
        .unwrap();
    let mut a = russh_sftp::protocol::FileAttributes::empty();
    a.permissions = Some(0o000);
    rsftp(&conn)
        .await
        .set_metadata(r.p("secreto.txt"), a)
        .await
        .unwrap();
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &r.p("secreto.txt")),
    )
    .await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m.contains("Permissão negada")),
        "{run:#?}"
    );
    assert_eq!(
        std::fs::read_dir(dst.path()).unwrap().count(),
        0,
        "sem sobras locais"
    );

    // destino local sem permissão de escrita
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        rsftp(&conn).await.put(r.p("ok.txt"), b"ok").await.unwrap();
        let ro = dst.path().join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let run = run_simple(&mut mgr, req(Direction::Download, &ro, &r.p("ok.txt"))).await;
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
        assert!(
            run.errors
                .iter()
                .any(|(_, m)| m.contains("Permissão negada")),
            "{run:#?}"
        );
    }
    // origem local inexistente
    let run = run_simple(
        &mut mgr,
        req(Direction::Upload, &src.path().join("nao-existe"), &r.base),
    )
    .await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("não encontrado")),
        "{run:#?}"
    );
    // restaura para a limpeza
    let mut a = russh_sftp::protocol::FileAttributes::empty();
    a.permissions = Some(0o755);
    let _ = rsftp(&conn).await.set_metadata(ro.as_str(), a).await;
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn disco_cheio_erro_claro_e_sem_temporario() {
    let conn = connect().await;
    let mut mgr = manager(&conn, 3);
    let browser = RemoteBrowser::open(conn.clone()).await.unwrap();
    let small = "/home/tester/small";
    // limpa restos de execuções anteriores
    for e in browser
        .list(small)
        .await
        .expect("tmpfs 'small' (rode ./testenv/up.sh)")
    {
        let _ = browser.remove(&e.path, true).await;
    }
    let src = tmp();
    let big = src.path().join("enorme.bin");
    write_big(&big, 3, 5);

    // 5 MiB num tmpfs de 2 MiB: falha antes de gravar (statvfs)
    let run = run_simple(&mut mgr, req(Direction::Upload, &big, small)).await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("Sem espaço")),
        "{run:#?}"
    );
    let n: Vec<String> = browser
        .list(small)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(n.is_empty(), "temporário deveria ter sido removido: {n:?}");

    // Dois arquivos de 1,5 MiB em paralelo: ambos passam na checagem prévia, um enche o disco no
    // meio da escrita ("Failure" do servidor, traduzido para "Sem espaço").
    let two = src.path().join("dois");
    std::fs::create_dir(&two).unwrap();
    for i in 0..2 {
        std::fs::write(two.join(format!("f{i}.bin")), fill(10 + i, 1_500_000)).unwrap();
    }
    let run = run_simple(&mut mgr, req(Direction::Upload, &two, small)).await;
    let (ok, failed, _) = run.finished.unwrap();
    assert_eq!(ok + failed, 2, "{run:#?}");
    assert!(failed >= 1, "{run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("Sem espaço")),
        "{run:#?}"
    );
    let all: Vec<String> = browser
        .list(&format!("{small}/dois"))
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(!all.iter().any(|x| x.contains("myssh-")), "{all:?}");
    assert_eq!(all.len() as u64, ok, "só os concluídos ficam: {all:?}");
    let _ = browser.remove(&format!("{small}/dois"), true).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn origem_alterada_durante_a_transferencia() {
    let conn = connect().await;
    let r = Remote::new(&conn, "alterada").await;
    let mut mgr = manager(&conn, 2);
    let src = tmp();
    let f = src.path().join("vivo.bin");
    write_big(&f, 5, 120);

    // upload: outra thread acrescenta bytes à origem enquanto a transferência corre
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (s2, f2) = (stop.clone(), f.clone());
    let t = std::thread::spawn(move || {
        use std::io::Write;
        while !s2.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut h) = std::fs::OpenOptions::new().append(true).open(&f2) {
                let _ = h.write_all(b"x");
            }
            std::thread::sleep(Duration::from_millis(3));
        }
    });
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &r.base)).await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    t.join().unwrap();
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m == "origem alterada durante a transferência"),
        "{run:#?}"
    );
    assert!(
        r.names("").await.is_empty(),
        "nada publicado nem sobrando: {:?}",
        r.names("").await
    );

    // download: a origem remota muda enquanto baixa
    write_big(&f, 5, 120);
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &r.base)).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (s2, c2, rp) = (stop.clone(), conn.clone(), r.p("vivo.bin"));
    let writer = tokio::spawn(async move {
        use russh_sftp::protocol::OpenFlags;
        use tokio::io::AsyncWriteExt;
        let s = c2.open_sftp().await.unwrap();
        while !s2.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut h) = s
                .open_with_flags(rp.as_str(), OpenFlags::WRITE | OpenFlags::APPEND)
                .await
            {
                let _ = h.write_all(b"y").await;
                let _ = h.shutdown().await;
            }
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    });
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &r.p("vivo.bin")),
    )
    .await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = writer.await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m == "origem alterada durante a transferência"),
        "{run:#?}"
    );
    assert_eq!(
        std::fs::read_dir(dst.path()).unwrap().count(),
        0,
        "sem destino nem temporário"
    );
    r.cleanup().await;
}

/// Interrompe a conexão assim que o progresso passa de `after` bytes.
async fn run_until_conn_dies(mgr: &mut TransferManager, conn: &Arc<Connection>, after: u64) -> Run {
    let mut done = false;
    let c = conn.clone();
    drive(mgr, 120, move |_, h| {
        if let Hook::Progress { done: d, .. } = h
            && d >= after
            && !done
        {
            done = true;
            let c = c.clone();
            tokio::spawn(async move { c.close().await });
        }
        None
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn retomada_valida_upload_e_download() {
    let conn = connect().await;
    let r = Remote::new(&conn, "retomada").await;
    let src = tmp();
    let f = src.path().join("meio.bin");
    write_big(&f, 11, 300);
    let want = std::fs::read(&f).unwrap();
    let remote_dir = r.base.clone();

    // --- upload interrompido por queda de conexão
    {
        let mut mgr = manager(&conn, 3);
        mgr.enqueue(req(Direction::Upload, &f, &remote_dir));
        let run = run_until_conn_dies(&mut mgr, &conn, 8 * 1024 * 1024).await;
        let (ok, failed, _) = run.finished.expect("finished");
        assert_eq!((ok, failed), (0, 1), "{run:#?}");
        assert!(
            run.errors.iter().any(|(_, m)| m.contains("conexão")),
            "{run:#?}"
        );
    }
    // --- nova conexão: o parcial e o sidecar estão lá; retoma
    let conn2 = connect().await;
    let r2 = Remote {
        base: r.base.clone(),
        browser: RemoteBrowser::open(conn2.clone()).await.unwrap(),
    };
    let names = r2.names("").await;
    assert!(
        names.iter().any(|n| n.contains(".myssh-part-")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.ends_with(".myssh-resume")),
        "{names:?}"
    );
    let mut mgr = manager(&conn2, 3);
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &remote_dir)).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let s = mgr.stats();
    assert_eq!(s.files_resumed, 1, "{s:?}");
    assert!(s.bytes_resumed >= 1024 * 1024, "{s:?}");
    assert_eq!(
        s.bytes_transferred + s.bytes_resumed,
        want.len() as u64,
        "{s:?}"
    );
    r2.assert_no_leftovers("").await;
    // confere o conteúdo baixando de volta
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &r2.p("meio.bin")),
    )
    .await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    assert!(
        std::fs::read(dst.path().join("meio.bin")).unwrap() == want,
        "conteúdo divergente"
    );

    // --- download interrompido e retomado
    let dst2 = tmp();
    {
        let mut mgr = manager(&conn2, 3);
        mgr.enqueue(req(Direction::Download, dst2.path(), &r2.p("meio.bin")));
        let run = run_until_conn_dies(&mut mgr, &conn2, 8 * 1024 * 1024).await;
        assert_eq!(run.finished.map(|f| (f.0, f.1)), Some((0, 1)), "{run:#?}");
    }
    let left: Vec<String> = std::fs::read_dir(dst2.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(left.iter().any(|n| n.contains(".myssh-part-")), "{left:?}");
    assert!(
        left.iter().any(|n| n.ends_with(".myssh-resume")),
        "{left:?}"
    );
    let conn3 = connect().await;
    let mut mgr = manager(&conn3, 3);
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst2.path(), &r2.p("meio.bin")),
    )
    .await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    let s = mgr.stats();
    assert_eq!(s.files_resumed, 1, "{s:?}");
    assert!(std::fs::read(dst2.path().join("meio.bin")).unwrap() == want);
    let left: Vec<_> = std::fs::read_dir(dst2.path()).unwrap().collect();
    assert_eq!(left.len(), 1, "sem sobras locais");
    r2.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn retomada_invalida_recomeca_do_zero() {
    let conn = connect().await;
    let r = Remote::new(&conn, "retomada-inv").await;
    let src = tmp();
    let f = src.path().join("meio.bin");
    write_big(&f, 21, 200);

    let interrupt = |conn: Arc<Connection>, f: PathBuf, base: String| async move {
        let mut mgr = manager(&conn, 3);
        mgr.enqueue(req(Direction::Upload, &f, &base));
        let run = run_until_conn_dies(&mut mgr, &conn, 8 * 1024 * 1024).await;
        assert_eq!(run.finished.map(|x| (x.0, x.1)), Some((0, 1)), "{run:#?}");
    };

    // 1) origem alterada (mesmo tamanho, outro conteúdo/mtime) -> não retoma
    interrupt(conn.clone(), f.clone(), r.base.clone()).await;
    let mut data = std::fs::read(&f).unwrap();
    data[1000] ^= 0xFF;
    std::fs::write(&f, &data).unwrap();
    set_mtime(&f, mtime_secs(&f) + 5);
    let conn2 = connect().await;
    let rb = Remote {
        base: r.base.clone(),
        browser: RemoteBrowser::open(conn2.clone()).await.unwrap(),
    };
    let mut mgr = manager(&conn2, 3);
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &r.base)).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    assert_eq!(mgr.stats().files_resumed, 0);
    assert_eq!(mgr.stats().bytes_transferred, data.len() as u64);
    rb.assert_no_leftovers("").await;
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &rb.p("meio.bin")),
    )
    .await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    assert!(std::fs::read(dst.path().join("meio.bin")).unwrap() == data);

    // 2) parcial corrompido no trecho final -> não retoma
    let _ = rb.browser.remove(&rb.p("meio.bin"), false).await;
    let conn3 = connect().await;
    interrupt(conn3.clone(), f.clone(), r.base.clone()).await;
    let conn4 = connect().await;
    let sftp = rsftp(&conn4).await;
    let part = rb
        .names("")
        .await
        .into_iter()
        .find(|n| n.contains(".myssh-part-"))
        .expect("parcial");
    {
        use russh_sftp::protocol::OpenFlags;
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let size = sftp.metadata(rb.p(&part)).await.unwrap().size.unwrap();
        let mut h = sftp
            .open_with_flags(rb.p(&part), OpenFlags::WRITE)
            .await
            .unwrap();
        h.seek(std::io::SeekFrom::Start(size - 100)).await.unwrap();
        h.write_all(&[0xAB; 100]).await.unwrap();
        h.shutdown().await.unwrap();
    }
    let mut mgr = manager(&conn4, 3);
    let run = run_simple(&mut mgr, req(Direction::Upload, &f, &r.base)).await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    assert_eq!(
        mgr.stats().files_resumed,
        0,
        "parcial corrompido não pode ser retomado"
    );
    rb.assert_no_leftovers("").await;
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &rb.p("meio.bin")),
    )
    .await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    assert!(std::fs::read(dst.path().join("meio.bin")).unwrap() == data);
    rb.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn queda_de_conexao_no_meio_falha_limpa() {
    let conn = connect().await;
    let r = Remote::new(&conn, "queda").await;
    let src = tmp();
    let d = src.path().join("lote");
    std::fs::create_dir(&d).unwrap();
    for i in 0..24 {
        write_big(&d.join(format!("f{i:02}.bin")), 100 + i, 6);
    }
    let mut mgr = manager(&conn, 3);
    mgr.enqueue(req(Direction::Upload, &d, &r.base));
    let t0 = Instant::now();
    let run = run_until_conn_dies(&mut mgr, &conn, 20 * 1024 * 1024).await;
    let (ok, failed, skipped) = run.finished.expect("finished mesmo sem conexão");
    println!(
        "queda: ok={ok} failed={failed} skipped={skipped} em {:?}",
        t0.elapsed()
    );
    assert!(failed >= 1 && ok < 24, "{run:#?}");
    assert_eq!(ok + failed + skipped, 24, "toda a contagem fecha: {run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("conexão")),
        "{run:#?}"
    );
    assert!(t0.elapsed() < Duration::from_secs(60));
    assert_eq!(mgr.active(), 0);
    // nada de arquivo final parcial: os publicados têm o tamanho certo
    let conn2 = connect().await;
    let rb = RemoteBrowser::open(conn2).await.unwrap();
    for e in rb.list(&r.p("lote")).await.unwrap() {
        if !e.name.starts_with('.') {
            assert_eq!(e.size, 6 * 1024 * 1024, "{}", e.name);
        }
    }
    let _ = rb.remove(&r.base, true).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn colisao_por_caixa_ao_baixar_e_explicita() {
    let conn = connect().await;
    let r = Remote::new(&conn, "caixa").await;
    let s = rsftp(&conn).await;
    s.create_dir(r.p("d")).await.unwrap();
    s.put(r.p("d/Foo.txt"), b"MAIUSCULO").await.unwrap();
    s.put(r.p("d/foo.txt"), b"minusculo").await.unwrap();
    s.put(r.p("d/livre.txt"), b"livre").await.unwrap();
    let mut mgr = manager(&conn, 2);
    mgr.set_local_case_insensitive(true);
    let dst = tmp();
    let mut q = req(Direction::Download, dst.path(), &r.p("d"));
    q.overwrite = Overwrite::Overwrite; // mesmo com "sobrescrever", nunca silencioso
    let run = run_simple(&mut mgr, q).await;
    assert_eq!(run.finished, Some((1, 2, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m.contains("Foo.txt") && m.contains("foo.txt") && m.contains("Conflito")),
        "{run:#?}"
    );
    let got: Vec<String> = std::fs::read_dir(dst.path().join("d"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(got, vec!["livre.txt".to_string()]);
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn navegador_remoto_e_remocao_segura() {
    let conn = connect().await;
    let r = Remote::new(&conn, "browser").await;
    let b = &r.browser;
    assert_eq!(b.home().await.unwrap(), "/home/tester");
    b.mkdir(&r.p("b-dir")).await.unwrap();
    b.mkdir(&r.p("A-dir")).await.unwrap();
    let s = rsftp(&conn).await;
    s.put(r.p("z.txt"), b"z").await.unwrap();
    s.put(r.p("b-dir/f.txt"), b"f").await.unwrap();
    // OpenSSH espera (alvo, nome do link): a ordem dos argumentos da biblioteca é a do fio.
    s.symlink("/etc".to_string(), r.p("b-dir/link"))
        .await
        .unwrap();
    let l = b.list(&r.base).await.unwrap();
    assert_eq!(
        l.iter()
            .map(|e| (e.name.as_str(), e.kind))
            .collect::<Vec<_>>(),
        vec![
            ("A-dir", Kind::Dir),
            ("b-dir", Kind::Dir),
            ("z.txt", Kind::File)
        ]
    );
    assert!(l.iter().all(|e| e.name != "." && e.name != ".."));
    assert_eq!(l[2].size, 1);
    assert!(l[2].mtime.is_some());
    let inner = b.list(&r.p("b-dir")).await.unwrap();
    assert!(
        inner
            .iter()
            .any(|e| e.name == "link" && e.kind == Kind::Symlink)
    );
    b.rename(&r.p("z.txt"), &r.p("y.txt")).await.unwrap();
    assert!(b.stat(&r.p("z.txt")).await.unwrap().is_none());
    assert!(b.stat(&r.p("y.txt")).await.unwrap().is_some());
    assert!(b.remove(&r.p("nao-existe"), false).await.is_err());
    // diretório não vazio sem recursive falha; com recursive remove sem seguir o link para /etc
    assert!(b.remove(&r.p("b-dir"), false).await.is_err());
    b.remove(&r.p("b-dir"), true).await.unwrap();
    assert!(b.stat(&r.p("b-dir")).await.unwrap().is_none());
    assert!(
        b.stat("/etc/passwd").await.unwrap().is_some(),
        "link não foi seguido"
    );
    assert!(b.stat("/home/tester/../../etc/passwd").await.is_ok());
    // recusa a raiz
    assert!(b.remove("/", true).await.is_err());
    assert!(b.remove("", true).await.is_err());
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn remote_tree_implementa_filetree() {
    let conn = connect().await;
    let r = Remote::new(&conn, "tree").await;
    let s = rsftp(&conn).await;
    s.create_dir(r.p("sub")).await.unwrap();
    s.put(r.p("sub/a.txt"), b"12345").await.unwrap();
    s.symlink(r.p("sub"), r.p("lnk")).await.unwrap();
    let mut a = russh_sftp::protocol::FileAttributes::empty();
    a.atime = Some(1_700_000_000);
    a.mtime = Some(1_700_000_000);
    s.set_metadata(r.p("sub/a.txt"), a).await.unwrap();
    let tree = RemoteTree::open(conn.clone(), r.base.clone())
        .await
        .unwrap();
    let root = tree.list_dir(&RelPath::root()).await.unwrap();
    let kinds: Vec<(String, EntryKind)> = root.iter().map(|e| (e.name.clone(), e.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            ("lnk".into(), EntryKind::Symlink),
            ("sub".into(), EntryKind::Dir)
        ],
        "symlink reportado como tal (lstat), sem seguir"
    );
    let e = tree
        .stat(&RelPath::parse("sub/a.txt").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((e.kind, e.size), (EntryKind::File, 5));
    assert_eq!(e.mtime_resolution, Duration::from_secs(1));
    assert_eq!(
        e.mtime,
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    );
    assert!(
        tree.stat(&RelPath::parse("nada").unwrap())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        tree.list_dir(&RelPath::parse("nada").unwrap())
            .await
            .is_err()
    );
    // lstat: o link para diretório continua link
    let l = tree
        .stat(&RelPath::parse("lnk").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.kind, EntryKind::Symlink);
    r.cleanup().await;
}

// ------------------------------------------------------ desempenho e terminal responsivo

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

/// Abre um shell na MESMA conexão e mede o eco de `echo` periódico.
async fn shell_latencies(
    conn: &Arc<Connection>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    every: Duration,
) -> Vec<f64> {
    let mut sh = conn
        .open_shell(
            PtySize {
                cols: 100,
                rows: 30,
                pix_w: 0,
                pix_h: 0,
            },
            "xterm",
        )
        .await
        .expect("shell");
    sh.data(b"stty -echo; PS1=''; export PS1\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut acc = String::new();
    let mut lat = Vec::new();
    let mut i = 0u32;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let t0 = Instant::now();
        // O texto digitado tem '$((' e não aparece na saída; o resultado, sim.
        sh.data(format!("echo M$((1000+{i}))X\n").as_bytes())
            .await
            .unwrap();
        let want = format!("M{}X", 1000 + i);
        loop {
            if acc.contains(&want) {
                break;
            }
            match tokio::time::timeout(Duration::from_secs(20), sh.wait()).await {
                Ok(Some(russh::ChannelMsg::Data { data })) => {
                    acc.push_str(&String::from_utf8_lossy(&data));
                }
                Ok(Some(_)) => {}
                _ => panic!("terminal sem resposta"),
            }
        }
        acc.clear();
        lat.push(t0.elapsed().as_secs_f64() * 1000.0);
        i += 1;
        tokio::time::sleep(every).await;
    }
    lat
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn desempenho_200mib_e_terminal_responsivo() {
    let conn = connect().await;
    let r = Remote::new(&conn, "perf").await;
    let src = tmp();
    let big = src.path().join("200mib.bin");
    write_big(&big, 42, 200);
    let size = 200u64 * 1024 * 1024;

    // referência: terminal ocioso
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (c2, s2) = (conn.clone(), stop.clone());
    let idle =
        tokio::spawn(async move { shell_latencies(&c2, s2, Duration::from_millis(100)).await });
    tokio::time::sleep(Duration::from_secs(3)).await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut idle = idle.await.unwrap();

    // 3 uploads e 3 downloads de 200 MiB (sobrescrevendo), cada fase com o terminal ativo na
    // MESMA Connection, para ter amostras suficientes de latência.
    let mut mgr = manager(&conn, 3);
    let dst = tmp();
    let mut phases: Vec<(&str, Direction)> = vec![];
    phases.extend([("upload", Direction::Upload); 3]);
    phases.extend([("download", Direction::Download); 3]);
    let (mut up_rates, mut down_rates) = (vec![], vec![]);
    let (mut during_up, mut during_down) = (vec![], vec![]);
    for (name, dir) in phases {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (c2, s2) = (conn.clone(), stop.clone());
        let term =
            tokio::spawn(async move { shell_latencies(&c2, s2, Duration::from_millis(50)).await });
        tokio::time::sleep(Duration::from_millis(800)).await;
        let mut q = match dir {
            Direction::Upload => req(dir, &big, &r.base),
            Direction::Download => req(dir, dst.path(), &r.p("200mib.bin")),
        };
        q.overwrite = Overwrite::Overwrite;
        let t0 = Instant::now();
        let run = run_simple(&mut mgr, q).await;
        let el = t0.elapsed();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let lat = term.await.unwrap();
        assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
        let mbs = size as f64 / 1e6 / el.as_secs_f64();
        let peak = run.progress.iter().map(|p| p.2).max().unwrap_or(0) as f64 / 1e6;
        println!(
            "{} 200 MiB: {:.2}s = {mbs:.1} MB/s (pico da média móvel: {peak:.1} MB/s)",
            name.to_uppercase(),
            el.as_secs_f64()
        );
        if dir == Direction::Upload {
            up_rates.push(mbs);
            during_up.extend(lat);
        } else {
            down_rates.push(mbs);
            during_down.extend(lat);
        }
    }
    assert_eq!(
        std::fs::metadata(dst.path().join("200mib.bin"))
            .unwrap()
            .len(),
        size
    );
    println!(
        "MB/s mediano: upload {:.1}, download {:.1}",
        pct(&mut up_rates, 0.5),
        pct(&mut down_rates, 0.5)
    );

    let stats = |name: &str, v: &mut Vec<f64>| {
        let (p50, p95, max) = (pct(v, 0.5), pct(v, 0.95), pct(v, 1.0));
        println!(
            "eco do terminal ({name}): n={} p50={p50:.1} ms p95={p95:.1} ms max={max:.1} ms",
            v.len()
        );
        (p50, p95)
    };
    let (_, idle95) = stats("ocioso", &mut idle);
    let (_, up95) = stats("durante upload", &mut during_up);
    let (_, down95) = stats("durante download", &mut during_down);
    assert!(
        up95 < 1500.0 && down95 < 1500.0,
        "terminal engasgou: {up95} / {down95} (ocioso {idle95})"
    );
    r.cleanup().await;
}

// ----------------------------------------------------------------- revisão de segurança

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn varredura_respeita_limites_de_profundidade_e_entradas() {
    let conn = connect().await;
    let r = Remote::new(&conn, "scanlim").await;
    let s = rsftp(&conn).await;
    for d in ["arv", "arv/a", "arv/a/b", "arv/a/b/c"] {
        r.browser.mkdir(&r.p(d)).await.unwrap();
    }
    s.put(r.p("arv/a/b/c/f.txt"), b"x").await.unwrap();
    for i in 0..5 {
        s.put(r.p(&format!("arv/n{i}.txt")), b"x").await.unwrap();
    }
    let mut mgr = manager(&conn, 1);
    let dst = tmp();

    mgr.set_scan_limits(2, 1_000);
    let run = run_simple(&mut mgr, req(Direction::Download, dst.path(), &r.p("arv"))).await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("profundidade")),
        "{run:#?}"
    );

    mgr.set_scan_limits(64, 3);
    let run = run_simple(&mut mgr, req(Direction::Download, dst.path(), &r.p("arv"))).await;
    assert_eq!(run.finished, Some((0, 1, 0)), "{run:#?}");
    assert!(
        run.errors.iter().any(|(_, m)| m.contains("entradas")),
        "{run:#?}"
    );

    // Limites padrão: passa.
    mgr.set_scan_limits(64, 1_000_000);
    let run = run_simple(&mut mgr, req(Direction::Download, dst.path(), &r.p("arv"))).await;
    assert_eq!(run.finished.map(|f| f.1), Some(0), "{run:#?}");
    r.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn nomes_inseguros_ocultos_na_listagem_e_recusados_no_download() {
    let conn = connect().await;
    let r = Remote::new(&conn, "unsafe").await;
    let s = rsftp(&conn).await;
    r.browser.mkdir(&r.p("d")).await.unwrap();
    let ruim = "fatura\u{202E}fdp.exe";
    let ctrl = "esc\u{1b}[31mx";
    s.put(r.p("d/ok.txt"), b"ok").await.unwrap();
    s.put(r.p(&format!("d/{ruim}")), b"mau").await.unwrap();
    s.put(r.p(&format!("d/{ctrl}")), b"mau").await.unwrap();

    let l = r.browser.list_with_report(&r.p("d")).await.unwrap();
    let nomes: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(nomes, ["ok.txt"]);
    assert_eq!(l.hidden_unsafe.len(), 2, "{l:?}");
    for h in &l.hidden_unsafe {
        assert!(
            !h.chars().any(myssh_transfer::names::is_unsafe_char),
            "{h:?}"
        );
    }
    assert_eq!(r.browser.list(&r.p("d")).await.unwrap().len(), 1);

    let mut mgr = manager(&conn, 1);
    let dst = tmp();
    let run = run_simple(&mut mgr, req(Direction::Download, dst.path(), &r.p("d"))).await;
    assert_eq!(run.finished, Some((1, 2, 0)), "{run:#?}");
    assert!(
        run.errors
            .iter()
            .any(|(_, m)| m.contains("caracteres de controle ou invisíveis")),
        "{run:#?}"
    );
    let baixados: Vec<_> = std::fs::read_dir(dst.path().join("d"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(baixados, ["ok.txt"]);
    r.cleanup().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn download_publica_com_modo_padrao_do_usuario_e_sem_sobras() {
    use std::os::unix::fs::PermissionsExt;
    let conn = connect().await;
    let r = Remote::new(&conn, "modo").await;
    let s = rsftp(&conn).await;
    s.put(r.p("a.txt"), b"conteudo").await.unwrap();
    let mut mgr = manager(&conn, 1);
    let dst = tmp();
    let run = run_simple(
        &mut mgr,
        req(Direction::Download, dst.path(), &r.p("a.txt")),
    )
    .await;
    assert_eq!(run.finished, Some((1, 0, 0)), "{run:#?}");
    // Referência: arquivo criado normalmente pelo usuário (umask).
    let ref_ = dst.path().join("referencia");
    std::fs::write(&ref_, "x").unwrap();
    let esperado = std::fs::metadata(&ref_).unwrap().permissions().mode() & 0o777;
    let got = std::fs::metadata(dst.path().join("a.txt"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(got, esperado);
    let n = std::fs::read_dir(dst.path()).unwrap().count();
    assert_eq!(n, 2, "sem temporários/sidecars/sondas sobrando");
    r.cleanup().await;
}
