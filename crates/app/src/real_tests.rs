//! Testes de UI headless (egui_kittest) das abas de terminal.
//!
//! * Os testes sem `#[ignore]` não usam rede (salto recusado, cofre bloqueado,
//!   conexão recusada).
//! * Os `#[ignore]` usam o servidor real do `testenv` (`./testenv/up.sh`, 127.0.0.1:2222):
//!
//!   ```text
//!   MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=/caminho/testenv/keys \
//!     cargo test -p myssh-app real_ -- --ignored --nocapture --test-threads=1
//!   ```
//!
//!   Os testes de jump host também usam o container `myssh-testsshd-target` (só alcançável a
//!   partir do bastion; ver `testenv/up.sh`).
//!
//!   `--test-threads=1` é obrigatório por causa do `PerSourcePenalties` do sshd. A host key
//!   do servidor é obtida com `ssh-keyscan` (fora do código sob teste) e nunca se desliga a
//!   validação.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Event, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore};
use myssh_core::{AuthMethod, Credential, CredentialId, CredentialKind, Profile, ProfileId};
use myssh_secrets::{ExposeSecret, MemoryStore, SecretString, Vault, new_secret_ref};
use myssh_store::Store;

use crate::app::{App, Dialog, VaultMode};
use crate::i18n;

const SECRETS: [&str; 4] = [
    "tester-pass",
    "test-passphrase",
    "ki-pass",
    "mfa-code-123456",
];

// ------------------------------------------------------------------ infraestrutura

pub(crate) fn runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    )
}

pub(crate) fn make_app(store: Store) -> App {
    let mut app = App::new(
        runtime(),
        store,
        Arc::new(Vault::new(Arc::new(MemoryStore::new()))),
        VaultMode::SessionOnly,
        None,
        None,
    );
    app.auto_connect = true;
    app
}

pub(crate) fn harness(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(move |_| app)
}

pub(crate) fn target() -> (String, u16) {
    let v = std::env::var("MYSSH_TEST_SSH").expect("defina MYSSH_TEST_SSH=host:porta");
    let (h, p) = v.rsplit_once(':').expect("host:porta");
    (h.to_string(), p.parse().expect("porta"))
}

pub(crate) fn keys_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("MYSSH_TEST_KEYS").expect("defina MYSSH_TEST_KEYS=.../testenv/keys"),
    )
}

/// Chave do servidor fixada fora do código sob teste.
fn server_key() -> HostKey {
    let (h, p) = target();
    let out = Command::new("ssh-keyscan")
        .args(["-p", &p.to_string(), "-t", "ed25519", &h])
        .output()
        .expect("ssh-keyscan");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .expect("ssh-keyscan sem saída");
    let mut it = line.split_whitespace().skip(1);
    let (algorithm, blob) = (it.next().unwrap(), it.next().unwrap());
    // fingerprint só para exibição/comparação nos testes (o store compara a chave).
    let fp = Command::new("sh")
        .arg("-c")
        .arg(format!("echo '{line}' | ssh-keygen -lf - | cut -d' ' -f2"))
        .output()
        .expect("ssh-keygen");
    HostKey {
        algorithm: algorithm.into(),
        key_base64: blob.into(),
        fingerprint_sha256: String::from_utf8_lossy(&fp.stdout).trim().to_string(),
    }
}

const TARGET_CONTAINER: &str = "myssh-testsshd-target";

/// Host key do container-destino (lida de dentro do container, fora do código sob teste).
fn target_key() -> HostKey {
    let out = Command::new("docker")
        .args([
            "exec",
            TARGET_CONTAINER,
            "cat",
            "/etc/ssh/ssh_host_ed25519_key.pub",
        ])
        .output()
        .expect("docker exec");
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut it = line.split_whitespace();
    let (algorithm, blob) = (it.next().unwrap(), it.next().unwrap());
    let fp = Command::new("sh")
        .arg("-c")
        .arg(format!("echo '{line}' | ssh-keygen -lf - | cut -d' ' -f2"))
        .output()
        .expect("ssh-keygen");
    HostKey {
        algorithm: algorithm.into(),
        key_base64: blob.into(),
        fingerprint_sha256: String::from_utf8_lossy(&fp.stdout).trim().to_string(),
    }
}

/// Perfil do destino do jump host (alcançável só pelo bastion), com `jump` = bastion.
fn target_profile(name: &str, jump: ProfileId, auth: AuthMethod) -> Profile {
    let mut pr = Profile::new(name, TARGET_CONTAINER, "tester");
    pr.auth = auth;
    pr.jump = Some(jump);
    pr.connect_timeout_secs = 20;
    pr
}

fn trust_target(app: &App) {
    app.store
        .trust(&HostEndpoint::new(TARGET_CONTAINER, 22), &target_key())
        .unwrap();
}

