//! Aplicação eframe: formulário, diálogo de host key e terminal.

use std::sync::Arc;

use eframe::egui;
use secrecy::SecretString;
use zeroize::Zeroize;

use crate::demo::Demo;
use crate::i18n;
use crate::known_hosts::KnownHosts;
use crate::session::{self, Auth, ConnectParams};
use crate::terminal::{Status, TermSession};
use crate::widget;

/// Máximo de bytes de saída remota processados por frame (resto fica no canal limitado,
/// aplicando backpressure ao servidor).
const PUMP_BUDGET: usize = 256 * 1024;

#[derive(Clone, Default)]
pub struct Form {
    pub host: String,
    pub port: String,
    pub user: String,
    pub use_key: bool,
    pub password: String,
    pub key_path: String,
    pub passphrase: String,
}

pub struct App {
    rt: Arc<tokio::runtime::Runtime>,
    form: Form,
    session: Option<TermSession>,
    error: Option<String>,
    demo: Option<Demo>,
    auto_connect: bool,
    font: egui::FontId,
}

impl App {
    pub fn new(rt: Arc<tokio::runtime::Runtime>, demo: Option<Demo>) -> Self {
        let mut form = Form {
            host: "127.0.0.1".into(),
            port: "22".into(),
            ..Form::default()
        };
        let mut auto_connect = false;
        if let Some(d) = &demo {
            form.host = d.cfg.host.clone();
            form.port = d.cfg.port.to_string();
            form.user = d.cfg.user.clone();
            form.use_key = true;
            form.key_path = d.cfg.key_path.clone();
            auto_connect = true;
        }
        Self {
            rt,
            form,
            session: None,
            error: None,
            demo,
            auto_connect,
            font: widget::font(),
        }
    }

    fn connect(&mut self, ctx: &egui::Context) {
        self.error = None;
        let Ok(port) = self.form.port.trim().parse::<u16>() else {
            self.error = Some(i18n::INVALID_PORT.into());
            return;
        };
        let auth = if self.form.use_key {
            let pass = (!self.form.passphrase.is_empty())
                .then(|| SecretString::from(self.form.passphrase.clone()));
            Auth::Key {
                path: self.form.key_path.trim().into(),
                passphrase: pass,
            }
        } else {
            Auth::Password(SecretString::from(self.form.password.clone()))
        };
        // Os segredos agora vivem em SecretString; zera os campos da UI.
        self.form.password.zeroize();
        self.form.passphrase.zeroize();

        let (cw, ch) = widget::cell_metrics(ctx, &self.font);
        let size = ctx.content_rect().size();
        let cols = ((size.x / cw).floor() as u16).clamp(20, 400);
        let rows = (((size.y - 60.0) / ch).floor() as u16).clamp(5, 200);
        let ppp = ctx.pixels_per_point();
        let params = ConnectParams {
            host: self.form.host.trim().to_string(),
            port,
            user: self.form.user.trim().to_string(),
            auth,
            cols,
            rows,
            pix_w: (cw * ppp * cols as f32) as u16,
            pix_h: (ch * ppp * rows as f32) as u16,
            known_hosts: Some(KnownHosts::default_path()),
            pinned_fingerprint: None,
        };
        let c = ctx.clone();
        let handle = session::spawn(
            self.rt.handle(),
            params,
            Arc::new(move || c.request_repaint()),
        );
        self.session = Some(TermSession::new(handle, cols, rows));
    }

