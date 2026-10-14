//! Testes de integração contra o servidor de teste (`./testenv/up.sh`).
//! Rodam com: `MYSSH_TEST_SSH=127.0.0.1:2222 cargo test -- --ignored`
//! (opcional: `MYSSH_TEST_KEYS=<dir das chaves>`; padrão `../../testenv/keys`).
//!
//! Usam o MESMO módulo de sessão/terminal do app, sem UI.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use poc_egui::known_hosts::fingerprint;
use poc_egui::session::{self, Auth, ConnectParams};
use poc_egui::terminal::{Status, TermSession};
use russh::keys::PublicKey;
use secrecy::SecretString;

fn target() -> (String, u16) {
    let v = std::env::var("MYSSH_TEST_SSH").expect("defina MYSSH_TEST_SSH=host:porta");
    let (h, p) = v.rsplit_once(':').expect("host:porta");
    (h.to_string(), p.parse().expect("porta"))
}

fn keys_dir() -> PathBuf {
    std::env::var_os("MYSSH_TEST_KEYS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/keys"))
}

/// Host key real do servidor, obtida FORA do app (`ssh-keyscan`) -> fingerprint esperado.
fn expected_fingerprint(host: &str, port: u16) -> String {
    let out = Command::new("ssh-keyscan")
        .args(["-p", &port.to_string(), "-t", "ed25519", host])
        .output()
        .expect("ssh-keyscan");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .expect("saída do ssh-keyscan");
    let key = line.split_once(' ').map(|x| x.1).expect("linha de chave");
    fingerprint(&PublicKey::from_openssh(key).expect("chave"))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("rt")
}