/// O sshd do testenv aplica `PerSourcePenalties` (falha de autenticação e conexões que fecham
/// antes de autenticar somam penalidade que decai com o tempo): testes que as provocam esperam
/// no fim, para não afetar o seguinte.
pub(crate) fn cooldown() {
    std::thread::sleep(Duration::from_secs(12));
}

/// Espera o servidor voltar a aceitar conexões.
pub(crate) fn ready() {
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

pub(crate) fn profile(name: &str, auth: AuthMethod) -> Profile {
    let (h, p) = target();
    let mut pr = Profile::new(name, h, "tester");
    pr.port = p;
    pr.auth = auth;
    pr.connect_timeout_secs = 20;
    pr
}

pub(crate) fn add_profile(app: &mut App, p: &Profile) {
    app.store.create_profile(p).unwrap();
    app.reload();
}

fn saved_password(app: &App, password: &str) -> CredentialId {
    let sref = new_secret_ref();
    app.vault
        .put(&sref, SecretString::from(password.to_string()))
        .unwrap();
    let cred = Credential {
        id: CredentialId::new(),
        label: "teste".into(),
        kind: CredentialKind::Password,
        secret: sref,
    };
    app.store.create_credential(&cred).unwrap();
    cred.id
}

pub(crate) fn trust_server(app: &App) {
    let (h, p) = target();
    app.store
        .trust(&HostEndpoint::new(&h, p), &server_key())
        .unwrap();
}

/// Avança frames até `cond` (as tarefas de rede reagem por canais).
pub(crate) fn wait(
    h: &mut Harness<'static, App>,
    what: &str,
    secs: u64,
    cond: impl Fn(&App) -> bool,
) {
    let t0 = Instant::now();
    while !cond(h.state()) {
        if t0.elapsed() > Duration::from_secs(secs) {
            panic!("timeout esperando: {what}\n{}", dump(h.state()));
        }
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(2);
}

pub(crate) fn dump(app: &App) -> String {
    app.tabs
        .iter()
        .map(|t| {
            format!(
                "[{}] {:?} modal={} tela:\n{}",
                t.body.title(),
                t.body.status().map(|s| s.text),
                t.body.needs_modal(),
                t.body.debug_screen()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn status(app: &App, idx: usize) -> &'static str {
    app.tabs[idx].body.status().map_or("", |s| s.text)
}

pub(crate) fn screen(app: &App, idx: usize) -> String {
    app.tabs[idx].body.debug_screen()
}

pub(crate) fn click(h: &mut Harness<'static, App>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn type_password(h: &mut Harness<'static, App>, text: &str) {
    let field = h.get_by_role(egui::accesskit::Role::PasswordInput);
    field.focus();
    h.run_steps(2);
    h.get_by_role(egui::accesskit::Role::PasswordInput)
        .type_text(text);
    h.run_steps(2);
}

/// Digita uma linha no terminal ativo (eventos de teclado reais do egui).
pub(crate) fn type_line(h: &mut Harness<'static, App>, line: &str) {
    h.event(Event::Text(line.to_string()));
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
}

fn assert_no_secrets(what: &str, text: &str) {
    for s in SECRETS {
        assert!(!text.contains(s), "segredo vazou em {what}");
    }
}

fn assert_clean(app: &App) {
    assert_no_secrets("perfis", &format!("{:?}", app.profiles));
    assert_no_secrets(
        "credenciais",
        &format!("{:?}", app.store.list_credentials().unwrap()),
    );
    assert_no_secrets("aviso", &format!("{:?}", app.notice));
    for t in &app.tabs {
        assert_no_secrets(
            "título/estado",
            &format!("{} {:?}", t.body.title(), t.body.status()),
        );
    }
}

/// Procura o segredo nos bytes de todos os arquivos do diretório de dados (inclui o WAL).
fn assert_no_secrets_on_disk(dir: &std::path::Path) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let bytes = std::fs::read(e.path()).unwrap_or_default();
        for s in SECRETS {
            assert!(
                !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
                "segredo gravado em {}",
                e.path().display()
            );
        }
    }
}

fn rss_mb() -> f64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

pub(crate) fn percentile(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

// ------------------------------------------------------------------ sem rede

#[test]
fn jump_host_recusado_indica_o_salto_no_erro() {
    let mut app = make_app(Store::open_in_memory().unwrap());
    let mut a = Profile::new("bastion-a", "127.0.0.1", "u");
    a.port = 1; // porta fechada
    a.auth = AuthMethod::Agent; // sem pergunta de senha
    add_profile(&mut app, &a);
    let mut b = Profile::new("b", "127.0.0.1", "u");
    b.port = 2; // endpoints distintos (o mesmo host:porta duas vezes na cadeia é ciclo)
    b.auth = AuthMethod::Agent;
    b.jump = Some(a.id);
    add_profile(&mut app, &b);
    app.open_profile(b.id);
    let mut h = harness(app);
    // O estado "erro" chega antes do evento `Closed` com a mensagem: espera os dois.
    wait(&mut h, "erro do salto", 15, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    assert!(h.state().tabs[1].body.is_closed());
    // O erro vem do salto 1 (e não de uma conexão direta ao destino ignorando o salto).
    assert!(
        screen_or_error(h.state(), 1).contains("Jump host 1: Conexão recusada"),
        "{}",
        dump(h.state())
    );
}

fn screen_or_error(app: &App, idx: usize) -> String {
    app.tabs[idx].body.debug_screen()
}

#[test]
fn keyboard_interactive_sem_servidor_vai_ate_a_rede() {
    // Antes (M1) KI era recusado na UI; agora a conexão é tentada (aqui: porta fechada).
    let mut app = make_app(Store::open_in_memory().unwrap());
    let mut p = Profile::new("ki", "127.0.0.1", "u");
    p.port = 1;
    p.auth = AuthMethod::KeyboardInteractive;
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);
    // O estado "erro" chega antes do evento `Closed` com a mensagem: espera os dois.
    wait(&mut h, "erro de rede", 15, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    assert!(screen_or_error(h.state(), 1).contains("recusada"));
}

#[test]
fn cofre_bloqueado_pede_desbloqueio_e_depois_conecta() {
    let mut app = make_app(Store::open_in_memory().unwrap());
    let mut p = Profile::new("fechado", "127.0.0.1", "u");
    p.port = 1; // porta fechada: o resultado esperado é "conexão recusada"
    let cid = {
        let sref = new_secret_ref();
        app.vault
            .put(&sref, SecretString::from("x".to_string()))
            .unwrap();
        let cred = Credential {
            id: CredentialId::new(),
            label: "t".into(),
            kind: CredentialKind::Password,
            secret: sref,
        };
        app.store.create_credential(&cred).unwrap();
        cred.id
    };
    p.auth = AuthMethod::Password {
        credential: Some(cid),
    };
    add_profile(&mut app, &p);
    app.vault.lock();
    app.open_profile(p.id);
    let mut h = harness(app);
    wait(&mut h, "modal do cofre", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    h.get_by_label(i18n::VAULT_LOCKED_TITLE());
    click(&mut h, i18n::UNLOCK_VAULT());
    // O estado "erro" chega antes do evento `Closed` com a mensagem: espera os dois.
    wait(&mut h, "erro de conexão", 10, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    h.get_by_label_contains("Conexão recusada");
    assert!(!h.state().vault.is_locked());
}

#[test]
fn fechar_aba_ocupada_pede_confirmacao_e_sair_desconecta() {
    // aba "ocupada" simulada: a sessão real é coberta nos testes com servidor
    let mut app = make_app(Store::open_in_memory().unwrap());
    let p = Profile::new("x", "127.0.0.1", "u");
    add_profile(&mut app, &p);
    app.auto_connect = false;
    app.open_profile(p.id);
    let id = app.active.unwrap();
    app.tabs.last_mut().unwrap().body.simulate_busy(true);
    app.request_close_tab(id);
    assert!(matches!(app.dialog, Dialog::ConfirmCloseTab(x) if x == id));
    app.close_all_tabs();
    assert!(app.tabs.iter().all(|t| t.body.is_closed()));
}

// ------------------------------------------------------------------ servidor real

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_senha_perguntar_host_desconhecido_comando_e_reconectar() {
    ready();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("myssh.db")).unwrap();
    let mut app = make_app(store);
    let p = profile("srv", AuthMethod::Password { credential: None });
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);

    // 1. pergunta a senha (modal); o campo é de senha e o segredo não aparece em nenhum rótulo
    wait(&mut h, "pedido de senha", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    h.get_by_label_contains("Senha de tester@");
    type_password(&mut h, "tester-pass");
    let node_value = h
        .get_by_role(egui::accesskit::Role::PasswordInput)
        .value()
        .unwrap_or_default();
    assert_no_secrets("valor acessível do campo de senha", &node_value);
    click(&mut h, i18n::CONNECT());

    // 2. host desconhecido: modal com fingerprint; "Aceitar e salvar"
    wait(&mut h, "host key desconhecida", 15, |a| {
        status(a, 1) == i18n::ST_AWAITING_TRUST() && a.tabs[1].body.needs_modal()
    });
    h.get_by_label(i18n::HK_FINGERPRINT());
    h.get_by_label_contains("SHA256:");
    // Rejeitar/aceitar existem; "Rejeitar" é o foco inicial
    assert!(h.get_by_label(i18n::HK_REJECT()).is_focused());
    click(&mut h, i18n::HK_ACCEPT_SAVE());

    // 3. conectado, prompt no terminal, comando
    wait(&mut h, "conectado", 15, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    wait(&mut h, "prompt do shell", 15, |a| {
        !screen(a, 1).trim().is_empty()
    });
    assert_eq!(h.state().store.list().unwrap().len(), 1, "host salvo");
    type_line(&mut h, "echo ok-$((1+1))");
    wait(&mut h, "saída ok-2", 15, |a| screen(a, 1).contains("ok-2"));

    // 4. desconectar e reconectar (mesmo terminal: scrollback mantido + separador, sem reenviar nada)
    click(&mut h, i18n::TAB_DISCONNECT());
    wait(&mut h, "desconectado", 15, |a| {
        status(a, 1) == i18n::ST_DISCONNECTED()
    });
    assert!(h.state().tabs[1].body.is_closed());
    ready();
    click(&mut h, i18n::TAB_RECONNECT());
    wait(&mut h, "novo pedido de senha", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    type_password(&mut h, "tester-pass");
    click(&mut h, i18n::CONNECT());
    wait(
        &mut h,
        "reconectado (host já confiável, sem modal)",
        15,
        |a| status(a, 1) == i18n::ST_CONNECTED(),
    );
    wait(&mut h, "prompt", 15, |a| !screen(a, 1).trim().is_empty());
    assert!(
        screen(h.state(), 1).contains(i18n::RECONNECTED_SEPARATOR()),
        "separador de reconexão: {}",
        screen(h.state(), 1)
    );
    assert_eq!(
        screen(h.state(), 1).matches("ok-2").count(),
        1,
        "a saída anterior continua visível (scrollback mantido)"
    );
    type_line(&mut h, "echo ok-$((2+2))");
    wait(&mut h, "saída ok-4", 15, |a| screen(a, 1).contains("ok-4"));
    assert_eq!(
        screen(h.state(), 1).matches("ok-2").count(),
        1,
        "comando antigo não foi reenviado"
    );

    assert_clean(h.state());
    h.state_mut().close_all_tabs();
    assert!(h.state().tabs.iter().all(|t| t.body.is_closed()));
    drop(h);
    assert_no_secrets_on_disk(dir.path());
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_host_key_alterada_bloqueia_e_esquecer_volta_ao_fluxo_desconhecido() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    let key = keys_dir().join("id_ed25519");
    let p = profile(
        "srv-chave",
        AuthMethod::KeyFile {
            path: key.clone(),
            passphrase: None,
        },
    );
    add_profile(&mut app, &p);
    // chave "confiável" falsa para o mesmo endpoint e algoritmo (outra chave pública válida)
    let pubkey = std::fs::read_to_string(keys_dir().join("id_ed25519.pub")).unwrap();
    let blob = pubkey.split_whitespace().nth(1).unwrap().to_string();
    let fake = HostKey {
        algorithm: "ssh-ed25519".into(),
        key_base64: blob,
        fingerprint_sha256: "SHA256:CHAVE-FALSA-CONHECIDA".into(),
    };
    let (h_, p_) = target();
    let ep = HostEndpoint::new(&h_, p_);
    app.store.trust(&ep, &fake).unwrap();
    app.open_profile(p.id);
    let mut h = harness(app);

    wait(&mut h, "bloqueio por chave alterada", 15, |a| {
        status(a, 1) == i18n::ST_BLOCKED()
    });
    h.get_by_label_contains("ALERTA");
    h.get_by_label_contains("SHA256:CHAVE-FALSA-CONHECIDA");
    h.get_by_label(i18n::HKC_OPEN_TRUSTED());
    // nenhuma forma de aceitar no fluxo da aba
    for l in [i18n::HK_ACCEPT_ONCE(), i18n::HK_ACCEPT_SAVE()] {
        assert!(h.query_by_label(l).is_none(), "{l} não pode existir aqui");
    }
    assert!(!h.state().tabs[1].body.needs_modal());
    assert!(h.state().tabs[1].body.is_closed());
    // o store não foi alterado
    assert_eq!(h.state().store.list().unwrap()[0].1, fake);

    // abre os hosts confiáveis e esquece (com confirmação)
    click(&mut h, i18n::HKC_OPEN_TRUSTED());
    assert!(matches!(h.state().dialog, Dialog::Settings));
    assert!(
        h.query_all_by_label_contains("SHA256:CHAVE-FALSA-CONHECIDA")
            .count()
            >= 1
    );
    // a rolagem até a seção é animada: deixa assentar antes de clicar
    h.run_steps(30);
    click(&mut h, i18n::TRUSTED_FORGET());
    assert!(matches!(h.state().dialog, Dialog::ConfirmForget(_)));
    assert_eq!(
        h.state().store.list().unwrap().len(),
        1,
        "ainda não removido"
    );
    click(&mut h, i18n::FORGET_CONFIRM());
    assert!(h.state().store.list().unwrap().is_empty());
    click(&mut h, i18n::CLOSE());
    assert!(matches!(h.state().dialog, Dialog::None));

    // nova conexão cai no fluxo de host desconhecido; rejeitar não salva nada
    ready();
    click(&mut h, i18n::TAB_RETRY());
    wait(&mut h, "host desconhecido", 15, |a| {
        status(a, 1) == i18n::ST_AWAITING_TRUST() && a.tabs[1].body.needs_modal()
    });
    click(&mut h, i18n::HK_REJECT());
    wait(&mut h, "rejeitado", 15, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    h.get_by_label_contains("host não confiável");
    assert!(h.state().store.list().unwrap().is_empty());
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_senha_errada_mostra_erro_e_tentar_novamente_pergunta_de_novo() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let p = profile("srv", AuthMethod::Password { credential: None });
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);

    wait(&mut h, "pedido de senha", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    type_password(&mut h, "senha-errada-xyz");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "erro de autenticação", 20, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    h.get_by_label_contains("senha incorreta");
    assert!(h.state().tabs[1].body.is_closed());
    ready();
    click(&mut h, i18n::TAB_RETRY());
    wait(&mut h, "pergunta de novo", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    h.get_by_label_contains("Senha de tester@");
    type_password(&mut h, "tester-pass");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "conectado após corrigir", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    assert_clean(h.state());
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_senha_salva_no_cofre_e_opcao_salvar() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    // (a) senha já salva: conecta direto, sem perguntar
    let cid = saved_password(&app, "tester-pass");
    let salva = profile(
        "salva",
        AuthMethod::Password {
            credential: Some(cid),
        },
    );
    add_profile(&mut app, &salva);
    // (b) perguntar + marcar "salvar no cofre"
    let ask = profile("pergunta", AuthMethod::Password { credential: None });
    add_profile(&mut app, &ask);
    app.open_profile(salva.id);
    let mut h = harness(app);
    wait(&mut h, "conectado com senha do cofre", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    assert!(!h.state().tabs[1].body.needs_modal());

    ready();
    h.state_mut().open_profile(ask.id);
    wait(&mut h, "pedido de senha", 10, |a| {
        a.tabs[2].body.needs_modal()
    });
    // a opção de salvar existe e NÃO vem marcada
    h.get_by_label(i18n::ASK_SAVE());
    type_password(&mut h, "tester-pass");
    click(&mut h, i18n::ASK_SAVE());
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "conectado", 20, |a| {
        status(a, 2) == i18n::ST_CONNECTED()
    });
    wait(&mut h, "perfil atualizado com credencial", 10, |a| {
        a.profile(ask.id).is_some_and(|p| {
            matches!(
                p.auth,
                AuthMethod::Password {
                    credential: Some(_)
                }
            )
        })
    });
    let app = h.state();
    let AuthMethod::Password {
        credential: Some(cid),
    } = app.profile(ask.id).unwrap().auth
    else {
        unreachable!()
    };
    let cred = app.store.get_credential(cid).unwrap().unwrap();
    assert_eq!(
        app.vault.get(&cred.secret).unwrap().expose_secret(),
        "tester-pass"
    );
    assert_clean(app);
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_chave_com_passphrase_perguntar_e_errada() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let p = profile(
        "chave-pass",
        AuthMethod::KeyFile {
            path: keys_dir().join("id_ed25519_pass"),
            passphrase: None,
        },
    );
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);
    wait(&mut h, "pedido de passphrase", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    h.get_by_label_contains("Passphrase da chave");
    type_password(&mut h, "passphrase-errada");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "erro de passphrase", 20, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    h.get_by_label_contains("passphrase incorreta");
    ready();
    click(&mut h, i18n::TAB_RETRY());
    wait(&mut h, "pergunta de novo", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    type_password(&mut h, "test-passphrase");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    assert_clean(h.state());
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_dez_abas_simultaneas_com_saida_continua() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let cid = saved_password(&app, "tester-pass");
    let mut ids: Vec<ProfileId> = Vec::new();
    for i in 0..10 {
        let auth = if i % 2 == 0 {
            AuthMethod::KeyFile {
                path: keys_dir().join("id_ed25519"),
                passphrase: None,
            }
        } else {
            AuthMethod::Password {
                credential: Some(cid),
            }
        };
        let p = profile(&format!("aba-{i}"), auth);
        add_profile(&mut app, &p);
        ids.push(p.id);
    }
    let mut h = harness(app);
    h.run_steps(3);
    let rss_base = rss_mb();

    // abre 10 abas (escalonadas para não acionar o MaxStartups do sshd)
    for id in &ids {
        h.state_mut().open_profile(*id);
        for _ in 0..20 {
            h.step();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    // abas: 0 = Início, 1..=10 = sessões
    wait(&mut h, "10 sessões conectadas", 60, |a| {
        (1..=10).all(|i| status(a, i) == i18n::ST_CONNECTED())
    });
    wait(&mut h, "prompts", 30, |a| {
        (1..=10).all(|i| !screen(a, i).trim().is_empty())
    });
    let rss_connected = rss_mb();
    eprintln!("[medida] RSS base={rss_base:.1} MB, com 10 sessões={rss_connected:.1} MB");

    // ocioso: sem repaint contínuo
    assert!(
        h.try_run().is_ok(),
        "ocioso não deve pedir repaint contínuo"
    );

    // (A) saída contínua na aba ATIVA (a última), demais ociosas
    let flood = "yes | head -c 5000000; echo DONE-$((7*6))";
    type_line(&mut h, flood);
    let mut times = Vec::new();
    let t0 = Instant::now();
    let mut last = h.state().tabs[10].body.debug_bytes();
    while !screen(h.state(), 10).contains("DONE-42") {
        assert!(
            t0.elapsed() < Duration::from_secs(180),
            "timeout no flood ativo"
        );
        let f = Instant::now();
        h.step();
        let dt = f.elapsed().as_secs_f64() * 1000.0;
        let now = h.state().tabs[10].body.debug_bytes();
        if now > last {
            times.push(dt);
        }
        last = now;
        std::thread::sleep(Duration::from_millis(1));
    }
    let (p50a, p95a) = (percentile(&mut times, 0.5), percentile(&mut times, 0.95));
    eprintln!(
        "[medida] flood 5 MB aba ATIVA: {} frames com saída, step() p50={p50a:.1} ms p95={p95a:.1} ms, total {:.1} s, RSS={:.1} MB",
        times.len(),
        t0.elapsed().as_secs_f64(),
        rss_mb()
    );

    // (B) saída contínua em aba de SEGUNDO PLANO (aba 5) enquanto a aba 1 está ativa
    let first = h.state().tabs[1].id;
    h.state_mut().active = Some(first);
    h.run_steps(2);
    h.state().tabs[5]
        .body
        .debug_send(format!("{flood}\r").as_bytes());
    let mut times = Vec::new();
    let t0 = Instant::now();
    let mut last = h.state().tabs[5].body.debug_bytes();
    while !screen(h.state(), 5).contains("DONE-42") {
        assert!(
            t0.elapsed() < Duration::from_secs(180),
            "timeout no flood em 2º plano"
        );
        let f = Instant::now();
        h.step();
        let dt = f.elapsed().as_secs_f64() * 1000.0;
        let now = h.state().tabs[5].body.debug_bytes();
        if now > last {
            times.push(dt);
        }
        last = now;
        std::thread::sleep(Duration::from_millis(1));
    }
    let (p50b, p95b) = (percentile(&mut times, 0.5), percentile(&mut times, 0.95));
    eprintln!(
        "[medida] flood 5 MB aba 2º PLANO: {} frames com saída, step() p50={p50b:.1} ms p95={p95b:.1} ms, total {:.1} s, RSS={:.1} MB",
        times.len(),
        t0.elapsed().as_secs_f64(),
        rss_mb()
    );
    // as outras abas continuam vivas
    assert!((1..=10).all(|i| status(h.state(), i) == i18n::ST_CONNECTED()));
    assert!(h.try_run().is_ok(), "ocioso depois do flood");

    // desconectar uma aba não afeta as outras
    let sixth = h.state().tabs[6].id;
    h.state_mut().active = Some(sixth);
    h.run_steps(2);
    click(&mut h, i18n::TAB_DISCONNECT());
    wait(&mut h, "aba 6 desconectada", 20, |a| {
        status(a, 6) == i18n::ST_DISCONNECTED()
    });
    assert!(
        (1..=10)
            .filter(|i| *i != 6)
            .all(|i| status(h.state(), i) == i18n::ST_CONNECTED())
    );

    // fechar o app: todas as sessões encerram de forma previsível
    let t0 = Instant::now();
    h.state_mut().close_all_tabs();
    eprintln!(
        "[medida] encerramento das 10 sessões: {:.2} s",
        t0.elapsed().as_secs_f64()
    );
    assert!(h.state().tabs.iter().all(|t| t.body.is_closed()));
    assert_clean(h.state());
}

#[test]
#[ignore = "requer MYSSH_TEST_LOG=arquivo com a saída de uma execução com --nocapture"]
fn real_saida_dos_testes_sem_segredos() {
    let path = std::env::var("MYSSH_TEST_LOG").expect("defina MYSSH_TEST_LOG");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(text.len() > 100, "log vazio: {path}");
    for s in SECRETS {
        assert!(!text.contains(s), "segredo encontrado no log de testes");
    }
}

/// Capturas das telas de terminal (render offscreen com wgpu), em `MYSSH_DEMO_SHOTS`.
#[test]
#[ignore = "gera PNGs; requer servidor de teste, MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn real_capturas() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    let mut p = profile("srv-producao", AuthMethod::Password { credential: None });
    p.environment = myssh_core::Environment::Production;
    add_profile(&mut app, &p);
    let key = profile(
        "srv-chave",
        AuthMethod::KeyFile {
            path: keys_dir().join("id_ed25519"),
            passphrase: None,
        },
    );
    add_profile(&mut app, &key);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_eframe(move |_| app);
    let shot = |h: &mut Harness<'static, App>, name: &str| {
        h.run_steps(4);
        h.render()
            .expect("render")
            .save(dir.join(format!("{name}.png")))
            .unwrap();
    };
    h.state_mut().open_profile(p.id);
    wait(&mut h, "senha", 10, |a| a.tabs[1].body.needs_modal());
    type_password(&mut h, "tester-pass");
    shot(&mut h, "20-senha");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "host key", 15, |a| a.tabs[1].body.needs_modal());
    shot(&mut h, "21-host-desconhecido");
    click(&mut h, i18n::HK_ACCEPT_SAVE());
    wait(&mut h, "conectado", 15, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    wait(&mut h, "prompt", 15, |a| !screen(a, 1).trim().is_empty());
    type_line(
        &mut h,
        "ls --color=always /etc | head -n 8; echo ok-$((1+1))",
    );
    wait(&mut h, "ok-2", 15, |a| screen(a, 1).contains("ok-2"));
    shot(&mut h, "22-conectado-producao");

    // colagem multilinha
    h.event(Event::Paste("echo um\necho dois\necho tres".into()));
    h.run_steps(3);
    wait(&mut h, "confirmação de colagem", 5, |a| {
        a.tabs[1].body.needs_modal()
    });
    shot(&mut h, "23-colar-multilinha");
    click(&mut h, i18n::CANCEL());

    // chave alterada (outra aba)
    ready();
    let (h_, p_) = target();
    let st = h.state().store.clone();
    st.forget(&HostEndpoint::new(&h_, p_)).unwrap();
    let pubkey = std::fs::read_to_string(keys_dir().join("id_ed25519.pub")).unwrap();
    st.trust(
        &HostEndpoint::new(&h_, p_),
        &HostKey {
            algorithm: "ssh-ed25519".into(),
            key_base64: pubkey.split_whitespace().nth(1).unwrap().to_string(),
            fingerprint_sha256: "SHA256:0000000000000000000000000000000000000000000".into(),
        },
    )
    .unwrap();
    h.state_mut().open_profile(key.id);
    wait(&mut h, "bloqueio", 15, |a| status(a, 2) == i18n::ST_BLOCKED());
    shot(&mut h, "24-chave-alterada");
    click(&mut h, i18n::HKC_OPEN_TRUSTED());
    shot(&mut h, "25-hosts-confiaveis");
    h.state_mut().close_dialog();
    h.state_mut().close_all_tabs();
}

// ------------------------------------------------------------------ MFA e jump host (servidor real)

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_keyboard_interactive_modal_de_prompt() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let mut p = profile("ki", AuthMethod::KeyboardInteractive);
    p.username = "kionly".into();
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);

    wait(
        &mut h,
        "pedido do servidor (keyboard-interactive)",
        15,
        |a| a.tabs[1].body.needs_modal(),
    );
    h.get_by_label(i18n::KI_TITLE());
    h.get_by_label_contains("Password");
    // O campo é mascarado (o servidor pediu sem eco).
    type_password(&mut h, "ki-pass");
    let value = h
        .get_by_role(egui::accesskit::Role::PasswordInput)
        .value()
        .unwrap_or_default();
    assert!(
        !value.contains("ki-pass"),
        "resposta exposta à acessibilidade"
    );
    click(&mut h, i18n::KI_SEND());
    wait(&mut h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    wait(&mut h, "prompt do shell", 15, |a| {
        !screen(a, 1).trim().is_empty()
    });
    assert_no_secrets("tela", &screen(h.state(), 1));
    assert!(!screen(h.state(), 1).contains("ki-pass"));
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_mfa_chave_mais_keyboard_interactive_cancelar() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let key = AuthMethod::KeyFile {
        path: keys_dir().join("id_ed25519"),
        passphrase: None,
    };
    let mut p = profile("mfa", key);
    p.username = "mfa".into();
    add_profile(&mut app, &p);
    app.open_profile(p.id);
    let mut h = harness(app);
    // A chave passa; o servidor pede o segundo fator. Cancelar encerra com erro claro.
    wait(&mut h, "segundo fator", 15, |a| {
        a.tabs[1].body.needs_modal()
    });
    click(&mut h, i18n::CANCEL());
    wait(&mut h, "autenticação cancelada", 15, |a| {
        status(a, 1) == i18n::ST_ERROR() && a.tabs[1].body.is_closed()
    });
    assert!(screen_or_error(h.state(), 1).contains("Autenticação cancelada"));
    // Tentar de novo e responder: conecta.
    ready();
    click(&mut h, i18n::TAB_RECONNECT());
    wait(&mut h, "segundo fator de novo", 15, |a| {
        a.tabs[1].body.needs_modal()
    });
    type_password(&mut h, "mfa-code-123456");
    click(&mut h, i18n::KI_SEND());
    wait(&mut h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste e container myssh-testsshd-target (./testenv/up.sh)"]
fn real_jump_host_conecta_e_pergunta_host_key_de_cada_salto() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    let cid = saved_password(&app, "tester-pass");
    let bastion = profile(
        "bastion",
        AuthMethod::Password {
            credential: Some(cid),
        },
    );
    add_profile(&mut app, &bastion);
    let dest = target_profile(
        "destino",
        bastion.id,
        AuthMethod::Password {
            credential: Some(cid),
        },
    );
    add_profile(&mut app, &dest);
    app.open_profile(dest.id);
    let mut h = harness(app);

    // 1. host key do bastion (salto 1 de 1), depois a do destino ("via bastion")
    wait(&mut h, "host key do bastion", 20, |a| {
        status(a, 1) == i18n::ST_AWAITING_TRUST() && a.tabs[1].body.needs_modal()
    });
    h.get_by_label_contains("Salto 1 de 1: bastion");
    click(&mut h, i18n::HK_ACCEPT_SAVE());
    wait(&mut h, "host key do destino", 20, |a| {
        status(a, 1) == i18n::ST_AWAITING_TRUST()
            && a.tabs[1].body.needs_modal()
            && a.store.list().map(|l| l.len() == 1).unwrap_or(false)
    });
    h.get_by_label_contains("via bastion bastion");
    h.get_by_label_contains("SHA256:");
    click(&mut h, i18n::HK_ACCEPT_SAVE());

    // 2. conectado ao DESTINO (o hostname do container-destino)
    wait(&mut h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    wait(&mut h, "prompt", 15, |a| !screen(a, 1).trim().is_empty());
    assert_eq!(
        h.state().store.list().unwrap().len(),
        2,
        "um host por salto"
    );
    let want = Command::new("docker")
        .args(["exec", TARGET_CONTAINER, "hostname"])
        .output()
        .unwrap();
    let want = String::from_utf8_lossy(&want.stdout).trim().to_string();
    type_line(&mut h, "echo dest-$(hostname)");
    wait(&mut h, "hostname do destino", 15, |a| {
        screen(a, 1).contains(&format!("dest-{want}"))
    });
    assert_clean(h.state());
    h.state_mut().close_all_tabs();
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste e container myssh-testsshd-target (./testenv/up.sh)"]
fn real_jump_host_pergunta_credenciais_de_cada_perfil_da_cadeia() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    trust_target(&app);
    // Nenhuma das duas senhas está salva: o app pergunta primeiro a do bastion, depois a do destino.
    let bastion = profile("bastion", AuthMethod::Password { credential: None });
    add_profile(&mut app, &bastion);
    let dest = target_profile(
        "destino",
        bastion.id,
        AuthMethod::Password { credential: None },
    );
    add_profile(&mut app, &dest);
    app.open_profile(dest.id);
    let mut h = harness(app);

    wait(&mut h, "senha do bastion", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    h.get_by_label_contains("(jump host bastion)");
    type_password(&mut h, "tester-pass");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "senha do destino", 10, |a| {
        a.tabs[1].body.needs_modal()
    });
    assert!(
        h.query_by_label_contains("(jump host bastion)").is_none(),
        "agora é a senha do destino"
    );
    h.get_by_label_contains("Senha de tester@myssh-testsshd-target");
    type_password(&mut h, "tester-pass");
    click(&mut h, i18n::CONNECT());
    wait(&mut h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED()
    });
    assert_clean(h.state());
    drop(h);
    cooldown();
}

#[test]
#[ignore = "requer servidor de teste e container myssh-testsshd-target (./testenv/up.sh)"]
fn real_jump_host_key_do_bastion_alterada_bloqueia_e_aponta_o_salto() {
    ready();
    let mut app = make_app(Store::open_in_memory().unwrap());
    let (h_, p_) = target();
    // Chave "conhecida" do bastion = a do destino (outra chave ed25519 qualquer).
    app.store
        .trust(&HostEndpoint::new(&h_, p_), &target_key())
        .unwrap();
    trust_target(&app);
    let cid = saved_password(&app, "tester-pass");
    let bastion = profile(
        "bastion",
        AuthMethod::Password {
            credential: Some(cid),
        },
    );
    add_profile(&mut app, &bastion);
    let dest = target_profile(
        "destino",
        bastion.id,
        AuthMethod::Password {
            credential: Some(cid),
        },
    );
    add_profile(&mut app, &dest);
    app.open_profile(dest.id);
    let mut h = harness(app);
    wait(&mut h, "bloqueado", 20, |a| {
        status(a, 1) == i18n::ST_BLOCKED()
    });
    h.get_by_label_contains("Salto 1 de 1: bastion");
    assert!(
        h.query_by_label(i18n::HK_ACCEPT_SAVE()).is_none(),
        "mudança de chave nunca oferece aceitar"
    );
    drop(h);
    cooldown();
}