    fn form_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut go = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::HOST);
            ui.add(egui::TextEdit::singleline(&mut self.form.host).desired_width(130.0));
            ui.label(i18n::PORT);
            ui.add(egui::TextEdit::singleline(&mut self.form.port).desired_width(45.0));
            ui.label(i18n::USER);
            ui.add(egui::TextEdit::singleline(&mut self.form.user).desired_width(100.0));
            ui.radio_value(&mut self.form.use_key, false, i18n::AUTH_PASSWORD);
            ui.radio_value(&mut self.form.use_key, true, i18n::AUTH_KEY);
        });
        ui.horizontal_wrapped(|ui| {
            if self.form.use_key {
                ui.label(i18n::KEY_PATH);
                ui.add(egui::TextEdit::singleline(&mut self.form.key_path).desired_width(280.0));
                ui.label(i18n::PASSPHRASE);
                ui.add(
                    egui::TextEdit::singleline(&mut self.form.passphrase)
                        .password(true)
                        .desired_width(140.0),
                );
            } else {
                ui.label(i18n::PASSWORD);
                ui.add(
                    egui::TextEdit::singleline(&mut self.form.password)
                        .password(true)
                        .desired_width(180.0),
                );
            }
            if ui.button(i18n::CONNECT).clicked() {
                go = true;
            }
        });
        go
    }

    fn host_key_dialog(&mut self, ctx: &egui::Context) {
        let Some(sess) = self.session.as_mut() else {
            return;
        };
        let Some(prompt) = sess.prompt.as_ref() else {
            return;
        };
        let (host, port, algo, fp) = (
            prompt.host.clone(),
            prompt.port,
            prompt.algorithm.clone(),
            prompt.fingerprint.clone(),
        );
        let mut answer = None;
        egui::Modal::new(egui::Id::new("hostkey-dialog")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading(i18n::HOSTKEY_TITLE);
            ui.label(format!("{host}:{port}"));
            ui.add_space(6.0);
            ui.label(i18n::HOSTKEY_BODY);
            ui.add_space(6.0);
            ui.label(format!("{}: {algo}", i18n::HOSTKEY_ALGORITHM));
            ui.label(format!("{}:", i18n::HOSTKEY_FINGERPRINT));
            ui.monospace(&fp);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(i18n::HOSTKEY_ACCEPT).clicked() {
                    answer = Some(true);
                }
                if ui.button(i18n::HOSTKEY_REJECT).clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(a) = answer {
            sess.answer_host_key(a);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_visuals(egui::Visuals::dark());

        // 1. Rede -> Term, em lotes.
        if let Some(sess) = self.session.as_mut() {
            if sess.pump(PUMP_BUDGET) {
                ctx.request_repaint();
            }
        }

        // 2. Demonstração automatizada (opcional).
        if let Some(demo) = self.demo.as_mut() {
            demo.tick(&ctx, &mut self.session);
        }

        let status = self.session.as_ref().map(|s| s.status.clone());
        let connected = matches!(status, Some(Status::Connected));
        let in_form = !connected && !matches!(status, Some(Status::Connecting));

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            match &status {
                Some(Status::Connected)
                | Some(Status::AwaitingHostKey)
                | Some(Status::Connecting) => {
                    ui.horizontal(|ui| {
                        let label = match status {
                            Some(Status::Connected) => i18n::CONNECTED,
                            _ => i18n::CONNECTING,
                        };
                        ui.label(format!("{label}: {}@{}", self.form.user, self.form.host));
                        if ui.button(i18n::DISCONNECT).clicked() {
                            if let Some(s) = &self.session {
                                s.close();
                            }
                            self.session = None;
                        }
                    });
                }
                _ => {
                    if self.form_ui(ui) {
                        self.auto_connect = true;
                    }
                    if let Some(Status::Closed(Some(e))) = &status {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("{}{e}", i18n::ERROR_PREFIX),
                        );
                    } else if matches!(&status, Some(Status::Closed(None))) {
                        ui.label(i18n::DISCONNECTED);
                    }
                    if let Some(e) = &self.error {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                    }
                }
            }
            ui.add_space(4.0);
        });
        let _ = in_form;

        if self.auto_connect {
            self.auto_connect = false;
            self.connect(&ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if let Some(sess) = self
                    .session
                    .as_mut()
                    .filter(|s| matches!(s.status, Status::Connected | Status::Closed(_)))
                {
                    let interactive = matches!(sess.status, Status::Connected);
                    widget::show(ui, sess, &self.font, interactive);
                }
            });

        self.host_key_dialog(&ctx);

        // Captura de tela solicitada pela demo (resposta chega como Event::Screenshot).
        if let Some(demo) = self.demo.as_mut() {
            let shots: Vec<_> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                    .collect()
            });
            for img in shots {
                demo.on_screenshot(&img);
            }
        }
    }
}
