//! Seletor de shell do formulário de perfil e aba de console local.
use std::sync::{Arc, mpsc};

use egui::{Color32, FontId};
use myssh_term::{DEFAULT_PUMP_BUDGET, TermSession, TermStyle, TermTheme};
use zeroize::Zeroizing;

use crate::i18n;
use crate::local_shell::{self, Shell};
use crate::tabs::{StatusBadge, TabBody, TabEnv};

#[derive(Default)]
pub struct ShellPicker {
    rx: Option<mpsc::Receiver<Vec<Shell>>>,
    pub shells: Option<Vec<Shell>>,
}

impl ShellPicker {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rt: &tokio::runtime::Handle,
        selected: &mut std::path::PathBuf,
    ) {
        if self.rx.is_none() && self.shells.is_none() {
            let (tx, rx) = mpsc::channel();
            let ctx = ui.ctx().clone();
            rt.spawn_blocking(move || {
                let _ = tx.send(local_shell::discover());
                ctx.request_repaint();
            });
            self.rx = Some(rx);
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(shells) => self.shells = Some(shells),
                Err(mpsc::TryRecvError::Disconnected) => self.shells = Some(Vec::new()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.shells.is_some() {
            self.rx = None;
        }
        match &self.shells {
            None => {
                ui.spinner();
                ui.label(i18n::LOCAL_LOADING());
            }
            Some(shells) => {
                // Edição preserva a escolha salva mesmo se o executável não existe neste SO.
                if selected.as_os_str().is_empty()
                    && let Some(shell) = shells.first()
                {
                    *selected = shell.path.clone();
                }
                let label = shells
                    .iter()
                    .find(|s| s.path == *selected)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| {
                        if selected.as_os_str().is_empty() {
                            i18n::LOCAL_EMPTY().into()
                        } else {
                            selected.display().to_string()
                        }
                    });
                egui::ComboBox::from_id_salt("profile-local-shell")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for shell in shells {
                            ui.selectable_value(
                                selected,
                                shell.path.clone(),
                                format!("{} ({})", shell.name, shell.path.display()),
                            );
                        }
                    });
                if !selected.as_os_str().is_empty() {
                    ui.weak(selected.display().to_string());
                }
                ui.weak(i18n::LOCAL_HELP());
            }
        }
    }
}

pub struct LocalConsole {
    name: String,
    shell: Shell,
    sess: Option<TermSession>,
    paste: Option<Zeroizing<String>>,
    paste_fresh: bool,
    focused: bool,
    closing: bool,
}

