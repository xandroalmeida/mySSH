//! Testes de integração contra o servidor de teste (`./testenv/up.sh`, 127.0.0.1:2222).
//! `MYSSH_TEST_SSH=127.0.0.1:2222 cargo test -p myssh-ssh -- --ignored --nocapture --test-threads=1`
//! (opcional: `MYSSH_TEST_KEYS=<dir>`). Nunca desabilitam a validação de host key: o
//! fingerprint esperado vem do `ssh-keyscan`, fora do código sob teste.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustVerdict};
use myssh_ssh::{
    AuthMaterial, ConnectRequest, HostKeyDecision, PtySize, SessionCommand, SessionEvent,
    SessionHandle, host_key_from_public, spawn,
};
use myssh_store::Store;
use russh::keys::PublicKey;
use secrecy::SecretString;

const DEFAULT_KEYS: &str = "/Users/alexandro/Projetos/Pessoal/mySSH/testenv/keys";

fn target() -> (String, u16) {
    let v = std::env::var("MYSSH_TEST_SSH").expect("defina MYSSH_TEST_SSH=host:porta");
    let (h, p) = v.rsplit_once(':').expect("host:porta");
    (h.to_string(), p.parse().expect("porta"))
}

fn keys_dir() -> PathBuf {
    std::env::var_os("MYSSH_TEST_KEYS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_KEYS))
}

fn endpoint() -> HostEndpoint {
    let (h, p) = target();
    HostEndpoint::new(&h, p)
}

fn pub_host_key(file: &str) -> HostKey {
    let text = std::fs::read_to_string(keys_dir().join(file)).expect("pub");
    host_key_from_public(&PublicKey::from_openssh(text.trim()).expect("pub")).expect("hostkey")
}

/// Host key real do servidor, obtida FORA do código sob teste (uma vez por processo).
fn server_key() -> HostKey {
    static KEY: std::sync::OnceLock<HostKey> = std::sync::OnceLock::new();
    KEY.get_or_init(scan_server_key).clone()
}

fn scan_server_key() -> HostKey {
    let (h, p) = target();
    let out = Command::new("ssh-keyscan")
        .args(["-p", &p.to_string(), "-t", "ed25519", &h])
        .output()
        .expect("ssh-keyscan");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .expect("saída do ssh-keyscan");
    let key = line.split_once(' ').map(|x| x.1).expect("linha de chave");
    host_key_from_public(&PublicKey::from_openssh(key).expect("chave")).expect("hostkey")
}

/// O sshd do testenv (OpenSSH >= 9.8) aplica `PerSourcePenalties`: muitas conexões fechadas
/// antes de autenticar (justamente o que os testes de host key fazem) fazem o servidor recusar
/// o IP por alguns segundos. Espera a penalidade expirar antes de cada teste.
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
                let _ = server_key();
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    panic!("servidor de teste indisponível (penalidade do sshd?)");
}

fn pw() -> AuthMaterial {
    AuthMaterial::Password(SecretString::from("tester-pass".to_string()))
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        rows,
        pix_w: cols * 8,
        pix_h: rows * 16,
    }
}

fn request(ep: HostEndpoint, auth: AuthMaterial) -> ConnectRequest {
    ConnectRequest {
        endpoint: ep,
        username: "tester".into(),
        auth,
        pty: size(80, 24),
        term: "xterm-256color".into(),
        connect_timeout: Duration::from_secs(10),
        keepalive: Some(Duration::from_secs(15)),
    }
}

fn store_with(keys: &[HostKey]) -> Arc<Store> {
    let s = Arc::new(Store::open_in_memory().expect("store"));
    for k in keys {
        s.trust(&endpoint(), k).expect("trust");
    }
    s
}

/// Store que já confia na chave real do servidor.
fn trusting() -> Arc<Store> {
    store_with(&[server_key()])
}

fn start(req: ConnectRequest, trust: Arc<Store>) -> SessionHandle {
    spawn(
        &tokio::runtime::Handle::current(),
        req,
        trust,
        Arc::new(|| {}),
    )
}

async fn next(h: &mut SessionHandle, secs: u64) -> SessionEvent {
    tokio::time::timeout(Duration::from_secs(secs), h.evt_rx.recv())
        .await
        .expect("timeout esperando evento")
        .expect("canal de eventos fechado")
}

