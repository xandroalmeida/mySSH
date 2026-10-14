//! Janela com um único terminal SSH, para validação manual (teclado, seleção, colar, rolagem,
//! vim/top/resize, tema claro/escuro). Configuração por variáveis de ambiente:
//!
//! ```text
//! MYSSH_HOST=127.0.0.1 MYSSH_PORT=2222 MYSSH_USER=tester \
//! MYSSH_PASSWORD=tester-pass          # ou MYSSH_KEY=/caminho/id_ed25519 [MYSSH_PASSPHRASE=...]
//!                                     # ou MYSSH_AGENT=1
//! MYSSH_FINGERPRINT=SHA256:...        # opcional: fixa a host key esperada (sem prompt)
//! MYSSH_CONFIRM_PASTE=1               # opcional: confirmar colagem multilinha
//! cargo run -p myssh-term --example single
//! ```
//!
//! Sem `MYSSH_FINGERPRINT`, um diálogo pergunta sobre a host key (aceitar uma vez / salvar em
//! memória / rejeitar). Variáveis de ambiente só servem a este exemplo de desenvolvimento.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustStoreError, TrustVerdict};
use myssh_ssh::{AuthMaterial, ConnectRequest, HostKeyDecision, PtySize, spawn};
use myssh_term::{TermSession, TermStyle, TermTheme, show};
use secrecy::SecretString;

/// Confia somente na chave com o fingerprint dado; qualquer outra é "alterada" (bloqueia).
struct PinnedTrust(String);

impl HostTrustStore for PinnedTrust {
    fn verify(&self, _: &HostEndpoint, key: &HostKey) -> Result<TrustVerdict, TrustStoreError> {
        Ok(if key.fingerprint_sha256 == self.0 {
            TrustVerdict::Trusted
        } else {
            TrustVerdict::Changed {
                previous: HostKey {
                    algorithm: "fixado por MYSSH_FINGERPRINT".into(),
                    key_base64: String::new(),
                    fingerprint_sha256: self.0.clone(),
                },
            }
        })
    }
    fn trust(&self, _: &HostEndpoint, _: &HostKey) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn replace(&self, _: &HostEndpoint, _: &HostKey) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn forget(&self, _: &HostEndpoint) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn list(&self) -> Result<Vec<(HostEndpoint, HostKey)>, TrustStoreError> {
        Ok(vec![])
    }
}

struct App {
    session: TermSession,
    style: TermStyle,
    dark: bool,
    pending_paste: Option<String>,
    _rt: Arc<tokio::runtime::Runtime>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl App {
    fn new(ctx: egui::Context, rt: Arc<tokio::runtime::Runtime>) -> Self {
        let host = env("MYSSH_HOST").unwrap_or_else(|| "127.0.0.1".into());
        let port: u16 = env("MYSSH_PORT").and_then(|p| p.parse().ok()).unwrap_or(22);
        let user = env("MYSSH_USER").unwrap_or_else(|| "tester".into());
        let auth = if let Some(path) = env("MYSSH_KEY") {
            AuthMaterial::KeyFile {
                path: path.into(),
                passphrase: env("MYSSH_PASSPHRASE").map(SecretString::from),
            }
        } else if env("MYSSH_AGENT").is_some() {
            AuthMaterial::Agent
        } else {
            AuthMaterial::Password(SecretString::from(
                env("MYSSH_PASSWORD").unwrap_or_default(),
            ))
        };
        let trust: Arc<dyn HostTrustStore> = match env("MYSSH_FINGERPRINT") {
            Some(fp) => Arc::new(PinnedTrust(fp)),
            None => Arc::new(myssh_store::Store::open_in_memory().expect("store em memória")),
        };
        let req = ConnectRequest {
            endpoint: HostEndpoint::new(&host, port),
            username: user,
            auth,
            pty: PtySize {
                cols: 100,
                rows: 30,
                pix_w: 800,
                pix_h: 480,
            },
            term: "xterm-256color".into(),
            connect_timeout: Duration::from_secs(15),
            keepalive: Some(Duration::from_secs(30)),
            jumps: vec![],
        };
        let handle = spawn(
            rt.handle(),
            req,
            trust,
            Arc::new(move || ctx.request_repaint()),
        );
        let mut session = TermSession::new(handle, 100, 30);
        session.set_confirm_multiline(env("MYSSH_CONFIRM_PASTE").is_some());
        Self {
            session,
            style: TermStyle::default(),
            dark: true,
            pending_paste: None,
            _rt: rt,
        }
    }

    fn host_key_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.session.host_key_prompt().cloned() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("hostkey")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Host desconhecido");
            ui.label(format!("{}:{}", p.endpoint.host, p.endpoint.port));
            ui.label("Confira o fingerprint por um canal confiável antes de aceitar.");
            ui.label(format!("Algoritmo: {}", p.key.algorithm));
            ui.monospace(&p.key.fingerprint_sha256);
            ui.horizontal(|ui| {
                if ui.button("Aceitar e salvar").clicked() {
                    answer = Some(HostKeyDecision::AcceptAndSave);
                }
                if ui.button("Aceitar uma vez").clicked() {
                    answer = Some(HostKeyDecision::AcceptOnce);
                }
                if ui.button("Rejeitar").clicked() {
                    answer = Some(HostKeyDecision::Reject);
                }
            });
        });
        if let Some(a) = answer {
            self.session.answer_host_key(a);
        }
    }

    fn paste_dialog(&mut self, ctx: &egui::Context) {
        let Some(text) = self.pending_paste.clone() else {
            return;
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::new("paste")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Colar várias linhas?");
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    ui.monospace(&text);
                });
            ui.horizontal(|ui| {
                if ui.button("Colar").clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancelar").clicked() {
                    decision = Some(false);
                }
            });
        });
        if let Some(ok) = decision {
            if ok {
                self.session.paste_confirmed(&text);
            }
            self.pending_paste = None;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        egui::Panel::top("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let state = match self.session.closed() {
                    Some(Some(e)) => format!("Encerrada: {e}"),
                    Some(None) => "Encerrada".to_string(),
                    None => format!("{:?}", self.session.state()),
                };
                ui.label(state);
                if let Some(t) = self.session.title() {
                    ui.separator();
                    ui.label(t);
                }
                if ui
                    .button(if self.dark {
                        "Tema claro"
                    } else {
                        "Tema escuro"
                    })
                    .clicked()
                {
                    self.dark = !self.dark;
                    self.style.theme = if self.dark {
                        TermTheme::dark()
                    } else {
                        TermTheme::light()
                    };
                }
                if ui.button("Desconectar").clicked() {
                    self.session.disconnect();
                }
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.session.set_input_enabled(self.pending_paste.is_none());
                let r = show(ui, &mut self.session, &self.style);
                if r.paste_needs_confirm.is_some() {
                    self.pending_paste = r.paste_needs_confirm;
                }
            });
        // Com um diálogo aberto o terminal não deve receber teclas.
        self.host_key_dialog(&ctx);
        self.paste_dialog(&ctx);
    }
}

fn main() -> eframe::Result {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ssh-net")
            .enable_all()
            .build()
            .expect("runtime tokio"),
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mySSH — terminal (exemplo)")
            .with_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "myssh-term-single",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc.egui_ctx.clone(), rt)))),
    )
}
