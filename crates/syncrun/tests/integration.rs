//! Integração do executor de sincronização contra o servidor de teste (`./testenv/up.sh`).
//!
//! ```text
//! MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys \
//!   cargo test -p myssh-syncrun -- --ignored --test-threads=1
//! ```
//! A host key esperada vem do `ssh-keyscan`; a validação nunca é desabilitada.

use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore};
use myssh_ssh::{
    AuthMaterial, ConnectRequest, Connection, EventSink, PtySize, Sftp, host_key_from_public,
};
use myssh_store::Store;
use myssh_sync::{Action, CompareMode, Direction, Op, Plan, PlanOptions, RelPath, Status};
use myssh_syncrun::{ConflictChoice, Decisions, SyncError, SyncEvent, SyncRunner, load_state};
use myssh_transfer::RemoteBrowser;
use russh::keys::PublicKey;
use secrecy::SecretString;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------- infraestrutura

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
    let (tx, mut rx) = mpsc::channel(32);
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

struct Env {
    conn: Arc<Connection>,
    sftp: Sftp,
    browser: RemoteBrowser,
    base: String,
    local: tempfile::TempDir,
}

impl Env {
    async fn new(name: &str) -> Env {
        let conn = connect().await;
        let sftp = conn.open_sftp().await.expect("sftp");
        let browser = RemoteBrowser::open(conn.clone()).await.expect("browser");
        let base = format!("/home/tester/sync-{name}");
        let _ = browser.remove(&base, true).await;
        browser.mkdir(&base).await.expect("mkdir base");
        Env {
            conn,
            sftp,
            browser,
            base,
            local: tempfile::tempdir().expect("tempdir"),
        }
    }

    /// Raiz local da sincronização (dentro de um diretório pai exclusivo do teste).
    fn root(&self) -> std::path::PathBuf {
        self.local.path().join("root")
    }

    fn rp(&self, rel: &str) -> String {
        format!("{}/root/{}", self.base, rel)
            .trim_end_matches('/')
            .to_string()
    }

    fn remote_root(&self) -> String {
        format!("{}/root", self.base)
    }

    async fn rput(&self, rel: &str, data: &[u8], mtime: u32) {
        let p = self.rp(rel);
        let mut f = self.sftp.create(p.clone()).await.expect("create");
        f.write_all(data).await.unwrap();
        f.shutdown().await.unwrap();
        let mut a = russh_sftp::protocol::FileAttributes::empty();
        a.atime = Some(mtime);
        a.mtime = Some(mtime);
        self.sftp.set_metadata(p, a).await.unwrap();
    }

    async fn rmkdir(&self, rel: &str) {
        self.sftp.create_dir(self.rp(rel)).await.expect("mkdir");
    }

    async fn rread(&self, rel: &str) -> Option<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut f = self.sftp.open(self.rp(rel)).await.ok()?;
        let mut v = Vec::new();
        f.read_to_end(&mut v).await.ok()?;
        Some(v)
    }

    async fn rexists(&self, rel: &str) -> bool {
        self.sftp.symlink_metadata(self.rp(rel)).await.is_ok()
    }

    async fn rnames(&self, rel: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .sftp
            .read_dir(self.rp(rel))
            .await
            .map(|rd| rd.map(|e| e.file_name()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    fn runner(&self, dir: Direction, f: impl FnOnce(&mut PlanOptions)) -> SyncRunner {
        let mut o = PlanOptions::new(dir);
        f(&mut o);
        SyncRunner::new(
            self.conn.clone(),
            self.root(),
            self.remote_root(),
            "perfil-teste",
            self.local.path().join("dados/journal.json"),
            o,
        )
        .concurrency(2)
    }

    async fn cleanup(self) {
        let _ = self.browser.remove(&self.base, true).await;
    }
}

fn lwrite(root: &Path, rel: &str, data: &[u8], mtime: u64) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, data).unwrap();
    set_mtime(&p, mtime);
}

fn set_mtime(path: &Path, secs: u64) {
    filetime::set_file_mtime(
        path,
        filetime::FileTime::from_system_time(UNIX_EPOCH + Duration::from_secs(secs)),
    )
    .unwrap();
}

