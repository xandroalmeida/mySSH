//! Janela principal: barra superior, sidebar de perfis e área de abas.

use std::time::Duration;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, RichText, Stroke, TextStyle};
use myssh_core::{Environment, Profile, ProfileId};

use crate::app::{App, Dialog, KeyringStatus, QuickDialog, VaultMode};
use crate::env;
use crate::i18n;
use crate::logic::{FolderSel, folder_path, group_by_folder, matches};
use crate::shortcuts::{self, Action};
use crate::split::Dir;
use crate::split_body::env_banner;
use crate::tabs::{TabBody, TabKind};

const SEARCH_ID: &str = "sidebar-search";

enum RowAction {
    Select(ProfileId),
    Open(ProfileId),
    Edit(ProfileId),
    Duplicate(ProfileId),
    Delete(ProfileId),
    ToggleFavorite(ProfileId),
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if self.vault_mode == VaultMode::Os && !self.keyring_check_started {
            // verificação do cofre do SO fora da thread de UI
            self.keyring_check_started = true;
            self.check_keyring(&ctx);
        }
        self.drain_messages(&ctx);
        self.vault_housekeeping(&ctx);
        self.settings_housekeeping(&ctx);
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);
        self.pump_tabs(&ctx);
        if let Some(mut d) = self.demo.take() {
            d.tick(self, &ctx);
            self.demo = Some(d);
        }

        self.top_panel(ui);
        self.notice_panel(ui);
        self.sidebar(ui);
        self.central(ui);
        self.dialogs(&ctx);
        self.tab_modals(&ctx);
        self.process_tab_events();
    }

    fn on_exit(&mut self) {
        self.close_all_tabs();
    }
}

impl App {
    // ------------------------------------------------------------ barra superior

