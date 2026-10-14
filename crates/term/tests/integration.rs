//! Terminal contra o servidor de teste real (`./testenv/up.sh`, 127.0.0.1:2222):
//! `MYSSH_TEST_SSH=127.0.0.1:2222 cargo test -p myssh-term -- --ignored --nocapture --test-threads=1`
//! Sem UI: usa o mesmo `TermSession` do widget. A host key é fixada via `ssh-keyscan` (fora do
//! código sob teste); nunca se desabilita a validação.

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::selection::SelectionType;
use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostTrustStore};
use myssh_ssh::{
    AuthMaterial, ConnectRequest, HostKeyDecision, PtySize, host_key_from_public, spawn,
};
use myssh_store::Store;
use myssh_term::TermSession;
use secrecy::SecretString;

fn target() -> (String, u16) {
    let v = std::env::var("MYSSH_TEST_SSH").expect("defina MYSSH_TEST_SSH=host:porta");
    let (h, p) = v.rsplit_once(':').expect("host:porta");
    (h.to_string(), p.parse().expect("porta"))
}

fn server_key() -> myssh_core::trust::HostKey {
    let (h, p) = target();
    let out = Command::new("ssh-keyscan")
        .args(["-p", &p.to_string(), "-t", "ed25519", &h])
        .output()
        .expect("ssh-keyscan");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .expect("ssh-keyscan");
    let key = line.split_once(' ').map(|x| x.1).expect("linha");
    host_key_from_public(&russh::keys::PublicKey::from_openssh(key).expect("chave")).expect("hk")
}