fn rel(s: &str) -> RelPath {
    RelPath::parse(s).unwrap()
}

type Out = (Result<myssh_sync::Journal, SyncError>, Vec<SyncEvent>);

async fn run(r: &SyncRunner, plan: &Plan, d: &Decisions) -> Out {
    let (tx, mut rx) = mpsc::channel(100_000);
    let res = r.execute(plan, d, tx, CancellationToken::new()).await;
    let mut ev = Vec::new();
    while let Some(e) = rx.recv().await {
        ev.push(e);
    }
    (res, ev)
}

fn actionable(p: &Plan) -> usize {
    p.items
        .iter()
        .filter(|i| {
            matches!(
                i.action,
                Action::Create | Action::Update { .. } | Action::Delete
            )
        })
        .count()
}

fn failures(ev: &[SyncEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            SyncEvent::ItemFailed { path, message, .. } => Some(format!("{path}: {message}")),
            _ => None,
        })
        .collect()
}

const T0: u64 = 1_700_000_000;

// ---------------------------------------------------------------- testes

#[tokio::test]
#[ignore]
async fn local_para_remoto_com_unicode_e_espacos_e_idempotente() {
    let env = Env::new("up").await;
    let root = env.root();
    lwrite(
        &root,
        "pasta com espaço/ação ñ 日本.txt",
        "olá mundo".as_bytes(),
        T0,
    );
    lwrite(&root, "pasta com espaço/sub dir/b.bin", &[1, 2, 3, 4], T0);
    lwrite(&root, "raiz.txt", b"raiz", T0);
    std::fs::create_dir_all(root.join("vazio")).unwrap();
    // O destino remoto (raiz) ainda não existe: o executor a cria.
    let r = env.runner(Direction::LocalToRemote, |_| {});
    let plan = r.plan().await.unwrap();
    assert!(plan.summary().create >= 6, "{:?}", plan.summary());
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    let j = res.unwrap();
    assert!(j.is_complete(), "{:?} {:?}", j.items, failures(&ev));
    assert_eq!(
        env.rread("pasta com espaço/ação ñ 日本.txt").await.unwrap(),
        "olá mundo".as_bytes()
    );
    assert_eq!(
        env.rread("pasta com espaço/sub dir/b.bin").await.unwrap(),
        [1, 2, 3, 4]
    );
    assert!(env.rexists("vazio").await);
    assert!(matches!(ev.last(), Some(SyncEvent::Finished(s)) if s.failed == 0 && s.done >= 6));
    assert!(ev.iter().any(|e| matches!(e, SyncEvent::Progress { .. })));
    // Idempotente: o segundo plano não tem nada a fazer (mtime preservado).
    let p2 = r.plan().await.unwrap();
    assert_eq!(actionable(&p2), 0, "{:?}", p2.items);
    assert_eq!(p2.summary().conflict, 0);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn remoto_para_local_com_unicode_e_espacos() {
    let env = Env::new("down").await;
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rmkdir("dir com espaço").await;
    env.rput("dir com espaço/çã é.txt", "conteúdo".as_bytes(), T0 as u32)
        .await;
    env.rput("topo.txt", b"topo", T0 as u32).await;
    let r = env.runner(Direction::RemoteToLocal, |_| {});
    let plan = r.plan().await.unwrap();
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete(), "{:?}", failures(&ev));
    assert_eq!(
        std::fs::read(env.root().join("dir com espaço/çã é.txt")).unwrap(),
        "conteúdo".as_bytes()
    );
    assert_eq!(std::fs::read(env.root().join("topo.txt")).unwrap(), b"topo");
    assert_eq!(actionable(&r.plan().await.unwrap()), 0);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn padrao_nunca_apaga_e_espelho_exige_confirmacao() {
    let env = Env::new("del").await;
    let root = env.root();
    lwrite(&root, "novo.txt", b"novo", T0);
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("extra.txt", b"extra", T0 as u32).await;
    env.rmkdir("extradir").await;
    env.rput("extradir/x.txt", b"x", T0 as u32).await;

    // 1) Padrão: nada de Delete no plano; o extra permanece.
    let r = env.runner(Direction::LocalToRemote, |_| {});
    let plan = r.plan().await.unwrap();
    assert_eq!(plan.summary().delete, 0);
    let (res, _) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete());
    assert!(env.rexists("extra.txt").await && env.rexists("extradir/x.txt").await);
    assert!(env.rexists("novo.txt").await);

    // 2) Espelho sem confirmação: planeja o delete, mas não executa.
    let r = env.runner(Direction::LocalToRemote, |o| o.mirror_deletes = true);
    let plan = r.plan().await.unwrap();
    assert_eq!(plan.summary().delete, 3);
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    let j = res.unwrap();
    assert!(env.rexists("extra.txt").await && env.rexists("extradir/x.txt").await);
    assert!(!j.is_complete());
    assert!(j.items.iter().all(|i| i.status == Status::Pending));
    assert!(matches!(ev.last(), Some(SyncEvent::Finished(s)) if s.pending == 3 && s.done == 0));

    // 3) Confirmado: apaga só o que estava no plano; um arquivo que surgiu depois fica.
    env.rput("surgiu-depois.txt", b"s", T0 as u32).await;
    let d = Decisions {
        confirm_deletes: true,
        ..Default::default()
    };
    let (res, ev) = run(&r, &plan, &d).await;
    let j = res.unwrap();
    assert!(j.is_complete(), "{:?}", failures(&ev));
    assert!(!env.rexists("extra.txt").await);
    assert!(!env.rexists("extradir").await);
    assert!(
        env.rexists("surgiu-depois.txt").await,
        "fora do plano: não pode sumir"
    );
    assert!(env.rexists("novo.txt").await);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn alteracao_no_destino_entre_plano_e_execucao_vira_plan_stale() {
    let env = Env::new("stale").await;
    let root = env.root();
    lwrite(&root, "a.txt", b"versao nova maior", T0 + 100);
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("a.txt", b"velha", T0 as u32).await;
    let r = env.runner(Direction::LocalToRemote, |_| {});
    let plan = r.plan().await.unwrap();
    assert_eq!(plan.summary().update, 1);
    // Alguém altera o destino depois do preview.
    env.rput("a.txt", b"alterada por outro", T0 as u32 + 50)
        .await;
    let (res, _) = run(&r, &plan, &Decisions::default()).await;
    match res {
        Err(SyncError::PlanStale(c)) => assert_eq!(c.len(), 1),
        other => panic!("esperava PlanStale, veio {other:?}"),
    }
    assert_eq!(env.rread("a.txt").await.unwrap(), b"alterada por outro");
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn alterado_entre_revalidacao_e_delete_nao_apaga() {
    let env = Env::new("race").await;
    // Remoto → local; o destino é o disco local, onde o gancho consegue mexer.
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("fica.txt", b"fica", T0 as u32).await;
    let root = env.root();
    lwrite(&root, "fica.txt", b"fica", T0);
    lwrite(&root, "sobra-alterada.txt", b"original", T0);
    lwrite(&root, "sobra-intacta.txt", b"intacta", T0);
    let mut r = env.runner(Direction::RemoteToLocal, |o| o.mirror_deletes = true);
    let plan = r.plan().await.unwrap();
    assert_eq!(plan.summary().delete, 2);
    let target = root.join("sobra-alterada.txt");
    r.set_before_delete_hook(move |p| {
        if p.as_str() == "sobra-alterada.txt" {
            std::fs::write(&target, b"o usuario mexeu agora, tamanho diferente").unwrap();
        }
    });
    let d = Decisions {
        confirm_deletes: true,
        ..Default::default()
    };
    let (res, ev) = run(&r, &plan, &d).await;
    let j = res.unwrap();
    assert!(
        root.join("sobra-alterada.txt").exists(),
        "não pode apagar o alterado"
    );
    assert!(!root.join("sobra-intacta.txt").exists());
    assert!(matches!(
        j.status(&rel("sobra-alterada.txt"), Op::Delete),
        Some(Status::Failed(m)) if m.contains("alterado após o preview")
    ));
    assert_eq!(failures(&ev).len(), 1);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn git_excluido_por_padrao() {
    let env = Env::new("git").await;
    let root = env.root();
    lwrite(&root, "src/main.rs", b"fn main(){}", T0);
    lwrite(&root, ".git/config", b"segredo", T0);
    lwrite(&root, "sub/.git/HEAD", b"ref", T0);
    let r = env.runner(Direction::LocalToRemote, |_| {});
    let plan = r.plan().await.unwrap();
    let (res, _) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete());
    assert!(env.rexists("src/main.rs").await);
    assert!(!env.rexists(".git").await && !env.rexists("sub/.git").await);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn symlink_remoto_nao_e_seguido() {
    let env = Env::new("sym").await;
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rmkdir("real").await;
    env.rput("real/dentro.txt", b"dentro", T0 as u32).await;
    env.sftp
        .symlink(env.rp("real"), env.rp("link-dir"))
        .await
        .unwrap();
    env.sftp
        .symlink("/etc/hostname".to_string(), env.rp("link-arq"))
        .await
        .unwrap();
    let r = env.runner(Direction::RemoteToLocal, |_| {});
    let plan = r.plan().await.unwrap();
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete(), "{:?}", failures(&ev));
    assert!(env.root().join("real/dentro.txt").exists());
    assert!(!env.root().join("link-dir").exists());
    assert!(!env.root().join("link-arq").exists());
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn comparacao_por_conteudo_detecta_mesmo_tamanho_e_mtime() {
    let env = Env::new("hash").await;
    let root = env.root();
    lwrite(&root, "igual.txt", b"AAAA", T0);
    lwrite(&root, "difere.txt", b"AAAB", T0);
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("igual.txt", b"AAAA", T0 as u32).await;
    env.rput("difere.txt", b"AAAA", T0 as u32).await;

    // Tamanho + mtime considera tudo igual (limitação conhecida).
    let r = env.runner(Direction::LocalToRemote, |_| {});
    assert_eq!(actionable(&r.plan().await.unwrap()), 0);

    let r = env.runner(Direction::LocalToRemote, |o| {
        o.compare = CompareMode::Content
    });
    let (tx, mut rx) = mpsc::channel(100);
    let plan = r.plan_with_events(Some(&tx)).await.unwrap();
    drop(tx);
    let mut hashing = 0;
    while let Some(e) = rx.recv().await {
        if matches!(e, SyncEvent::Hashing { total: 2, .. }) {
            hashing += 1;
        }
    }
    assert_eq!(hashing, 2);
    assert_eq!(plan.summary().update, 1);
    let it = plan
        .items
        .iter()
        .find(|i| matches!(i.action, Action::Update { .. }))
        .unwrap();
    assert_eq!(it.path.as_str(), "difere.txt");
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete(), "{:?}", failures(&ev));
    assert_eq!(env.rread("difere.txt").await.unwrap(), b"AAAB");
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn conflito_so_executa_com_decisao_explicita() {
    let env = Env::new("conf").await;
    let root = env.root();
    lwrite(&root, "c.txt", b"origem", T0);
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("c.txt", b"destin", T0 as u32 + 1000).await; // destino mais novo, mesmo tamanho
    let r = env.runner(Direction::LocalToRemote, |_| {});
    let plan = r.plan().await.unwrap();
    assert_eq!(plan.summary().conflict, 1);
    let (res, _) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.unwrap().is_complete()); // nada no journal
    assert_eq!(env.rread("c.txt").await.unwrap(), b"destin");
    let mut d = Decisions::default();
    d.conflicts.insert(rel("c.txt"), ConflictChoice::Skip);
    let (res, _) = run(&r, &plan, &d).await;
    assert!(res.unwrap().is_complete());
    assert_eq!(env.rread("c.txt").await.unwrap(), b"destin");
    d.conflicts.insert(rel("c.txt"), ConflictChoice::UseSource);
    let (res, ev) = run(&r, &plan, &d).await;
    assert!(res.unwrap().is_complete(), "{:?}", failures(&ev));
    assert_eq!(env.rread("c.txt").await.unwrap(), b"origem");
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn cancelamento_no_meio_e_resume_completa() {
    let env = Env::new("resume").await;
    let root = env.root();
    for i in 0..40 {
        lwrite(
            &root,
            &format!("d/f{i:02}.txt"),
            format!("conteúdo {i}").as_bytes(),
            T0,
        );
    }
    let r = env.runner(Direction::LocalToRemote, |_| {}).concurrency(1);
    let plan = r.plan().await.unwrap();
    let (tx, mut rx) = mpsc::channel(100_000);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    let watcher = tokio::spawn(async move {
        let mut dones = 0;
        let mut all = Vec::new();
        while let Some(e) = rx.recv().await {
            if matches!(e, SyncEvent::ItemDone { .. }) {
                dones += 1;
                if dones == 5 {
                    c2.cancel();
                }
            }
            all.push(e);
        }
        all
    });
    let j = r
        .execute(&plan, &Decisions::default(), tx, cancel)
        .await
        .unwrap();
    let ev = watcher.await.unwrap();
    assert!(!j.is_complete());
    let pend = j
        .items
        .iter()
        .filter(|i| i.status == Status::Pending)
        .count();
    assert!(pend > 0 && pend < 41, "pendentes: {pend}");
    assert!(matches!(ev.last(), Some(SyncEvent::Finished(s)) if s.cancelled && s.pending == pend));
    assert!(
        !env.rnames("d").await.iter().any(|n| n.contains("myssh-")),
        "sem sobras de temporário"
    );

    // Retoma a partir do arquivo gravado em disco.
    let st = load_state(&env.local.path().join("dados/journal.json"))
        .await
        .unwrap();
    assert_eq!(st.profile_id, "perfil-teste");
    let text = std::fs::read_to_string(env.local.path().join("dados/journal.json")).unwrap();
    assert!(
        !text.contains("127.0.0.1") && !text.contains("tester"),
        "journal sem host/usuário"
    );
    let (tx, mut rx) = mpsc::channel(100_000);
    let j2 = r
        .resume(
            &st.plan,
            &Decisions::default(),
            st.journal,
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let mut ev2 = Vec::new();
    while let Some(e) = rx.recv().await {
        ev2.push(e);
    }
    assert!(j2.is_complete(), "{:?}", failures(&ev2));
    assert_eq!(env.rnames("d").await.len(), 40);
    assert_eq!(actionable(&r.plan().await.unwrap()), 0);
    env.cleanup().await;
}

#[tokio::test]
#[ignore]
async fn nome_remoto_malicioso_nao_escapa_da_raiz_local() {
    let env = Env::new("evil").await;
    env.sftp.create_dir(env.remote_root()).await.unwrap();
    env.rput("bom.txt", b"bom", T0 as u32).await;
    env.rput("..\\x", b"mau1", T0 as u32).await;
    env.rput("a\\b.txt", b"mau2", T0 as u32).await;
    env.rput("C:\\win.txt", b"mau3", T0 as u32).await;
    env.rmkdir("sub").await;
    env.rput("sub/..\\..\\y", b"mau4", T0 as u32).await;
    let r = env.runner(Direction::RemoteToLocal, |_| {});
    let plan = r.plan().await.unwrap();
    let (res, ev) = run(&r, &plan, &Decisions::default()).await;
    assert!(res.is_ok(), "{:?}", failures(&ev));
    // Nada fora da raiz (o pai é o tempdir, que só deve conter "root").
    let parent: Vec<String> = std::fs::read_dir(env.local.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        parent.iter().all(|n| n == "root" || n == "dados"),
        "{parent:?}"
    );
    let in_root: Vec<String> = std::fs::read_dir(env.root())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(in_root.contains(&"bom.txt".to_string()));
    assert!(
        !in_root.iter().any(|n| n.contains('\\')),
        "nomes com barra invertida não são criados: {in_root:?}"
    );
    assert_eq!(
        std::fs::read_dir(env.root().join("sub")).unwrap().count(),
        0
    );
    env.cleanup().await;
}
