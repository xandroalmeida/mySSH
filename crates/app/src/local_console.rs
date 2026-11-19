//! Criação e aba de console local, independente de perfis/credenciais SSH.
use std::sync::{Arc, mpsc};

use egui::{Color32, FontId};
use myssh_term::{DEFAULT_PUMP_BUDGET, TermSession, TermStyle, TermTheme};
use zeroize::Zeroizing;

use crate::dialogs::Next;
use crate::i18n;
use crate::local_shell::{self, Shell};
use crate::tabs::{StatusBadge, TabBody, TabEnv};

pub struct ConsoleDialog {
    rx: mpsc::Receiver<Vec<Shell>>,
    shells: Option<Vec<Shell>>,
    selected: usize,
}

impl ConsoleDialog {
    pub fn load(rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        rt.spawn_blocking(move || {
            let _ = tx.send(local_shell::discover());
            ctx.request_repaint();
        });
        Self {
            rx,
            shells: None,
            selected: 0,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Next {
        if self.shells.is_none() {
            match self.rx.try_recv() {
                Ok(shells) => self.shells = Some(shells),
                Err(mpsc::TryRecvError::Disconnected) => self.shells = Some(Vec::new()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut next = Next::Keep;
        let resp = egui::Modal::new(egui::Id::new("local-console-dialog")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(i18n::LOCAL_CONSOLE());
            ui.label(i18n::LOCAL_HELP());
            ui.add_space(8.0);
            match &self.shells {
                None => {
                    ui.spinner();
                    ui.label(i18n::LOCAL_LOADING());
                }
                Some(shells) if shells.is_empty() => {
                    ui.label(i18n::LOCAL_EMPTY());
                }
                Some(shells) => {
                    ui.label(i18n::LOCAL_SHELL());
                    egui::ComboBox::from_id_salt("local-shell")
                        .selected_text(&shells[self.selected].name)
                        .show_ui(ui, |ui| {
                            for (i, shell) in shells.iter().enumerate() {
                                ui.selectable_value(&mut self.selected, i, &shell.name)
                                    .on_hover_text(shell.path.display().to_string());
                            }
                        });
                    ui.weak(shells[self.selected].path.display().to_string());
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let choice = self.shells.as_ref().and_then(|s| s.get(self.selected));
                if ui
                    .add_enabled(choice.is_some(), egui::Button::new(i18n::OPEN()))
                    .clicked()
                    && let Some(shell) = choice
                {
                    next = Next::Local(shell.clone());
                }
                if ui.button(i18n::CANCEL()).clicked() {
                    next = Next::Close;
                }
            });
        });
        if resp.should_close() {
            next = Next::Close;
        }
        next
    }
}

pub struct LocalConsole {
    shell: Shell,
    sess: Option<TermSession>,
    paste: Option<Zeroizing<String>>,
    paste_fresh: bool,
    focused: bool,
    closing: bool,
}

impl LocalConsole {
    pub fn new(shell: Shell) -> Self {
        Self {
            shell,
            sess: None,
            paste: None,
            paste_fresh: false,
            focused: false,
            closing: false,
        }
    }

    fn start(&mut self, env: &TabEnv) {
        let ctx = env.ctx.clone();
        let handle = local_shell::spawn(
            env.rt,
            self.shell.clone(),
            Arc::new(move || ctx.request_repaint()),
        );
        if let Some(sess) = &mut self.sess {
            sess.restart(handle, i18n::RECONNECTED_SEPARATOR());
        } else {
            self.sess = Some(TermSession::new(handle, 80, 24));
        }
        self.closing = false;
    }
}

impl TabBody for LocalConsole {
    fn title(&self) -> String {
        format!("{} · {}", i18n::LOCAL_CONSOLE(), self.shell.name)
    }

    fn pump(&mut self, env: &TabEnv) {
        if self.sess.is_none() && env.auto_connect && !self.closing {
            self.start(env);
        }
        if let Some(sess) = &mut self.sess {
            if sess.pump(DEFAULT_PUMP_BUDGET) {
                env.ctx.request_repaint();
            }
            if let Some(delay) = sess.sync_deadline() {
                env.ctx.request_repaint_after(delay);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, env: &TabEnv) {
        let live = self.is_busy();
        ui.horizontal(|ui| {
            ui.label(&self.shell.name)
                .on_hover_text(self.shell.path.display().to_string());
            if live {
                if ui.button(i18n::TAB_DISCONNECT()).clicked() {
                    self.close();
                }
            } else if ui.button(i18n::TAB_RECONNECT()).clicked() {
                self.start(env);
            }
        });
        ui.separator();
        let Some(sess) = &mut self.sess else { return };
        if let Some(error) = sess.closed() {
            match error {
                Some(msg) => {
                    ui.colored_label(Color32::RED, msg);
                }
                None => {
                    ui.weak(i18n::TAB_SESSION_ENDED());
                }
            }
        }
        sess.set_confirm_multiline(env.confirm_paste);
        sess.set_input_enabled(!env.input_blocked);
        let theme = if env.dark {
            TermTheme::dark()
        } else {
            TermTheme::light()
        };
        let response = myssh_term::show(
            ui,
            sess,
            &TermStyle::new(FontId::monospace(env.font_size), theme),
        );
        self.focused = response.response.has_focus();
        if let Some(text) = response.paste_needs_confirm {
            self.paste = Some(Zeroizing::new(text));
            self.paste_fresh = true;
        }
    }

    fn needs_modal(&self) -> bool {
        self.paste.is_some()
    }

    fn modal_ui(&mut self, ctx: &egui::Context, _env: &TabEnv) {
        let Some(text) = &self.paste else { return };
        let (preview, lines) = crate::terminal_tab::paste_preview(text);
        let preview = Zeroizing::new(preview);
        let fresh = std::mem::take(&mut self.paste_fresh);
        let mut accept = false;
        let mut cancel = false;
        let resp = egui::Modal::new(egui::Id::new((
            "local-paste",
            self.sess.as_ref().map(TermSession::id),
        )))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(i18n::PASTE_TITLE());
            ui.label(i18n::PASTE_BODY());
            ui.strong(i18n::paste_lines(lines));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(preview.as_str()).monospace());
            });
            ui.horizontal(|ui| {
                accept = ui.button(i18n::PASTE_CONFIRM()).clicked();
                let button = ui.button(i18n::CANCEL());
                if fresh {
                    button.request_focus();
                }
                cancel = button.clicked();
            });
        });
        if accept {
            if let Some(text) = self.paste.take()
                && let Some(sess) = &mut self.sess
            {
                sess.paste_confirmed(&text);
            }
        } else if cancel || resp.should_close() {
            self.paste = None;
        }
    }

    fn fills_space(&self) -> bool {
        true
    }
    fn has_terminal_focus(&self) -> bool {
        self.focused && self.sess.as_ref().is_some_and(TermSession::is_connected)
    }
    fn is_busy(&self) -> bool {
        self.sess.as_ref().is_some_and(|s| s.closed().is_none())
    }
    fn close(&mut self) {
        self.closing = true;
        self.paste = None;
        if let Some(sess) = &self.sess {
            sess.disconnect();
        }
    }
    fn is_closed(&self) -> bool {
        !self.is_busy()
    }
    fn drain(&mut self) {
        if let Some(sess) = &mut self.sess {
            sess.pump(DEFAULT_PUMP_BUDGET);
        }
    }
    fn status(&self) -> Option<StatusBadge> {
        let connected = self.sess.as_ref().is_some_and(TermSession::is_connected);
        let starting = self.is_busy() && !connected;
        Some(StatusBadge {
            icon: if connected {
                "●"
            } else if starting {
                "◌"
            } else {
                "○"
            },
            text: if connected {
                i18n::ST_CONNECTED()
            } else if starting {
                i18n::ST_CONNECTING()
            } else {
                i18n::ST_DISCONNECTED()
            },
            color: if connected {
                Color32::from_rgb(80, 180, 120)
            } else {
                Color32::GRAY
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Dialog;
    use crate::tabs::TabKind;
    use crate::ui_tests::{harness, memory_app, wait_until};
    use egui_kittest::kittest::Queryable;

    #[test]
    fn console_local_cria_aba_sem_perfil_ssh_e_pode_ser_cancelado() {
        let mut h = harness(memory_app());
        h.run_steps(2);
        h.get_by_label(i18n::LOCAL_CONSOLE()).click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Dialog::Local(_)));
        // Aguarda descoberta fora da UI e renderiza o seletor.
        wait_until(
            &mut h,
            "descoberta de shells",
            |a| matches!(&a.dialog, Dialog::Local(d) if d.shells.is_some()),
        );
        h.run_steps(2);
        h.get_by_label(i18n::CANCEL()).click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Dialog::None));
        assert_eq!(h.state().tabs.len(), 1);
        h.get_by_label(i18n::LOCAL_CONSOLE()).click();
        h.run_steps(2);
        wait_until(
            &mut h,
            "descoberta de shells",
            |a| matches!(&a.dialog, Dialog::Local(d) if d.shells.is_some()),
        );
        h.run_steps(2);
        h.get(
            egui_kittest::kittest::By::new()
                .label(i18n::OPEN())
                .predicate(|n| !n.is_disabled()),
        )
        .click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Dialog::None));
        assert!(matches!(
            h.state().tabs.last().unwrap().kind,
            TabKind::Local { .. }
        ));
        assert!(h.state().profiles.is_empty());
    }
}