/// O sshd do testenv aplica `PerSourcePenalties`: espera a penalidade (por conexões fechadas
/// antes de autenticar) expirar antes de cada teste.
fn ready() {
    use std::io::Read;
    let (h, p) = target();
    for _ in 0..60 {
        if let Ok(mut s) = std::net::TcpStream::connect((h.as_str(), p)) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut b = [0u8; 4];
            if s.read_exact(&mut b).is_ok() && &b == b"SSH-" {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    panic!("servidor de teste indisponível (penalidade do sshd?)");
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("rt")
}

fn start(rt: &tokio::runtime::Runtime, trust: Arc<Store>) -> TermSession {
    let (h, p) = target();
    let req = ConnectRequest {
        endpoint: HostEndpoint::new(&h, p),
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
    };
    let handle = spawn(rt.handle(), req, trust, Arc::new(|| {}));
    TermSession::new(handle, 80, 24)
}

fn wait_for(
    s: &mut TermSession,
    what: &str,
    secs: u64,
    mut pred: impl FnMut(&TermSession) -> bool,
) {
    let t0 = Instant::now();
    loop {
        s.pump(1 << 20);
        if pred(s) {
            return;
        }
        if t0.elapsed() > Duration::from_secs(secs) {
            panic!(
                "timeout esperando: {what}; estado={:?} closed={:?}\n--- tela ---\n{}",
                s.state(),
                s.closed(),
                s.screen_text()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn type_line(s: &TermSession, line: &str) {
    assert!(s.send_input(format!("{line}\r").into_bytes()));
}

fn connected(rt: &tokio::runtime::Runtime) -> TermSession {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (h, p) = target();
    store
        .trust(&HostEndpoint::new(&h, p), &server_key())
        .unwrap();
    let mut s = start(rt, store);
    wait_for(&mut s, "conectar", 15, |s| {
        *s.state() == SessionState::Connected
    });
    wait_for(&mut s, "prompt", 10, |s| s.bytes_received() > 0);
    std::thread::sleep(Duration::from_millis(500));
    s.pump(1 << 20);
    s
}

#[test]
#[ignore]
fn prompt_de_host_key_pela_term_session() {
    ready();
    let rt = rt();
    let store = Arc::new(Store::open_in_memory().unwrap());
    let mut s = start(&rt, store.clone());
    wait_for(&mut s, "prompt", 15, |s| s.host_key_prompt().is_some());
    let fp = server_key().fingerprint_sha256;
    assert_eq!(s.host_key_prompt().unwrap().key.fingerprint_sha256, fp);
    assert_eq!(*s.state(), SessionState::AwaitingHostTrust);
    s.answer_host_key(HostKeyDecision::AcceptAndSave);
    wait_for(&mut s, "conectar", 15, |s| {
        *s.state() == SessionState::Connected
    });
    let (h, p) = target();
    assert_eq!(
        store
            .verify(&HostEndpoint::new(&h, p), &server_key())
            .unwrap(),
        myssh_core::trust::TrustVerdict::Trusted
    );
}

#[test]
#[ignore]
fn vim_top_resize_selecao_e_titulo() {
    ready();
    let rt = rt();
    let mut s = connected(&rt);

    type_line(&s, "stty size; echo TERM=$TERM");
    wait_for(&mut s, "stty inicial", 10, |s| {
        s.screen_text().contains("TERM=xterm-256color")
    });
    assert!(
        s.screen_lines().iter().any(|l| l.trim() == "24 80"),
        "{}",
        s.screen_text()
    );

    // Seleção -> texto sobre saída real.
    type_line(&s, "echo SELECIONE-ESTA-PALAVRA");
    wait_for(&mut s, "eco", 10, |s| {
        s.screen_lines()
            .iter()
            .any(|l| l == "SELECIONE-ESTA-PALAVRA")
    });
    let row = s
        .screen_lines()
        .iter()
        .position(|l| l == "SELECIONE-ESTA-PALAVRA")
        .unwrap() as i32;
    s.begin_selection(
        SelectionType::Semantic,
        Point::new(Line(row), Column(3)),
        Direction::Left,
    );
    assert_eq!(
        s.selection_text().as_deref(),
        Some("SELECIONE-ESTA-PALAVRA")
    );
    s.clear_selection();

    // Título remoto (OSC 0) vindo do servidor, sanitizado.
    type_line(&s, "printf '\\033]0;titulo-\\342\\200\\256remoto\\007'");
    wait_for(&mut s, "título", 10, |s| s.title().is_some());
    assert_eq!(s.title(), Some("titulo-remoto"));

    // vim -> tela alternativa
    type_line(&s, "vim");
    wait_for(&mut s, "vim em tela alternativa", 10, |s| s.is_alt_screen());
    wait_for(&mut s, "til no vim", 10, |s| {
        s.screen_lines()
            .iter()
            .filter(|l| l.starts_with('~'))
            .count()
            >= 10
    });
    assert!(s.send_input(b":q\r".to_vec()));
    wait_for(&mut s, "sair da tela alternativa", 10, |s| {
        !s.is_alt_screen()
    });

    // top interativo
    type_line(&s, "top");
    wait_for(&mut s, "top", 10, |s| {
        s.screen_text().to_lowercase().contains("load average")
    });
    assert!(s.send_input(b"q".to_vec()));
    std::thread::sleep(Duration::from_millis(500));
    type_line(&s, "echo TOP_ENCERRADO");
    wait_for(&mut s, "sair do top", 10, |s| {
        s.screen_lines().iter().any(|l| l == "TOP_ENCERRADO")
    });

    // resize 120x40 -> Term::resize + window_change
    s.resize(120, 40, (8, 16));
    assert_eq!(s.size(), (120, 40));
    std::thread::sleep(Duration::from_millis(300));
    type_line(&s, "clear; stty size");
    wait_for(&mut s, "stty após resize", 10, |s| {
        s.screen_lines().iter().any(|l| l.trim() == "40 120")
    });

    // DSR (resposta do Term ao servidor)
    type_line(
        &s,
        "printf '\\033[6n'; read -s -t 3 -d R x; echo DSR=${x#*[}",
    );
    wait_for(&mut s, "DSR", 10, |s| {
        s.screen_lines()
            .iter()
            .any(|l| l.starts_with("DSR=") && l.len() > 6)
    });

    // OSC 52 do servidor não pode disparar nada: o terminal segue funcionando e sem título novo.
    type_line(&s, "printf '\\033]52;c;aGVsbG8=\\007'; echo OSC52_OK");
    wait_for(&mut s, "osc52", 10, |s| {
        s.screen_lines().iter().any(|l| l == "OSC52_OK")
    });
}

#[test]
#[ignore]
fn colar_multilinha_e_scrollback() {
    ready();
    let rt = rt();
    let mut s = connected(&rt);
    s.set_confirm_multiline(true);
    let pending = s.paste("echo L1\necho L2");
    assert_eq!(pending.as_deref(), Some("echo L1\necho L2"));
    s.paste_confirmed("echo L1\necho L2");
    // bracketed paste (bash): o texto fica no editor de linha até o Enter do usuário
    std::thread::sleep(Duration::from_millis(300));
    assert!(s.send_input(b"\r".to_vec()));
    wait_for(&mut s, "L2", 10, |s| {
        s.screen_lines().iter().any(|l| l == "L2")
    });

    // 8000 linhas -> histórico limitado a 5000 (padrão)
    type_line(&s, "seq 1 8000");
    wait_for(&mut s, "seq", 20, |s| {
        s.screen_lines().iter().any(|l| l == "8000")
    });
    let h = s.history_len();
    println!("histórico após 8000 linhas: {h}");
    assert!((4000..=myssh_term::DEFAULT_SCROLLBACK).contains(&h));
}
