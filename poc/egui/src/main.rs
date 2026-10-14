use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;
use poc_egui::{app::App, demo, i18n};

fn main() -> eframe::Result {
    // Runtime de rede separado da thread de UI.
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ssh-net")
            .enable_all()
            .build()
            .expect("runtime tokio"),
    );
    let demo = demo::Demo::from_env();
    let is_demo = demo.is_some();

    // Em demo, fica por cima: o macOS não redesenha janelas totalmente cobertas (oclusão).
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(i18n::TITLE)
        .with_inner_size([900.0, 600.0]);
    if is_demo {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "poc-egui",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(rt, demo)))),
    );
    if is_demo && demo::DEMO_FAILED.load(Ordering::SeqCst) {
        eprintln!("[demo] terminou com falhas");
        std::process::exit(1);
    }
    result
}