/// Consome eventos até `Closed` (ou até `stop` retornar true), acumulando estados e dados.
struct Run {
    states: Vec<SessionState>,
    data: Vec<u8>,
    closed: Option<Option<String>>,
}

async fn drain_until(h: &mut SessionHandle, secs: u64, mut stop: impl FnMut(&Run) -> bool) -> Run {
    let mut run = Run {
        states: vec![],
        data: vec![],
        closed: None,
    };
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) && !stop(&run) && run.closed.is_none() {
        match tokio::time::timeout(Duration::from_millis(200), h.evt_rx.recv()).await {
            Ok(Some(SessionEvent::State(s))) => run.states.push(s),
            Ok(Some(SessionEvent::Data(d))) => run.data.extend(d),
            Ok(Some(SessionEvent::Closed { error })) => run.closed = Some(error),
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }
    run
}

async fn connected(auth: AuthMaterial) -> SessionHandle {
    let mut h = start(request(endpoint(), auth), trusting());
    let run = drain_until(&mut h, 15, |r| r.states.contains(&SessionState::Connected)).await;
    assert!(
        run.states.contains(&SessionState::Connected),
        "{:?} {:?}",
        run.states,
        run.closed
    );
    h
}

fn text(d: &[u8]) -> String {
    String::from_utf8_lossy(d).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn host_desconhecido_aceitar_e_salvar() {
    ready().await;
    let server = server_key();
    let store = store_with(&[]);
    let mut h = start(request(endpoint(), pw()), store.clone());
    let mut states = vec![];
    let responder = loop {
        match next(&mut h, 15).await {
            SessionEvent::State(s) => states.push(s),
            SessionEvent::HostKeyUnknown {
                endpoint: ep,
                key,
                responder,
            } => {
                assert_eq!(ep, endpoint());
                assert_eq!(key.fingerprint_sha256, server.fingerprint_sha256);
                break responder;
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    };
    assert_eq!(
        states,
        [SessionState::Connecting, SessionState::AwaitingHostTrust]
    );
    // Antes da decisão nada foi salvo.
    assert_eq!(
        store.verify(&endpoint(), &server).unwrap(),
        TrustVerdict::Unknown
    );
    responder.answer(HostKeyDecision::AcceptAndSave);
    let run = drain_until(&mut h, 15, |r| r.states.contains(&SessionState::Connected)).await;
    assert_eq!(
        run.states,
        [SessionState::AwaitingAuth, SessionState::Connected]
    );
    assert_eq!(
        store.verify(&endpoint(), &server).unwrap(),
        TrustVerdict::Trusted
    );
    drop(h);

    // Segunda conexão: confiável, sem prompt.
    let mut h = start(request(endpoint(), pw()), store);
    let run = drain_until(&mut h, 15, |r| r.states.contains(&SessionState::Connected)).await;
    assert!(!run.states.contains(&SessionState::AwaitingHostTrust));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn host_desconhecido_rejeitado_ou_sem_resposta() {
    ready().await;
    let server = server_key();
    for decision in [Some(HostKeyDecision::Reject), None] {
        let store = store_with(&[]);
        let mut h = start(request(endpoint(), pw()), store.clone());
        loop {
            match next(&mut h, 15).await {
                SessionEvent::HostKeyUnknown { responder, .. } => {
                    match decision {
                        Some(d) => responder.answer(d),
                        None => drop(responder), // Drop sem resposta = Reject
                    }
                    break;
                }
                SessionEvent::State(_) => {}
                other => panic!("evento inesperado: {other:?}"),
            }
        }
        let run = drain_until(&mut h, 15, |_| false).await;
        assert!(
            !run.states.contains(&SessionState::AwaitingAuth),
            "{:?}",
            run.states
        );
        let err = run.closed.expect("Closed").expect("erro");
        assert!(err.contains("não confiável"), "{err}");
        assert_eq!(
            store.verify(&endpoint(), &server).unwrap(),
            TrustVerdict::Unknown
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn host_aceitar_uma_vez_nao_salva() {
    ready().await;
    let server = server_key();
    let store = store_with(&[]);
    let mut h = start(request(endpoint(), pw()), store.clone());
    loop {
        match next(&mut h, 15).await {
            SessionEvent::HostKeyUnknown { responder, .. } => {
                responder.answer(HostKeyDecision::AcceptOnce);
                break;
            }
            SessionEvent::State(_) => {}
            other => panic!("evento inesperado: {other:?}"),
        }
    }
    let run = drain_until(&mut h, 15, |r| r.states.contains(&SessionState::Connected)).await;
    assert!(run.states.contains(&SessionState::Connected));
    assert_eq!(
        store.verify(&endpoint(), &server).unwrap(),
        TrustVerdict::Unknown
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn chave_alterada_bloqueia_antes_da_autenticacao() {
    ready().await;
    // O store tem OUTRA chave ed25519 para o mesmo host:porta.
    let store = store_with(&[pub_host_key("id_ed25519.pub")]);
    let mut h = start(request(endpoint(), pw()), store);
    let mut states = vec![];
    let mut saw_changed = false;
    let mut closed = None;
    while closed.is_none() {
        match next(&mut h, 15).await {
            SessionEvent::State(s) => states.push(s),
            SessionEvent::HostKeyChanged {
                presented, known, ..
            } => {
                assert_eq!(
                    presented.fingerprint_sha256,
                    server_key().fingerprint_sha256
                );
                assert_eq!(known.len(), 1);
                assert_eq!(
                    known[0].fingerprint_sha256,
                    pub_host_key("id_ed25519.pub").fingerprint_sha256
                );
                saw_changed = true;
            }
            SessionEvent::HostKeyUnknown { .. } => {
                panic!("mudança de chave não pode oferecer aceitar")
            }
            SessionEvent::Closed { error } => closed = Some(error),
            SessionEvent::Data(_) => panic!("sem dados antes de autenticar"),
        }
    }
    assert!(saw_changed);
    assert!(!states.contains(&SessionState::AwaitingAuth), "{states:?}");
    assert!(!states.contains(&SessionState::Connected));
    assert!(closed.unwrap().expect("erro").contains("mudou"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn senha_certa_e_errada() {
    ready().await;
    let mut h = connected(pw()).await;
    h.cmd_tx
        .send(SessionCommand::Input(b"echo ola-$((6*7))\r".to_vec()))
        .await
        .unwrap();
    let run = drain_until(&mut h, 10, |r| text(&r.data).contains("ola-42\r\n")).await;
    assert!(text(&run.data).contains("ola-42"), "{}", text(&run.data));
    drop(h);

    let wrong = AuthMaterial::Password(SecretString::from("senha-errada-xyz".to_string()));
    let mut h = start(request(endpoint(), wrong), trusting());
    let run = drain_until(&mut h, 15, |_| false).await;
    let err = run.closed.expect("Closed").expect("erro");
    assert_eq!(err, "Autenticação recusada pelo servidor.");
    assert!(!err.contains("senha-errada-xyz"));
    assert!(matches!(run.states.last(), Some(SessionState::Error(m)) if *m == err));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn chave_com_e_sem_passphrase() {
    ready().await;
    let keys = keys_dir();
    let sem = AuthMaterial::KeyFile {
        path: keys.join("id_ed25519"),
        passphrase: None,
    };
    let h = connected(sem).await;
    drop(h);

    let com = AuthMaterial::KeyFile {
        path: keys.join("id_ed25519_pass"),
        passphrase: Some(SecretString::from("test-passphrase".to_string())),
    };
    let h = connected(com).await;
    drop(h);

    for passphrase in [None, Some(SecretString::from("errada".to_string()))] {
        let auth = AuthMaterial::KeyFile {
            path: keys.join("id_ed25519_pass"),
            passphrase,
        };
        let mut h = start(request(endpoint(), auth), trusting());
        let run = drain_until(&mut h, 15, |_| false).await;
        assert_eq!(
            run.closed.expect("Closed").expect("erro"),
            "Chave privada inválida ou passphrase incorreta."
        );
    }

    let ausente = AuthMaterial::KeyFile {
        path: keys.join("nao-existe"),
        passphrase: None,
    };
    let mut h = start(request(endpoint(), ausente), trusting());
    let run = drain_until(&mut h, 15, |_| false).await;
    assert!(run.closed.unwrap().unwrap().contains("ler o arquivo"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn keyboard_interactive_ainda_nao_suportado() {
    ready().await;
    let mut h = start(
        request(endpoint(), AuthMaterial::KeyboardInteractive),
        trusting(),
    );
    let run = drain_until(&mut h, 15, |_| false).await;
    assert!(
        run.closed
            .unwrap()
            .unwrap()
            .contains("ainda não é suportado")
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn pty_e_resize() {
    ready().await;
    let mut h = connected(pw()).await;
    // Deixa o prompt chegar.
    drain_until(&mut h, 3, |r| !r.data.is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    h.cmd_tx
        .send(SessionCommand::Input(b"stty size\r".to_vec()))
        .await
        .unwrap();
    let run = drain_until(&mut h, 10, |r| text(&r.data).contains("24 80\r\n")).await;
    assert!(text(&run.data).contains("24 80\r\n"), "{}", text(&run.data));

    h.cmd_tx
        .send(SessionCommand::Resize(size(100, 30)))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    h.cmd_tx
        .send(SessionCommand::Input(b"stty size\r".to_vec()))
        .await
        .unwrap();
    let run = drain_until(&mut h, 10, |r| text(&r.data).contains("30 100\r\n")).await;
    assert!(
        text(&run.data).contains("30 100\r\n"),
        "{}",
        text(&run.data)
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn desconexao_pelo_usuario_e_exit_remoto() {
    ready().await;
    let mut h = connected(pw()).await;
    h.cmd_tx.send(SessionCommand::Disconnect).await.unwrap();
    let run = drain_until(&mut h, 10, |_| false).await;
    assert_eq!(run.closed, Some(None));
    assert_eq!(run.states.last(), Some(&SessionState::Disconnected));

    let mut h = connected(pw()).await;
    drain_until(&mut h, 2, |r| !r.data.is_empty()).await;
    h.cmd_tx
        .send(SessionCommand::Input(b"exit\r".to_vec()))
        .await
        .unwrap();
    let run = drain_until(&mut h, 10, |r| r.closed.is_some()).await;
    assert_eq!(run.closed, Some(None), "exit limpo não é erro");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn timeout_porta_fechada_e_blackhole() {
    ready().await;
    // Porta fechada: erro imediato e claro.
    let t0 = Instant::now();
    let mut h = start(
        request(HostEndpoint::new("127.0.0.1", 1), pw()),
        store_with(&[]),
    );
    let run = drain_until(&mut h, 10, |_| false).await;
    let err = run.closed.unwrap().unwrap();
    println!("porta fechada: {err:?} em {:?}", t0.elapsed());
    assert!(err.contains("recusada"), "{err}");
    assert!(!err.contains("127.0.0.1"));

    // Blackhole: respeita connect_timeout (ou rede sem rota => inacessível, também rápido).
    let mut req = request(HostEndpoint::new("10.255.255.1", 22), pw());
    req.connect_timeout = Duration::from_secs(2);
    let t0 = Instant::now();
    let mut h = start(req, store_with(&[]));
    let run = drain_until(&mut h, 10, |_| false).await;
    let err = run.closed.unwrap().unwrap();
    let dt = t0.elapsed();
    println!("blackhole: {err:?} em {dt:?}");
    assert!(
        err == "Tempo esgotado ao conectar." || err.contains("inacessível"),
        "{err}"
    );
    assert!(dt < Duration::from_secs(4), "{dt:?}");
    assert!(!err.contains("10.255.255.1"));
}

/// Proxy TCP que pode "congelar" (para de encaminhar sem fechar): simula perda de rede.
async fn freezable_proxy(upstream: (String, u16)) -> (u16, Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let frozen = Arc::new(AtomicBool::new(false));
    let fz = frozen.clone();
    tokio::spawn(async move {
        let (mut c, _) = listener.accept().await.unwrap();
        let mut s = tokio::net::TcpStream::connect(upstream).await.unwrap();
        let (mut cr, mut cw) = c.split();
        let (mut sr, mut sw) = s.split();
        let f1 = fz.clone();
        let a = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = cr.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                if !f1.load(Ordering::Relaxed) && sw.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        };
        let b = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = sr.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                if !fz.load(Ordering::Relaxed) && cw.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        };
        tokio::select! { _ = a => {}, _ = b => {} }
    });
    (port, frozen)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perda_de_rede_vira_closed_com_erro() {
    ready().await;
    let (host, port) = target();
    let (pport, frozen) = freezable_proxy((host, port)).await;
    // Mesma chave confiável, mas sob o endpoint do proxy.
    let ep = HostEndpoint::new("127.0.0.1", pport);
    let store = Arc::new(Store::open_in_memory().unwrap());
    store.trust(&ep, &server_key()).unwrap();
    let mut req = request(ep, pw());
    req.keepalive = Some(Duration::from_secs(1));
    let mut h = start(req, store);
    let run = drain_until(&mut h, 15, |r| r.states.contains(&SessionState::Connected)).await;
    assert!(run.states.contains(&SessionState::Connected));
    frozen.store(true, std::sync::atomic::Ordering::Relaxed);
    let t0 = Instant::now();
    let run = drain_until(&mut h, 30, |_| false).await;
    let err = run.closed.expect("deveria fechar").expect("erro");
    println!("perda de rede detectada em {:?}: {err:?}", t0.elapsed());
    assert_eq!(err, "A conexão com o servidor foi perdida.");
    assert!(t0.elapsed() < Duration::from_secs(15));
}

fn rss_kb() -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn dez_sessoes_simultaneas() {
    ready().await;
    let rss0 = rss_kb();
    let trust = trusting();
    let t0 = Instant::now();
    let mut tasks = vec![];
    for i in 0..10 {
        let trust = trust.clone();
        tasks.push(tokio::spawn(async move {
            let t = Instant::now();
            let mut h = start(request(endpoint(), pw()), trust);
            let run =
                drain_until(&mut h, 30, |r| r.states.contains(&SessionState::Connected)).await;
            assert!(
                run.states.contains(&SessionState::Connected),
                "sessão {i}: {:?}",
                run.closed
            );
            let t_conn = t.elapsed();
            let marker = format!("m{i}-$((100+{i}))");
            h.cmd_tx
                .send(SessionCommand::Input(
                    format!("echo {marker}\r").into_bytes(),
                ))
                .await
                .unwrap();
            let want = format!("m{i}-{}\r\n", 100 + i);
            let run = drain_until(&mut h, 15, |r| text(&r.data).contains(&want)).await;
            assert!(
                text(&run.data).contains(&want),
                "sessão {i}: {}",
                text(&run.data)
            );
            (h, t_conn)
        }));
    }
    let mut handles = vec![];
    let mut conn_times = vec![];
    for t in tasks {
        let (h, c) = t.await.unwrap();
        handles.push(h);
        conn_times.push(c);
    }
    let total = t0.elapsed();
    let rss1 = rss_kb();
    conn_times.sort();
    println!(
        "10 sessões: total {:?}, conexão min {:?} / mediana {:?} / máx {:?}; RSS {} KB -> {} KB (delta {} KB, {} KB/sessão)",
        total,
        conn_times[0],
        conn_times[5],
        conn_times[9],
        rss0,
        rss1,
        rss1.saturating_sub(rss0),
        rss1.saturating_sub(rss0) / 10
    );
    drop(handles);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn autenticacao_por_agente() {
    ready().await;
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("myssh-agent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key = dir.join("k");
    std::fs::copy(keys_dir().join("id_ed25519"), &key).unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let sock = dir.join("agent.sock");
    let mut agent = Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&sock)
        .spawn()
        .expect("ssh-agent");
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let st = Command::new("ssh-add")
        .arg(&key)
        .env("SSH_AUTH_SOCK", &sock)
        .status()
        .expect("ssh-add");
    assert!(st.success());
    // SAFETY: único teste que lê/escreve SSH_AUTH_SOCK; rode com --test-threads=1.
    unsafe { std::env::set_var("SSH_AUTH_SOCK", &sock) };
    let h = connected(AuthMaterial::Agent).await;
    drop(h);
    unsafe { std::env::set_var("SSH_AUTH_SOCK", dir.join("inexistente")) };
    let mut h = start(request(endpoint(), AuthMaterial::Agent), trusting());
    let run = drain_until(&mut h, 15, |_| false).await;
    assert!(
        run.closed
            .unwrap()
            .unwrap()
            .contains("Agente SSH indisponível")
    );
    let _ = agent.kill();
    let _ = agent.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