fn tmp_known_hosts(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("poc-egui-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d.join("known_hosts")
}

fn params(auth: Auth, pinned: Option<String>, known: Option<PathBuf>) -> ConnectParams {
    let (host, port) = target();
    ConnectParams {
        host,
        port,
        user: "tester".into(),
        auth,
        cols: 80,
        rows: 24,
        pix_w: 640,
        pix_h: 384,
        known_hosts: known,
        pinned_fingerprint: pinned,
    }
}

fn start(rt: &tokio::runtime::Runtime, p: ConnectParams) -> TermSession {
    let (cols, rows) = (p.cols, p.rows);
    let h = session::spawn(rt.handle(), p, Arc::new(|| {}));
    TermSession::new(h, cols, rows)
}

fn pw() -> Auth {
    Auth::Password(SecretString::from("tester-pass".to_string()))
}

/// Bombeia a sessão até `pred` ou timeout.
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
                "timeout esperando: {what}; status={:?}\n--- tela ---\n{}",
                s.status,
                s.screen_text()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn type_line(s: &TermSession, line: &str) {
    assert!(s.send_input(format!("{line}\r").into_bytes()));
}

fn connected(rt: &tokio::runtime::Runtime, auth: Auth) -> TermSession {
    let (h, p) = target();
    let mut s = start(rt, params(auth, Some(expected_fingerprint(&h, p)), None));
    wait_for(&mut s, "conectar", 15, |s| s.status == Status::Connected);
    // espera o prompt do shell
    wait_for(&mut s, "prompt", 10, |s| s.bytes_received > 0);
    std::thread::sleep(Duration::from_millis(500));
    s.pump(1 << 20);
    s
}

#[test]
#[ignore]
fn host_key_errada_e_rejeitada() {
    let rt = rt();
    // Fingerprint "esperado" de OUTRA chave (a pública do cliente).
    let other = std::fs::read_to_string(keys_dir().join("id_ed25519.pub")).expect("pub");
    let wrong = fingerprint(&PublicKey::from_openssh(other.trim()).expect("pub"));
    let mut s = start(&rt, params(pw(), Some(wrong), None));
    wait_for(&mut s, "fechar", 15, |s| {
        matches!(s.status, Status::Closed(_))
    });
    let Status::Closed(Some(e)) = &s.status else {
        panic!("esperava erro, veio {:?}", s.status)
    };
    assert!(e.contains("difere do esperado"), "{e}");
    println!("rejeição da chave errada: {e}");
}

#[test]
#[ignore]
fn prompt_aceitar_persistir_e_detectar_mudanca() {
    let rt = rt();
    let (h, p) = target();
    let real = expected_fingerprint(&h, p);
    let kh = tmp_known_hosts("prompt");

    // 1) host desconhecido -> prompt com o fingerprint real; rejeitar fecha a sessão.
    let mut s = start(&rt, params(pw(), None, Some(kh.clone())));
    wait_for(&mut s, "prompt", 15, |s| {
        s.status == Status::AwaitingHostKey
    });
    assert_eq!(s.prompt.as_ref().expect("prompt").fingerprint, real);
    s.answer_host_key(false);
    wait_for(&mut s, "fechar", 10, |s| {
        matches!(s.status, Status::Closed(_))
    });
    assert!(!kh.exists(), "rejeitar não pode gravar o host");

    // 2) aceitar -> conecta e grava.
    let mut s = start(&rt, params(pw(), None, Some(kh.clone())));
    wait_for(&mut s, "prompt", 15, |s| {
        s.status == Status::AwaitingHostKey
    });
    s.answer_host_key(true);
    wait_for(&mut s, "conectar", 15, |s| s.status == Status::Connected);
    drop(s);
    assert!(kh.exists());

    // 3) agora é confiável: conecta sem prompt.
    let mut s = start(&rt, params(pw(), None, Some(kh.clone())));
    wait_for(&mut s, "conectar sem prompt", 15, |s| {
        assert_ne!(s.status, Status::AwaitingHostKey, "não deveria perguntar");
        s.status == Status::Connected
    });
    drop(s);

    // 4) arquivo com OUTRA chave para o mesmo host:porta -> bloqueio explícito, sem prompt.
    let other = std::fs::read_to_string(keys_dir().join("id_ed25519.pub")).expect("pub");
    let mut parts = other.split_whitespace();
    let line = format!(
        "[{h}]:{p} {} {}\n",
        parts.next().unwrap(),
        parts.next().unwrap()
    );
    std::fs::write(&kh, line).expect("escrever known_hosts");
    let mut s = start(&rt, params(pw(), None, Some(kh)));
    wait_for(&mut s, "fechar", 15, |s| {
        assert_ne!(
            s.status,
            Status::AwaitingHostKey,
            "mudança de chave não pode oferecer aceitar"
        );
        matches!(s.status, Status::Closed(_))
    });
    let Status::Closed(Some(e)) = &s.status else {
        panic!("esperava erro")
    };
    assert!(e.contains("mudou"), "{e}");
    println!("bloqueio por chave diferente: {e}");
}

#[test]
#[ignore]
fn senha_errada_falha() {
    let rt = rt();
    let (h, p) = target();
    let bad = Auth::Password(SecretString::from("senha-errada".to_string()));
    let mut s = start(&rt, params(bad, Some(expected_fingerprint(&h, p)), None));
    wait_for(&mut s, "fechar", 15, |s| {
        matches!(s.status, Status::Closed(_))
    });
    assert!(matches!(&s.status, Status::Closed(Some(e)) if e.contains("autenticação")));
}

#[test]
#[ignore]
fn auth_por_chave_e_passphrase() {
    let rt = rt();
    let key = Auth::Key {
        path: keys_dir().join("id_ed25519"),
        passphrase: None,
    };
    let s = connected(&rt, key);
    assert_eq!(s.status, Status::Connected);
    drop(s);

    let key = Auth::Key {
        path: keys_dir().join("id_ed25519_pass"),
        passphrase: Some(SecretString::from("test-passphrase".to_string())),
    };
    let s = connected(&rt, key);
    assert_eq!(s.status, Status::Connected);
    drop(s);

    // passphrase errada
    let (h, p) = target();
    let key = Auth::Key {
        path: keys_dir().join("id_ed25519_pass"),
        passphrase: Some(SecretString::from("errada".to_string())),
    };
    let mut s = start(&rt, params(key, Some(expected_fingerprint(&h, p)), None));
    wait_for(&mut s, "fechar", 15, |s| {
        matches!(s.status, Status::Closed(_))
    });
    assert!(matches!(&s.status, Status::Closed(Some(e)) if e.contains("passphrase")));
}

#[test]
#[ignore]
fn pty_vim_top_resize_stty() {
    let rt = rt();
    let mut s = connected(&rt, pw());

    // PTY 80x24 e TERM
    type_line(&s, "stty size; echo TERM=$TERM");
    wait_for(&mut s, "stty inicial", 10, |s| {
        s.screen_text().contains("TERM=xterm-256color")
    });
    assert!(
        s.screen_lines().iter().any(|l| l.trim() == "24 80"),
        "{}",
        s.screen_text()
    );

    // vim -> tela alternativa com '~'
    type_line(&s, "vim");
    wait_for(&mut s, "vim em tela alternativa", 10, |s| s.is_alt_screen());
    wait_for(&mut s, "til no vim", 10, |s| {
        s.screen_lines()
            .iter()
            .filter(|l| l.starts_with('~'))
            .count()
            >= 10
    });
    println!("--- vim ---\n{}\n-----------", s.screen_text());
    assert!(s.send_input(b":q\r".to_vec()));
    wait_for(&mut s, "sair da tela alternativa", 10, |s| {
        !s.is_alt_screen()
    });

    // top interativo (alt screen) com texto esperado
    type_line(&s, "top");
    wait_for(&mut s, "top", 10, |s| {
        s.screen_text().to_lowercase().contains("load average")
    });
    println!("--- top ---\n{}\n-----------", s.screen_text());
    println!("top usa tela alternativa: {}", s.is_alt_screen());
    assert!(s.send_input(b"q".to_vec()));
    std::thread::sleep(Duration::from_millis(500));
    type_line(&s, "echo TOP_ENCERRADO");
    wait_for(&mut s, "sair do top", 10, |s| {
        s.screen_lines().iter().any(|l| l == "TOP_ENCERRADO")
    });

    // top em lote
    type_line(&s, "top -b -n1 | head -3");
    wait_for(&mut s, "top -b", 10, |s| s.screen_text().contains("Tasks:"));

    // resize 120x40 -> Term::resize + window_change; stty size no servidor
    s.resize(120, 40, (8, 16));
    assert_eq!((s.cols, s.rows), (120, 40));
    std::thread::sleep(Duration::from_millis(300));
    type_line(&s, "clear; stty size");
    wait_for(&mut s, "stty após resize", 10, |s| {
        s.screen_lines().iter().any(|l| l.trim() == "40 120")
    });

    // Respostas do Term (PtyWrite): DSR de posição do cursor volta ao servidor.
    type_line(
        &s,
        "printf '\\033[6n'; read -s -t 3 -d R x; echo DSR=${x#*[}",
    );
    wait_for(&mut s, "resposta DSR", 10, |s| {
        s.screen_lines()
            .iter()
            .any(|l| l.starts_with("DSR=") && l.len() > 6)
    });
    println!(
        "{}",
        s.screen_lines()
            .iter()
            .find(|l| l.starts_with("DSR="))
            .cloned()
            .unwrap_or_default()
    );
}

/// RSS (KiB) do processo atual.
fn rss_kib() -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

#[test]
#[ignore]
fn memoria_1_vs_10_sessoes() {
    let rt = rt();
    let base = rss_kib();
    let mut sessions = Vec::new();
    let mut rss_at = Vec::new();
    for n in 1..=10 {
        let mut s = connected(&rt, pw());
        type_line(&s, "top");
        wait_for(&mut s, "top", 10, |s| {
            s.screen_text().contains("load average")
        });
        sessions.push(s);
        if n == 1 || n == 10 {
            for s in sessions.iter_mut() {
                s.pump(1 << 20);
            }
            std::thread::sleep(Duration::from_secs(2));
            for s in sessions.iter_mut() {
                s.pump(1 << 20);
            }
            rss_at.push((n, rss_kib()));
        }
    }
    println!("RSS base (processo de teste, antes de sessões): {base} KiB");
    for (n, r) in &rss_at {
        println!("RSS com {n} sessão(ões) com top rodando: {r} KiB");
    }
    // 10 sessões ativas: todas continuam recebendo dados.
    assert!(sessions.iter().all(|s| s.status == Status::Connected));
}
