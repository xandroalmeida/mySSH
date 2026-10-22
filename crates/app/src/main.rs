//! mySSH: casca da aplicação desktop (perfis, abas, configurações).

mod about;
mod app;
mod config_io;
#[cfg(any(test, debug_assertions, feature = "demo"))]
mod demo;
#[cfg(not(any(test, debug_assertions, feature = "demo")))]
#[path = "demo_off.rs"]
mod demo;
mod dialogs;
mod dialogs_extra;
mod env;
mod files;
mod form;
mod i18n;
mod icon;
mod logic;
mod paths;
mod settings;
mod shortcuts;
mod split;
mod split_body;
mod ssh_import;
mod suspend;
mod tabs;
mod terminal_tab;
mod theming;
mod view;

#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod real_tests_m3c;
#[cfg(test)]
mod real_tests_m4b;
#[cfg(test)]
mod ui_tests;
#[cfg(test)]
mod ui_tests_aws;
#[cfg(test)]
mod ui_tests_m2b;
#[cfg(test)]
mod ui_tests_m2b_shots;

use std::sync::Arc;

use myssh_secrets::{MemoryStore, OsKeyring, SecretStore, Vault};
use myssh_store::Store;

use crate::app::{App, VaultMode};

fn main() -> eframe::Result {
    // Idioma do sistema até as configurações salvas serem lidas (em `App`).
    myssh_i18n::set_lang(settings::system_lang());
    // Runtime criado uma vez; usado por tarefas de cofre e, depois, pelas sessões SSH.
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("myssh-rt")
            .enable_all()
            .build()
            .expect("runtime tokio"),
    );

    let (dir, overridden) = match paths::data_dir() {
        Some(x) => x,
        None => {
            eprintln!("Não foi possível determinar o diretório de dados.");
            std::process::exit(1);
        }
    };
    let demo = demo::Demo::from_env(overridden);

    let mut store_warning = None;
    let store = match paths::prepare_db(&dir)
        .map_err(|_| ())
        .and_then(|db| Store::open(db).map_err(|_| ()))
    {
        Ok(s) => s,
        Err(()) => {
            store_warning = Some(i18n::STORE_FALLBACK().to_string());
            Store::open_in_memory().expect("sqlite em memória")
        }
    };

    // Diretório de dados sobrescrito (demo/testes) ou MYSSH_MEMORY_VAULT: nunca usa o Keychain real.
    let memory_vault = overridden || std::env::var_os("MYSSH_MEMORY_VAULT").is_some();
    let (backend, mode): (Arc<dyn SecretStore>, _) = if memory_vault {
        (Arc::new(MemoryStore::new()), VaultMode::SessionOnly)
    } else {
        (Arc::new(OsKeyring::new()), VaultMode::Os)
    };
    let vault = Arc::new(Vault::new(backend));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(i18n::TITLE())
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([720.0, 480.0])
        .with_icon(Arc::new(icon::app_icon()));
    if demo.is_some() {
        // O macOS não redesenha janelas totalmente cobertas.
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let is_demo = demo.is_some();
    let data_dir = dir.clone();
    let result = eframe::run_native(
        "myssh",
        options,
        Box::new(move |cc| {
            myssh_term::fonts::install(&cc.egui_ctx);
            let mut app = App::new(rt, store, vault, mode, store_warning, demo);
            app.data_dir = Some(data_dir);
            Ok(Box::new(app))
        }),
    );
    if is_demo && demo::FAILED.load(std::sync::atomic::Ordering::SeqCst) {
        eprintln!("[demo] terminou com falhas");
        std::process::exit(1);
    }
    result
}