    fn top_panel(&mut self, ui: &mut egui::Ui) {
        let can_split = self.can_split();
        let splitting = self.tabs.iter().any(|t| {
            Some(t.id) == self.active && t.body.as_split_ref().is_some_and(|s| s.pane_count() > 1)
        });
        let sessions = self.active_sessions();
        let locked = self.vault.is_locked();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button(i18n::NEW_PROFILE())
                    .on_hover_text(shortcuts::label(Action::NewProfile))
                    .clicked()
                {
                    self.new_profile_dialog();
                }
                if ui.button(i18n::QUICK_CONNECT()).clicked() {
                    self.dialog = Dialog::Quick(QuickDialog {
                        text: String::new(),
                        error: None,
                    });
                }
                ui.add_enabled_ui(can_split, |ui| {
                    ui.menu_button(i18n::SPLIT(), |ui| self.split_menu(ui, splitting));
                });
                if ui.button(i18n::IMPORT_SSH()).clicked() {
                    self.open_import_ssh();
                }
                if ui.button(i18n::SETTINGS()).clicked() {
                    self.open_settings();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(i18n::ABOUT_BUTTON()).frame(false))
                        .on_hover_text(i18n::ABOUT_BUTTON_HELP())
                        .clicked()
                    {
                        self.open_about();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(sessions > 0, egui::Button::new(i18n::END_ALL()))
                        .on_hover_text(i18n::END_ALL_HELP())
                        .on_disabled_hover_text(i18n::END_ALL_NONE())
                        .clicked()
                    {
                        self.request_end_all();
                    }
                    ui.separator();
                    if locked {
                        let b = egui::Button::new(
                            RichText::new(i18n::VAULT_LOCKED_BADGE()).color(warn_color(ui)),
                        );
                        if ui
                            .add(b)
                            .on_hover_text(i18n::VAULT_UNLOCK_BTN_HELP())
                            .clicked()
                        {
                            let ctx = ui.ctx().clone();
                            self.unlock_vault(&ctx);
                        }
                    } else if ui
                        .button(i18n::VAULT_OPEN_BADGE())
                        .on_hover_text(i18n::VAULT_LOCK_BTN_HELP())
                        .clicked()
                    {
                        self.lock_vault();
                    }
                    if self.vault_mode == VaultMode::SessionOnly {
                        ui.weak(i18n::VAULT_SESSION_BADGE());
                    }
                });
            });
            ui.add_space(2.0);
            let mut use_session = false;
            if let Some(w) = &self.store_warning {
                ui.colored_label(warn_color(ui), format!("⚠ {w}"));
            }
            if let KeyringStatus::Unavailable(reason) = &self.keyring {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        warn_color(ui),
                        format!("⚠ {} ({reason})", i18n::KEYRING_UNAVAILABLE()),
                    );
                    if ui.button(i18n::USE_SESSION_ONLY()).clicked() {
                        use_session = true;
                    }
                });
            }
            if use_session {
                let ctx = ui.ctx().clone();
                self.use_session_vault(&ctx);
            }
            ui.add_space(2.0);
        });
    }

    fn split_menu(&mut self, ui: &mut egui::Ui, splitting: bool) {
        let entries = [
            (Dir::Vertical, i18n::SPLIT_SIDE(), Action::SplitSide),
            (Dir::Horizontal, i18n::SPLIT_STACK(), Action::SplitStack),
        ];
        for (dir, name, action) in entries {
            ui.label(RichText::new(name).strong());
            if ui
                .button(format!(
                    "  {}    {}",
                    i18n::SPLIT_SAME(),
                    shortcuts::label(action)
                ))
                .clicked()
            {
                self.split_active(dir, None);
                ui.close();
            }
            if ui.button(format!("  {}", i18n::SPLIT_OTHER())).clicked() {
                self.open_split_picker(dir);
                ui.close();
            }
            ui.separator();
        }
        if ui
            .add_enabled(
                splitting,
                egui::Button::new(format!(
                    "{}    {}",
                    i18n::SPLIT_CLOSE_PANE(),
                    shortcuts::label(Action::ClosePane)
                )),
            )
            .clicked()
        {
            if let Some(id) = self.active {
                let pane = self
                    .tabs
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(|t| t.body.as_split_ref())
                    .map(|s| s.focused_id());
                if let Some(pane) = pane {
                    self.request_close_pane(id, pane);
                }
            }
            ui.close();
        }
    }

    fn notice_panel(&mut self, ui: &mut egui::Ui) {
        const TTL: Duration = Duration::from_secs(7);
        let Some((msg, at)) = &self.notice else {
            return;
        };
        if at.elapsed() > TTL {
            self.notice = None;
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        let msg = msg.clone();
        let mut dismiss = false;
        egui::Panel::bottom("notice").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(msg);
                if ui.small_button("×").clicked() {
                    dismiss = true;
                }
            });
        });
        if dismiss {
            self.notice = None;
        }
    }

    // ------------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<RowAction> = None;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(300.0)
            .size_range(220.0..=520.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.filter.search)
                        .id(egui::Id::new(SEARCH_ID))
                        .hint_text(i18n::SEARCH_HINT())
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut self.focus_search) {
                    search.request_focus();
                }
                self.filters_ui(ui);
                ui.separator();
                self.selected_actions_ui(ui, &mut action);
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.profile_list_ui(ui, &mut action));
            });
        if let Some(a) = action {
            self.apply_row_action(a);
        }
    }

    fn filters_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.filter.favorites_only, i18n::FILTER_FAVORITES());
            if self.filter.is_active() && ui.small_button(i18n::FILTER_CLEAR()).clicked() {
                self.filter = Default::default();
            }
        });
        ui.horizontal(|ui| {
            ui.label(i18n::FILTER_ENV());
            let cur = self.filter.environment.map_or(i18n::ALL(), env::name);
            egui::ComboBox::from_id_salt("filter-env")
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.environment, None, i18n::ALL());
                    for e in ENVIRONMENTS {
                        ui.selectable_value(&mut self.filter.environment, Some(e), env::name(e));
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(i18n::FILTER_FOLDER());
            let cur = match self.filter.folder {
                FolderSel::All => i18n::ALL_F().to_string(),
                FolderSel::Unfiled => i18n::NO_FOLDER().to_string(),
                FolderSel::Folder(id) => folder_path(&self.folders, id),
            };
            egui::ComboBox::from_id_salt("filter-folder")
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.folder, FolderSel::All, i18n::ALL_F());
                    ui.selectable_value(
                        &mut self.filter.folder,
                        FolderSel::Unfiled,
                        i18n::NO_FOLDER(),
                    );
                    let mut fs: Vec<_> = self
                        .folders
                        .iter()
                        .map(|f| (folder_path(&self.folders, f.id), f.id))
                        .collect();
                    fs.sort_by_key(|(p, _)| p.to_lowercase());
                    for (path, id) in fs {
                        ui.selectable_value(&mut self.filter.folder, FolderSel::Folder(id), path);
                    }
                });
        });
    }

    fn selected_actions_ui(&self, ui: &mut egui::Ui, action: &mut Option<RowAction>) {
        let sel = self.selected;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(sel.is_some(), |ui| {
                let id = sel.unwrap_or_default();
                if ui
                    .button(i18n::OPEN())
                    .on_hover_text(shortcuts::label(Action::NewTab))
                    .clicked()
                {
                    *action = Some(RowAction::Open(id));
                }
                if ui.button(i18n::EDIT()).clicked() {
                    *action = Some(RowAction::Edit(id));
                }
                if ui.button(i18n::DUPLICATE()).clicked() {
                    *action = Some(RowAction::Duplicate(id));
                }
                if ui.button(i18n::DELETE()).clicked() {
                    *action = Some(RowAction::Delete(id));
                }
            });
        });
    }

    fn profile_list_ui(&self, ui: &mut egui::Ui, action: &mut Option<RowAction>) {
        let visible: Vec<&Profile> = self
            .profiles
            .iter()
            .filter(|p| matches(p, &self.filter))
            .collect();
        if visible.is_empty() {
            ui.add_space(8.0);
            ui.weak(if self.profiles.is_empty() {
                i18n::NO_PROFILES()
            } else {
                i18n::NO_MATCH()
            });
            return;
        }
        let force_open = self.filter.is_active();
        for g in group_by_folder(&visible, &self.folders) {
            let header = format!("{} ({})", g.label, g.items.len());
            let mut ch = egui::CollapsingHeader::new(header)
                .id_salt(("group", g.folder))
                .default_open(true);
            if force_open {
                ch = ch.open(Some(true));
            }
            ch.show(ui, |ui| {
                for &i in &g.items {
                    self.profile_row(ui, visible[i], action);
                }
            });
        }
    }

    fn profile_row(&self, ui: &mut egui::Ui, p: &Profile, action: &mut Option<RowAction>) {
        let selected = self.selected == Some(p.id);
        let resp = ui.add(
            egui::Button::selectable(selected, row_job(ui, p))
                .min_size(egui::vec2(ui.available_width(), 0.0)),
        );
        if resp.clicked() {
            *action = Some(RowAction::Select(p.id));
        }
        if resp.double_clicked() {
            *action = Some(RowAction::Open(p.id));
        }
        resp.context_menu(|ui| {
            let fav_label = if p.favorite {
                i18n::UNFAVORITE()
            } else {
                i18n::FAVORITE()
            };
            let mut picked = None;
            if ui.button(i18n::OPEN()).clicked() {
                picked = Some(RowAction::Open(p.id));
            } else if ui.button(i18n::EDIT()).clicked() {
                picked = Some(RowAction::Edit(p.id));
            } else if ui.button(i18n::DUPLICATE()).clicked() {
                picked = Some(RowAction::Duplicate(p.id));
            } else if ui.button(fav_label).clicked() {
                picked = Some(RowAction::ToggleFavorite(p.id));
            } else if ui.button(i18n::DELETE()).clicked() {
                picked = Some(RowAction::Delete(p.id));
            }
            if picked.is_some() {
                *action = picked;
                ui.close();
            }
        });
    }

    fn apply_row_action(&mut self, a: RowAction) {
        match a {
            RowAction::Select(id) => self.selected = Some(id),
            RowAction::Open(id) => {
                self.selected = Some(id);
                self.open_profile(id);
            }
            RowAction::Edit(id) => {
                self.selected = Some(id);
                self.edit_profile_dialog(id);
            }
            RowAction::Duplicate(id) => self.duplicate(id),
            RowAction::Delete(id) => {
                self.selected = Some(id);
                self.dialog = Dialog::ConfirmDelete(id);
            }
            RowAction::ToggleFavorite(id) => self.toggle_favorite(id),
        }
    }

    // ------------------------------------------------------------ área de abas

    fn central(&mut self, ui: &mut egui::Ui) {
        let mut select = None;
        let mut close = None;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("tabbar")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for t in &self.tabs {
                            let active = self.active == Some(t.id);
                            // em abas divididas, o estilo segue o painel focado
                            let style_kind =
                                t.body.as_split_ref().map_or(&t.kind, |s| s.focused_kind());
                            let r = ui.selectable_label(active, tab_job(ui, style_kind, &*t.body));
                            if let Some(a) = tab_accent(style_kind) {
                                ui.painter().line_segment(
                                    [r.rect.left_bottom(), r.rect.right_bottom()],
                                    Stroke::new(3.0, a),
                                );
                            }
                            let r = match t.body.status() {
                                Some(b) => r.on_hover_text(format!("{} {}", b.icon, b.text)),
                                None => r,
                            };
                            if r.clicked() {
                                select = Some(t.id);
                            }
                            if r.middle_clicked() {
                                close = Some(t.id);
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text(i18n::CLOSE_TAB())
                                .clicked()
                            {
                                close = Some(t.id);
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
            ui.separator();
            let prod_warning = self.settings.prod_warning;
            let active = self.active;
            let ctx = ui.ctx().clone();
            let (tabs, env_services) = self.tabs_and_env(&ctx);
            match tabs.iter_mut().find(|t| Some(t.id) == active) {
                Some(t) => {
                    let manages = t.body.manages_chrome();
                    let accent = tab_accent(&t.kind);
                    let tab_env = env_services.with_profile_theme(&t.kind);
                    let (profile_info, environment) = match &t.kind {
                        TabKind::Terminal { profile, .. } => (
                            Some(format!("{}@{}", profile.username, profile.host)),
                            profile.environment,
                        ),
                        TabKind::Welcome => (None, Environment::None),
                    };
                    let st = env::style(environment)
                        .filter(|_| environment != Environment::Production || prod_warning);
                    let border = match st {
                        Some(s) if environment == Environment::Production && !manages => {
                            Stroke::new(3.0, s.fill)
                        }
                        _ => Stroke::NONE,
                    };
                    egui::Frame::new()
                        .stroke(border)
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            if manages {
                                // painéis divididos: cada painel desenha seu aviso e borda
                                t.body.ui(ui, &env_services);
                                return;
                            }
                            if let Some(a) = accent {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 3.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(r, 1.0, a);
                                ui.add_space(2.0);
                            }
                            if let (Some(s), Some(info)) = (st, profile_info) {
                                env_banner(ui, s, &info);
                            }
                            if t.body.fills_space() {
                                t.body.ui(ui, &tab_env);
                            } else {
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| t.body.ui(ui, &tab_env));
                            }
                        });
                }
                None => {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| ui.weak(i18n::NO_TABS()));
                }
            }
        });
        if let Some(id) = select {
            self.active = Some(id);
        }
        if let Some(id) = close {
            self.request_close_tab(id);
        }
    }
}

