//! Testes de UI headless (egui_kittest): fluxos do app sem janela nem Keychain real.
//!
//! `headless_screenshots` (ignorado) renderiza as mesmas telas do modo demo offscreen:
//! `MYSSH_DEMO_SHOTS=dir cargo test -p myssh-app headless_screenshots -- --ignored`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use myssh_core::AuthMethod;
use myssh_secrets::{
    Availability, MemoryStore, SecretError, SecretRef, SecretStore, SecretString, Vault,
};
use myssh_store::Store;

use crate::app::{App, Dialog, ProfileDialog, VaultIssue, VaultMode};
use crate::demo;
use crate::form::{ProfileForm, SecretMode};

fn runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    )
}

fn app_with(store: Arc<dyn SecretStore>, mode: VaultMode) -> App {
    App::new(
        runtime(),
        Store::open_in_memory().unwrap(),
        Arc::new(Vault::new(store)),
        mode,
        None,
        None,
    )
}

fn memory_app() -> App {
    app_with(Arc::new(MemoryStore::new()), VaultMode::SessionOnly)
}

fn harness(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(move |_| app)
}

/// Avança frames até `cond` (tarefas em segundo plano reagem via canal).
fn wait_until(h: &mut Harness<'static, App>, what: &str, cond: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !cond(h.state()) {
        assert!(start.elapsed() < Duration::from_secs(10), "timeout: {what}");
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn form(name: &str) -> ProfileForm {
    let mut f = ProfileForm::blank();
    f.name = name.into();
    f.host = "srv.local".into();
    f.username = "ana".into();
    f
}

fn submit(h: &mut Harness<'static, App>, f: ProfileForm) {
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    app.dialog = Dialog::Profile(Box::new(ProfileDialog::new(f)));
    app.submit_profile_dialog(&ctx);
}

#[test]
fn atalho_novo_perfil_abre_dialogo() {
    let mut h = harness(memory_app());
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(2);
    assert!(matches!(h.state().dialog, Dialog::Profile(_)));
}

#[test]
fn atalhos_nao_disparam_com_dialogo_aberto() {
    let mut h = harness(memory_app());
    h.state_mut().dialog = Dialog::Settings;
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run_steps(2);
    assert!(matches!(h.state().dialog, Dialog::Settings));
}

#[test]
fn atalho_nova_aba_e_fechar_aba() {
    let mut h = harness(memory_app());
    submit(&mut h, form("srv"));
    wait_until(&mut h, "perfil salvo", |a| a.profiles.len() == 1);
    assert_eq!(h.state().tabs.len(), 1); // boas-vindas
    h.key_press_modifiers(Modifiers::COMMAND, Key::T);
    h.run_steps(2);
    assert_eq!(h.state().tabs.len(), 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(2);
    assert_eq!(h.state().tabs.len(), 1);
}

#[test]
fn fechar_aba_ocupada_pede_confirmacao() {
    let mut h = harness(memory_app());
    submit(&mut h, form("srv"));
    wait_until(&mut h, "perfil salvo", |a| a.profiles.len() == 1);
    let app = h.state_mut();
    app.open_profile(app.profiles[0].id);
    let id = app.active.unwrap();
    app.tabs.last_mut().unwrap().body.simulate_busy(true);
    app.request_close_tab(id);
    assert!(matches!(app.dialog, Dialog::ConfirmCloseTab(x) if x == id));
    assert_eq!(app.tabs.len(), 2);
    h.run_steps(2);
    // confirmar
    let app = h.state_mut();
    app.close_dialog();
    app.close_tab(id);
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn senha_salva_vai_para_o_cofre_e_exclusao_limpa() {
    let mut h = harness(memory_app());
    let mut f = form("srv");
    f.password_mode = SecretMode::Save;
    f.password = "s3gredo".into();
    submit(&mut h, f);
    wait_until(&mut h, "perfil salvo", |a| {
        a.profiles.len() == 1 && matches!(a.dialog, Dialog::None)
    });
    let app = h.state();
    let AuthMethod::Password {
        credential: Some(cid),
    } = app.profiles[0].auth
    else {
        panic!("credencial esperada");
    };
    let cred = app.store.get_credential(cid).unwrap().unwrap();
    assert_eq!(
        myssh_secrets::ExposeSecret::expose_secret(&app.vault.get(&cred.secret).unwrap()),
        "s3gredo"
    );
    // o banco não contém o segredo
    assert!(!format!("{:?}", app.store.list_credentials().unwrap()).contains("s3gredo"));

    let pid = app.profiles[0].id;
    let secret_ref = cred.secret.clone();
    h.state_mut().delete_profile(pid);
    assert!(h.state().store.list_credentials().unwrap().is_empty());
    wait_until(&mut h, "segredo removido do cofre", |a| {
        matches!(a.vault.get(&secret_ref), Err(SecretError::NotFound))
    });
}

#[test]
fn excluir_perfil_mantem_credencial_usada_por_duplicata() {
    let mut h = harness(memory_app());
    let mut f = form("srv");
    f.password_mode = SecretMode::Save;
    f.password = "x".into();
    submit(&mut h, f);
    wait_until(&mut h, "salvo", |a| a.profiles.len() == 1);
    let id = h.state().profiles[0].id;
    h.state_mut().duplicate(id);
    assert_eq!(h.state().profiles.len(), 2);
    h.state_mut().delete_profile(id);
    assert_eq!(h.state().store.list_credentials().unwrap().len(), 1);
    let rest = h.state().profiles[0].id;
    h.state_mut().delete_profile(rest);
    assert!(h.state().store.list_credentials().unwrap().is_empty());
}

/// Cofre do SO indisponível.
struct Unavailable;
impl SecretStore for Unavailable {
    fn put(&self, _: &SecretRef, _: SecretString) -> myssh_secrets::Result<()> {
        Err(SecretError::Unavailable("teste".into()))
    }
    fn get(&self, _: &SecretRef) -> myssh_secrets::Result<SecretString> {
        Err(SecretError::Unavailable("teste".into()))
    }
    fn delete(&self, _: &SecretRef) -> myssh_secrets::Result<()> {
        Err(SecretError::Unavailable("teste".into()))
    }
    fn availability(&self) -> Availability {
        Availability::Unavailable("teste".into())
    }
}

#[test]
fn cofre_indisponivel_nao_grava_e_oferece_so_sessao() {
    let mut h = harness(app_with(Arc::new(Unavailable), VaultMode::Os));
    let mut f = form("srv");
    f.password_mode = SecretMode::Save;
    f.password = "s3gredo".into();
    submit(&mut h, f);
    wait_until(
        &mut h,
        "erro do cofre",
        |a| matches!(&a.dialog, Dialog::Profile(d) if d.issue.is_some()),
    );
    let app = h.state();
    let Dialog::Profile(d) = &app.dialog else {
        unreachable!()
    };
    assert_eq!(d.issue, Some(VaultIssue::Unavailable));
    assert!(!d.saving);
    // nada foi gravado: nem perfil, nem credencial
    assert!(app.profiles.is_empty());
    assert!(app.store.list_credentials().unwrap().is_empty());

    // ação explícita: usar só nesta sessão e salvar de novo
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    app.use_session_vault(&ctx);
    app.submit_profile_dialog(&ctx);
    wait_until(&mut h, "salvo em memória", |a| {
        a.profiles.len() == 1 && matches!(a.dialog, Dialog::None)
    });
    assert_eq!(h.state().vault_mode, VaultMode::SessionOnly);
    assert_eq!(h.state().store.list_credentials().unwrap().len(), 1);
}

#[test]
fn bloquear_cofre_nao_fecha_abas() {
    let mut h = harness(memory_app());
    h.state_mut().vault.lock();
    h.run_steps(2);
    assert!(h.state().vault.is_locked());
    assert_eq!(h.state().tabs.len(), 1);
}

#[test]
fn configuracoes_persistem_ao_mudar() {
    let mut h = harness(memory_app());
    h.state_mut().settings.prod_warning = false;
    h.state_mut().settings.vault_lock_minutes = 3;
    h.run_steps(3);
    let loaded = crate::settings::Settings::load(&h.state().store);
    assert!(!loaded.prod_warning);
    assert_eq!(loaded.vault_lock_minutes, 3);
}

#[test]
#[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn headless_screenshots() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS") else {
        eprintln!("MYSSH_DEMO_SHOTS não definido: nada a fazer");
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_eframe(|_| memory_app());
    h.run_steps(3);
    for seed in demo::seeds() {
        let f = seed(h.state());
        submit(&mut h, f);
        wait_until(&mut h, "seed", |a| {
            a.is_idle() && matches!(a.dialog, Dialog::None)
        });
    }
    for (name, act) in demo::STEPS {
        let ctx = h.ctx.clone();
        act(h.state_mut(), &ctx);
        h.run_steps(12);
        let name = name.unwrap_or("shot");
        let img = h.render().expect("render");
        let path = dir.join(format!("{name}.png"));
        img.save(&path).unwrap();
        eprintln!("screenshot: {}", path.display());
    }
}
