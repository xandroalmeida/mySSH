//! Diálogos modais: perfil, configurações, conexão rápida e confirmações.

use egui::{Color32, Key, RichText};
use myssh_core::{Environment, ProfileId};

use crate::app::{App, Dialog, ProfileDialog, QuickDialog, VaultIssue, VaultMode};
use crate::env;
use crate::form::{AuthKind, SecretMode};
use crate::i18n;
use crate::logic::folder_path;
use crate::settings::{FONT_MAX, FONT_MIN, LOCK_MAX_MINUTES, ThemePref};
use crate::view::warn_color;

enum Next {
    Keep,
    Close,
    Save,
    UseSession,
    Unlock,
    PickKey,
    ConfirmDelete(ProfileId),
    ConfirmCloseTab(u64),
    ConfirmQuit,
    Quick(String),
}

fn error_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 120, 110)
    } else {
        Color32::from_rgb(180, 30, 30)
    }
}

impl App {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        let dialog = std::mem::replace(&mut self.dialog, Dialog::None);
        let (dialog, next) = match dialog {
            Dialog::None => (Dialog::None, Next::Keep),
            Dialog::Profile(mut d) => {
                let next = self.profile_dialog(ctx, &mut d);
                (Dialog::Profile(d), next)
            }
            Dialog::Settings => {
                let close = self.settings_dialog(ctx);
                (
                    Dialog::Settings,
                    if close { Next::Close } else { Next::Keep },
                )
            }
            Dialog::Quick(mut q) => {
                let next = quick_dialog(ctx, &mut q);
                (Dialog::Quick(q), next)
            }
            Dialog::ConfirmDelete(id) => {
                let name = self.profile(id).map(|p| p.name.clone()).unwrap_or_default();
                let next = confirm(
                    ctx,
                    "confirm-delete",
                    i18n::DELETE_TITLE,
                    &i18n::delete_body(&name),
                    i18n::DELETE,
                    Next::ConfirmDelete(id),
                );
                (Dialog::ConfirmDelete(id), next)
            }
            Dialog::ConfirmCloseTab(id) => {
                let title = self
                    .tabs
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.body.title())
                    .unwrap_or_default();
                let next = confirm(
                    ctx,
                    "confirm-close",
                    i18n::CLOSE_TAB_TITLE,
                    &i18n::close_tab_body(&title),
                    i18n::CLOSE_TAB_CONFIRM,
                    Next::ConfirmCloseTab(id),
                );
                (Dialog::ConfirmCloseTab(id), next)
            }
            Dialog::ConfirmQuit => {
                let next = confirm(
                    ctx,
                    "confirm-quit",
                    i18n::QUIT_TITLE,
                    i18n::QUIT_BODY,
                    i18n::QUIT_CONFIRM,
                    Next::ConfirmQuit,
                );
                (Dialog::ConfirmQuit, next)
            }
        };
        self.dialog = dialog;
        match next {
            Next::Keep => {}
            Next::Close => self.close_dialog(),
            Next::Save => self.submit_profile_dialog(ctx),
            Next::UseSession => {
                self.use_session_vault(ctx);
                self.submit_profile_dialog(ctx);
            }
            Next::Unlock => self.unlock_vault(ctx),
            Next::PickKey => self.pick_key_file(ctx),
            Next::ConfirmDelete(id) => {
                self.close_dialog();
                self.delete_profile(id);
            }
            Next::ConfirmCloseTab(id) => {
                self.close_dialog();
                self.close_tab(id);
            }
            Next::ConfirmQuit => self.confirm_quit(ctx),
            Next::Quick(text) => match self.confirm_quick(&text) {
                Ok(()) => self.close_dialog(),
                Err(e) => {
                    if let Dialog::Quick(q) = &mut self.dialog {
                        q.error = Some(e);
                    }
                }
            },
        }
    }

    // ------------------------------------------------------------------ perfil

    fn profile_dialog(&self, ctx: &egui::Context, d: &mut ProfileDialog) -> Next {
        let mut next = Next::Keep;
        let editing = d.form.existing.is_some();
        let max_h = (ctx.content_rect().height() - 190.0).max(240.0);
        let resp = egui::Modal::new(egui::Id::new("profile-dialog")).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.heading(if editing {
                i18n::EDIT_PROFILE_TITLE
            } else {
                i18n::NEW_PROFILE_TITLE
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| self.profile_form_ui(ui, d, editing, &mut next));
            ui.add_space(6.0);
            if let Some(e) = &d.error {
                ui.colored_label(error_color(ui), e);
            }
            match d.issue {
                Some(VaultIssue::Unavailable) => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(i18n::SESSION_ONLY_HELP);
                        if ui.button(i18n::USE_SESSION_ONLY).clicked() {
                            next = Next::UseSession;
                        }
                    });
                }
                Some(VaultIssue::Locked) if ui.button(i18n::UNLOCK_VAULT).clicked() => {
                    next = Next::Unlock;
                }
                Some(VaultIssue::Locked) => {}
                None => {}
            }
            ui.separator();
            ui.horizontal(|ui| {
                if d.saving {
                    ui.spinner();
                    ui.label(i18n::SAVING);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!d.saving, egui::Button::new(i18n::SAVE))
                        .clicked()
                    {
                        next = Next::Save;
                    }
                    if ui.button(i18n::CANCEL).clicked() {
                        next = Next::Close;
                    }
                });
            });
        });
        if resp.is_top_modal
            && !resp.any_popup_open
            && !d.saving
            && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            next = Next::Close;
        }
        next
    }

    fn profile_form_ui(
        &self,
        ui: &mut egui::Ui,
        d: &mut ProfileDialog,
        editing: bool,
        next: &mut Next,
    ) {
        let f = &mut d.form;
        let own_id = f.existing.as_ref().map(|p| p.id);
        egui::Grid::new("profile-grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label(i18n::F_NAME);
                ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(f32::INFINITY));
                ui.end_row();

                ui.label(i18n::F_HOST);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut f.host).desired_width(250.0));
                    ui.label(i18n::F_PORT);
                    ui.add(egui::TextEdit::singleline(&mut f.port).desired_width(60.0));
                });
                ui.end_row();

                ui.label(i18n::F_USER);
                ui.add(egui::TextEdit::singleline(&mut f.username).desired_width(250.0));
                ui.end_row();

                ui.label(i18n::F_AUTH);
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut f.auth, AuthKind::Password, i18n::AUTH_PASSWORD);
                    ui.radio_value(&mut f.auth, AuthKind::Key, i18n::AUTH_KEY);
                    ui.radio_value(&mut f.auth, AuthKind::Agent, i18n::AUTH_AGENT);
                    ui.radio_value(&mut f.auth, AuthKind::KeyboardInteractive, i18n::AUTH_KI);
                });
                ui.end_row();

                match f.auth {
                    AuthKind::Password => {
                        ui.label("");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut f.password_mode, SecretMode::Ask, i18n::PW_ASK);
                            ui.radio_value(&mut f.password_mode, SecretMode::Save, i18n::PW_SAVE);
                        });
                        ui.end_row();
                        if f.password_mode == SecretMode::Save {
                            ui.label(i18n::F_PASSWORD);
                            let keeps = editing
                                && f.existing.as_ref().is_some_and(|p| {
                                    matches!(
                                        p.auth,
                                        myssh_core::AuthMethod::Password {
                                            credential: Some(_)
                                        }
                                    )
                                });
                            ui.add(
                                egui::TextEdit::singleline(&mut f.password)
                                    .password(true)
                                    .hint_text(if keeps { i18n::KEEP_SAVED } else { "" })
                                    .desired_width(250.0),
                            );
                            ui.end_row();
                        }
                    }
                    AuthKind::Key => {
                        ui.label(i18n::F_KEY_PATH);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut f.key_path).desired_width(250.0),
                            );
                            if ui.button(i18n::BROWSE).clicked() {
                                *next = Next::PickKey;
                            }
                        });
                        ui.end_row();
                        ui.label(i18n::F_PASSPHRASE);
                        ui.vertical(|ui| {
                            ui.radio_value(&mut f.passphrase_mode, SecretMode::Ask, i18n::PP_ASK);
                            ui.radio_value(&mut f.passphrase_mode, SecretMode::Save, i18n::PP_SAVE);
                        });
                        ui.end_row();
                        if f.passphrase_mode == SecretMode::Save {
                            ui.label("");
                            let keeps = editing
                                && f.existing.as_ref().is_some_and(|p| {
                                    matches!(
                                        p.auth,
                                        myssh_core::AuthMethod::KeyFile {
                                            passphrase: Some(_),
                                            ..
                                        }
                                    )
                                });
                            ui.add(
                                egui::TextEdit::singleline(&mut f.passphrase)
                                    .password(true)
                                    .hint_text(if keeps { i18n::KEEP_SAVED } else { "" })
                                    .desired_width(250.0),
                            );
                            ui.end_row();
                        }
                    }
                    AuthKind::Agent => {
                        ui.label("");
                        ui.weak(i18n::AGENT_HELP);
                        ui.end_row();
                    }
                    AuthKind::KeyboardInteractive => {
                        ui.label("");
                        ui.weak(i18n::KI_HELP);
                        ui.end_row();
                    }
                }

                ui.label(i18n::F_FOLDER);
                ui.horizontal(|ui| {
                    let cur = f.folder.map_or_else(
                        || i18n::NO_FOLDER.to_string(),
                        |id| folder_path(&self.folders, id),
                    );
                    egui::ComboBox::from_id_salt("form-folder")
                        .selected_text(cur)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.folder, None, i18n::NO_FOLDER);
                            for fo in &self.folders {
                                ui.selectable_value(
                                    &mut f.folder,
                                    Some(fo.id),
                                    folder_path(&self.folders, fo.id),
                                );
                            }
                        });
                    ui.label(i18n::OR_NEW_FOLDER);
                    ui.add(egui::TextEdit::singleline(&mut f.new_folder).desired_width(120.0));
                });
                ui.end_row();

                ui.label(i18n::F_TAGS);
                ui.add(
                    egui::TextEdit::singleline(&mut f.tags)
                        .hint_text(i18n::TAGS_HINT)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label(i18n::F_ENV);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("form-env")
                        .selected_text(env::name(f.environment))
                        .show_ui(ui, |ui| {
                            for e in [
                                Environment::None,
                                Environment::Development,
                                Environment::Staging,
                                Environment::Production,
                            ] {
                                ui.selectable_value(&mut f.environment, e, env::name(e));
                            }
                        });
                    ui.checkbox(&mut f.favorite, i18n::F_FAVORITE);
                });
                ui.end_row();

                ui.label(i18n::F_JUMP);
                let cur = f
                    .jump
                    .and_then(|j| self.profile(j))
                    .map_or_else(|| i18n::NO_JUMP.to_string(), |p| p.name.clone());
                egui::ComboBox::from_id_salt("form-jump")
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.jump, None, i18n::NO_JUMP);
                        for p in self.profiles.iter().filter(|p| Some(p.id) != own_id) {
                            ui.selectable_value(&mut f.jump, Some(p.id), &p.name);
                        }
                    });
                ui.end_row();

                ui.label(i18n::F_KEEPALIVE);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.keepalive)
                            .hint_text(i18n::KEEPALIVE_OFF)
                            .desired_width(70.0),
                    );
                    ui.label(i18n::F_TIMEOUT);
                    ui.add(egui::TextEdit::singleline(&mut f.timeout).desired_width(50.0));
                });
                ui.end_row();
            });
    }

    // ----------------------------------------------------------- configurações

    fn settings_dialog(&mut self, ctx: &egui::Context) -> bool {
        let mut close = false;
        let mut lock_now = false;
        let mut unlock = false;
        let locked = self.vault.is_locked();
        let session_only = self.vault_mode == VaultMode::SessionOnly;
        let resp = egui::Modal::new(egui::Id::new("settings-dialog")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(i18n::SETTINGS_TITLE);
            ui.add_space(6.0);
            let s = &mut self.settings;
            egui::Grid::new("settings-grid")
                .num_columns(2)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    ui.label(i18n::S_THEME);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut s.theme, ThemePref::System, i18n::THEME_SYSTEM);
                        ui.radio_value(&mut s.theme, ThemePref::Light, i18n::THEME_LIGHT);
                        ui.radio_value(&mut s.theme, ThemePref::Dark, i18n::THEME_DARK);
                    });
                    ui.end_row();

                    ui.label(i18n::S_FONT);
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Slider::new(&mut s.terminal_font_size, FONT_MIN..=FONT_MAX)
                                .suffix(" pt")
                                .step_by(0.5),
                        );
                        ui.label(
                            RichText::new(i18n::FONT_SAMPLE)
                                .monospace()
                                .size(s.terminal_font_size),
                        );
                    });
                    ui.end_row();

                    ui.label(i18n::S_PASTE);
                    ui.checkbox(&mut s.confirm_multiline_paste, i18n::S_PASTE_HELP);
                    ui.end_row();

                    ui.label(i18n::S_PROD);
                    ui.checkbox(&mut s.prod_warning, i18n::S_PROD_HELP);
                    ui.end_row();

                    ui.label(i18n::S_VAULT_TIMEOUT);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut s.vault_lock_minutes)
                                .range(0..=LOCK_MAX_MINUTES)
                                .suffix(" min"),
                        );
                        ui.weak(i18n::S_VAULT_TIMEOUT_HELP);
                    });
                    ui.end_row();
                });
            ui.separator();
            ui.label(RichText::new(i18n::S_VAULT).strong());
            if session_only {
                ui.colored_label(warn_color(ui), i18n::S_VAULT_SESSION_ONLY);
            }
            ui.horizontal(|ui| {
                if locked {
                    ui.label(i18n::VAULT_LOCKED_BADGE);
                    if ui.button(i18n::UNLOCK_VAULT).clicked() {
                        unlock = true;
                    }
                } else if ui.button(i18n::LOCK_NOW).clicked() {
                    lock_now = true;
                }
            });
            ui.weak(i18n::LOCK_NOW_HELP);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(i18n::CLOSE).clicked() {
                    close = true;
                }
            });
        });
        if resp.should_close() {
            close = true;
        }
        if lock_now {
            self.vault.lock();
            self.set_notice(i18n::VAULT_LOCKED_NOTICE);
        }
        if unlock {
            self.unlock_vault(ctx);
        }
        close
    }
}