const ENVIRONMENTS: [Environment; 4] = [
    Environment::Production,
    Environment::Staging,
    Environment::Development,
    Environment::None,
];

pub fn warn_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 200, 80)
    } else {
        Color32::from_rgb(160, 90, 0)
    }
}

/// Cor de destaque do perfil da aba, se houver.
fn tab_accent(kind: &TabKind) -> Option<Color32> {
    match kind {
        TabKind::Terminal { profile, .. } => profile
            .theme
            .accent
            .map(|[r, g, b]| Color32::from_rgb(r, g, b)),
        TabKind::Welcome => None,
    }
}

fn row_job(ui: &egui::Ui, p: &Profile) -> LayoutJob {
    let base = ui.visuals().text_color();
    let mut job = LayoutJob::default();
    let body = TextStyle::Body.resolve(ui.style());
    let small = TextStyle::Small.resolve(ui.style());
    if p.favorite {
        job.append(
            "★ ",
            0.0,
            TextFormat {
                font_id: body.clone(),
                color: Color32::from_rgb(235, 175, 30),
                ..Default::default()
            },
        );
    }
    job.append(
        &p.name,
        0.0,
        TextFormat {
            font_id: body,
            color: base,
            ..Default::default()
        },
    );
    if let Some(s) = env::style(p.environment) {
        job.append(
            &format!(" {} {} ", s.icon, s.label),
            10.0,
            TextFormat {
                font_id: small.clone(),
                color: s.text,
                background: s.fill,
                ..Default::default()
            },
        );
    }
    job.append(
        &format!("\n{}@{}:{}", p.username, p.host, p.port),
        0.0,
        TextFormat {
            font_id: small,
            color: ui.visuals().weak_text_color(),
            ..Default::default()
        },
    );
    job
}

fn tab_job(ui: &egui::Ui, kind: &TabKind, body: &dyn TabBody) -> LayoutJob {
    let title = body.title();
    let mut job = LayoutJob::default();
    let font: FontId = TextStyle::Button.resolve(ui.style());
    if let Some(b) = body.status() {
        job.append(
            &format!("{} ", b.icon),
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: b.color,
                ..Default::default()
            },
        );
    }
    if let Some(s) = env::style(kind.environment()) {
        job.append(
            &format!("{} ", s.icon),
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: s.fill,
                ..Default::default()
            },
        );
    }
    job.append(
        &title,
        0.0,
        TextFormat {
            font_id: font,
            color: ui.visuals().text_color(),
            ..Default::default()
        },
    );
    job
}