impl LocalConsole {
    pub fn new(name: String, shell: Shell) -> Self {
        Self {
            name,
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
        self.name.clone()
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
        let theme = env.theme.clone().unwrap_or_else(|| {
            if env.dark {
                TermTheme::dark()
            } else {
                TermTheme::light()
            }
        });
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

    #[cfg(test)]
    fn debug_screen(&self) -> String {
        self.sess
            .as_ref()
            .map_or_else(String::new, TermSession::screen_text)
    }
    #[cfg(test)]
    fn debug_send(&self, bytes: &[u8]) {
        if let Some(sess) = &self.sess {
            sess.send_input(bytes.to_vec());
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
    use crate::app::{App, Dialog};
    use crate::form::{AuthKind, ProfileForm, SecretMode, TransportKind};
    use crate::tabs::TabKind;
    use crate::ui_tests::{harness, memory_app, wait_until};
    use egui_kittest::kittest::Queryable;
    use myssh_core::{AuthMethod, CredentialId, Transport};

    fn form_mut(app: &mut App) -> &mut ProfileForm {
        let Dialog::Profile(d) = &mut app.dialog else {
            panic!("formulário esperado")
        };
        &mut d.form
    }

    fn test_shell() -> std::path::PathBuf {
        #[cfg(unix)]
        {
            "/bin/sh".into()
        }
        #[cfg(windows)]
        {
            local_shell::discover()
                .into_iter()
                .find(|s| s.path.ends_with("cmd.exe"))
                .unwrap()
                .path
        }
    }

    #[test]
    fn console_e_criado_editado_duplicado_e_aberto_como_perfil() {
        let mut h = harness(memory_app());
        h.run_steps(2);
        assert!(h.query_by_label(i18n::LOCAL_CONSOLE()).is_none());
        h.get_by_label(i18n::NEW_PROFILE()).click();
        h.run_steps(3);
        form_mut(h.state_mut()).name = "My local shell".into();
        for _ in 0..3 {
            h.get_by_label(i18n::LOCAL_CONSOLE()).click();
            h.run_steps(3);
            if form_mut(h.state_mut()).transport == TransportKind::Local {
                break;
            }
        }
        assert_eq!(form_mut(h.state_mut()).transport, TransportKind::Local);
        wait_until(
            &mut h,
            "seletor de shell",
            |a| matches!(&a.dialog, Dialog::Profile(d) if d.local_shells.shells.is_some()),
        );
        assert!(h.query_by_label(i18n::F_HOST()).is_none());
        assert!(h.query_by_label(i18n::F_AUTH()).is_none());
        assert!(h.query_by_label(i18n::F_JUMP()).is_none());
        // O shell escolhido fica no próprio perfil, não em uma preferência separada.
        form_mut(h.state_mut()).local_shell = test_shell();
        h.get_by_label(i18n::SAVE()).click();
        h.run_steps(3);
        wait_until(&mut h, "perfil local salvo", |a| {
            a.profiles.len() == 1 && matches!(a.dialog, Dialog::None)
        });
        let profile = h.state().profiles[0].clone();
        assert!(matches!(&profile.transport, Transport::Local { shell } if *shell == test_shell()));
        assert!(h.state().store.list_credentials().unwrap().is_empty());
        h.state_mut().open_profile(profile.id);
        h.run_steps(3);
        assert_eq!(
            h.state().tabs.last().unwrap().body.title(),
            "My local shell"
        );
        assert!(matches!(
            h.state().tabs.last().unwrap().kind,
            TabKind::Terminal { saved: true, .. }
        ));
        // Perfis locais também funcionam nos painéis divididos existentes.
        h.state_mut()
            .split_active(crate::split::Dir::Vertical, None);
        h.run_steps(3);
        assert_eq!(
            h.state()
                .tabs
                .last()
                .unwrap()
                .body
                .as_split_ref()
                .unwrap()
                .pane_count(),
            2
        );
        h.state_mut().edit_profile_dialog(profile.id);
        h.run_steps(3);
        assert_eq!(form_mut(h.state_mut()).local_shell, test_shell());
        form_mut(h.state_mut()).name = "Renamed shell".into();
        h.get_by_label(i18n::SAVE()).click();
        h.run_steps(3);
        assert_eq!(h.state().profiles[0].name, "Renamed shell");
        h.state_mut().duplicate(profile.id);
        h.run_steps(3);
        assert_eq!(h.state().profiles.len(), 2);
        assert!(
            h.state()
                .profiles
                .iter()
                .all(|p| p.transport == profile.transport)
        );
    }

    #[cfg(unix)]
    #[test]
    fn abrir_perfil_local_executa_shell_escolhido_na_aba() {
        let mut app = memory_app();
        let mut f = ProfileForm::blank();
        f.transport = TransportKind::Local;
        f.local_shell = test_shell();
        f.name = "Saved shell".into();
        let p = f.build(&[]).unwrap().profile;
        app.store.create_profile(&p).unwrap();
        app.reload();
        app.auto_connect = true;
        app.open_profile(p.id);
        let mut h = harness(app);
        wait_until(&mut h, "shell conectado", |a| {
            a.tabs
                .last()
                .unwrap()
                .body
                .status()
                .is_some_and(|b| b.text == i18n::ST_CONNECTED())
        });
        h.state()
            .tabs
            .last()
            .unwrap()
            .body
            .debug_send(b"printf '__MYSSH_%s__\\n' PROFILE_LOCAL\r");
        wait_until(&mut h, "saída do shell escolhido", |a| {
            a.tabs
                .last()
                .unwrap()
                .body
                .debug_screen()
                .contains("__MYSSH_PROFILE_LOCAL__")
        });
        h.state().tabs.last().unwrap().body.debug_send(b"exit\r");
        wait_until(&mut h, "shell encerrado", |a| {
            a.tabs.last().unwrap().body.is_closed()
        });
    }

    #[test]
    fn formulario_local_ignora_campos_ssh_e_libera_credenciais_antigas() {
        let mut old = myssh_core::Profile::new("Old SSH", "host", "user");
        let cred = CredentialId::new();
        old.auth = AuthMethod::Password {
            credential: Some(cred),
        };
        old.agent_forwarding = true;
        let mut f = ProfileForm::from_profile(&old);
        f.transport = TransportKind::Local;
        f.local_shell = test_shell();
        f.name = "Local".into();
        f.port = "invalid".into();
        f.keepalive = "invalid".into();
        f.timeout.clear();
        f.auth = AuthKind::Password;
        f.password_mode = SecretMode::Save;
        f.password = "test-only-unused".into();
        f.jump = Some(myssh_core::ProfileId::new());
        let draft = f.build(&[]).unwrap();
        assert!(draft.new_secret.is_none());
        assert_eq!(draft.obsolete, vec![cred]);
        assert!(draft.profile.host.is_empty() && draft.profile.username.is_empty());
        assert!(!draft.profile.agent_forwarding);
        assert!(draft.profile.jump.is_none());
        assert_eq!(draft.profile.auth, AuthMethod::Agent);
        assert_eq!(draft.profile.validate(), Ok(()));
        let roundtrip = ProfileForm::from_profile(&draft.profile)
            .build(&[])
            .unwrap();
        assert_eq!(roundtrip.profile, draft.profile);
    }
}