// ------------------------------------------------------------- conexão rápida

fn quick_dialog(ctx: &egui::Context, q: &mut QuickDialog) -> Next {
    let mut next = Next::Keep;
    let resp = egui::Modal::new(egui::Id::new("quick-dialog")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(i18n::QUICK_TITLE);
        ui.add_space(6.0);
        ui.label(i18n::QUICK_HELP);
        let te = ui.add(
            egui::TextEdit::singleline(&mut q.text)
                .hint_text(i18n::QUICK_HINT)
                .desired_width(f32::INFINITY),
        );
        if !te.has_focus() && q.text.is_empty() {
            te.request_focus();
        }
        let enter = te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if let Some(e) = &q.error {
            ui.colored_label(error_color(ui), e);
        }
        ui.add_space(6.0);
        ui.weak(i18n::QUICK_NOTE);
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(i18n::CONNECT).clicked() || enter {
                next = Next::Quick(q.text.clone());
            }
            if ui.button(i18n::CANCEL).clicked() {
                next = Next::Close;
            }
        });
    });
    if resp.should_close() {
        next = Next::Close;
    }
    next
}

// ---------------------------------------------------------------- confirmações

fn confirm(
    ctx: &egui::Context,
    id: &'static str,
    title: &str,
    body: &str,
    confirm_label: &str,
    on_confirm: Next,
) -> Next {
    let mut result = Next::Keep;
    let mut on_confirm = Some(on_confirm);
    let resp = egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading(title);
        ui.add_space(6.0);
        ui.label(body);
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(confirm_label).clicked() {
                result = on_confirm.take().unwrap_or(Next::Close);
            }
            if ui.button(i18n::CANCEL).clicked() {
                result = Next::Close;
            }
        });
    });
    if resp.should_close() {
        result = Next::Close;
    }
    result
}
