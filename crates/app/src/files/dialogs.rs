//! Diálogos modais do painel: conflito, nova pasta, renomear e exclusão.

use egui::{Color32, RichText};

use super::contract::RemoteOrLocalMeta;
use super::fmt::{fmt_mtime, fmt_size};
use super::model::{Entry, Side};
use super::name_problem;
use super::plan::ConflictChoice;
use super::queue::PendingConflict;
use crate::i18n;

pub enum FsDialog {
    NewDir {
        side: Side,
        name: String,
        error: Option<String>,
        fresh: bool,
    },
    Rename {
        side: Side,
        from: String,
        old_name: String,
        name: String,
        error: Option<String>,
        fresh: bool,
    },
    Delete {
        side: Side,
        items: Vec<Entry>,
        ack: bool,
    },
}

impl FsDialog {
    pub fn new_dir(side: Side) -> Self {
        Self::NewDir {
            side,
            name: String::new(),
            error: None,
            fresh: true,
        }
    }

    pub fn rename(side: Side, e: &Entry) -> Self {
        Self::Rename {
            side,
            from: e.path.clone(),
            old_name: e.name.clone(),
            name: e.name.clone(),
            error: None,
            fresh: true,
        }
    }

    pub fn delete(side: Side, items: Vec<Entry>) -> Self {
        Self::Delete {
            side,
            items,
            ack: false,
        }
    }

    pub fn side(&self) -> Side {
        match self {
            Self::NewDir { side, .. } | Self::Rename { side, .. } | Self::Delete { side, .. } => {
                *side
            }
        }
    }

    /// A exclusão atinge pastas (recursiva): exige o "entendo".
    pub fn delete_needs_ack(items: &[Entry]) -> bool {
        items.iter().any(Entry::is_dir)
    }
}

pub enum DialogResult {
    Keep,
    Close,
    Mkdir(Side, String),
    Rename(Side, String, String),
    Delete(Side, Vec<Entry>),
}

fn err_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 120, 110)
    } else {
        Color32::from_rgb(180, 30, 30)
    }
}

/// `Some(true)` = confirmar, `Some(false)` = cancelar.
fn button_row(ui: &mut egui::Ui, confirm: &str, enabled: bool) -> Option<bool> {
    let mut out = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .add_enabled(enabled, egui::Button::new(confirm))
            .clicked()
        {
            out = Some(true);
        }
        if ui.button(i18n::CANCEL()).clicked() {
            out = Some(false);
        }
    });
    out
}

/// Campo de nome; `true` quando o usuário apertou Enter nele.
fn name_input(
    ui: &mut egui::Ui,
    name: &mut String,
    error: &Option<String>,
    fresh: &mut bool,
) -> bool {
    ui.label(i18n::FILES_NAME_LABEL());
    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
    r.widget_info(|| super::column::text_info(i18n::FILES_NAME_FIELD(), name));
    if *fresh {
        r.request_focus();
        *fresh = false;
    }
    if let Some(e) = error {
        ui.colored_label(err_color(ui), e.as_str());
    }
    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

pub fn fs_dialog(ctx: &egui::Context, d: &mut FsDialog, existing: &[String]) -> DialogResult {
    match d {
        FsDialog::NewDir {
            side,
            name,
            error,
            fresh,
        } => {
            let mut decision = None;
            let resp = egui::Modal::new(egui::Id::new("files-newdir")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading(i18n::FILES_NEWDIR_TITLE());
                ui.add_space(6.0);
                let enter = name_input(ui, name, error, fresh);
                ui.add_space(8.0);
                decision = button_row(ui, i18n::FILES_CREATE(), true);
                if enter {
                    decision = Some(true);
                }
            });
            match decision {
                Some(true) => match name_problem(name, existing, None) {
                    Some(m) => *error = Some(m),
                    None => return DialogResult::Mkdir(*side, name.trim().to_string()),
                },
                Some(false) => return DialogResult::Close,
                None if resp.should_close() => return DialogResult::Close,
                None => {}
            }
        }
        FsDialog::Rename {
            side,
            from,
            old_name,
            name,
            error,
            fresh,
        } => {
            let mut decision = None;
            let resp = egui::Modal::new(egui::Id::new("files-rename")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading(i18n::FILES_RENAME_TITLE());
                ui.add_space(6.0);
                let enter = name_input(ui, name, error, fresh);
                ui.add_space(8.0);
                decision = button_row(ui, i18n::FILES_RENAME_CONFIRM(), true);
                if enter {
                    decision = Some(true);
                }
            });
            match decision {
                Some(true) => {
                    if name.trim() == old_name.as_str() {
                        return DialogResult::Close;
                    }
                    match name_problem(name, existing, None) {
                        Some(m) => *error = Some(m),
                        None => {
                            return DialogResult::Rename(
                                *side,
                                from.clone(),
                                name.trim().to_string(),
                            );
                        }
                    }
                }
                Some(false) => return DialogResult::Close,
                None if resp.should_close() => return DialogResult::Close,
                None => {}
            }
        }
        FsDialog::Delete { side, items, ack } => {
            let needs_ack = FsDialog::delete_needs_ack(items);
            let mut decision = None;
            let remote = *side == Side::Remote;
            let resp = egui::Modal::new(egui::Id::new("files-delete")).show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.heading(i18n::files_delete_title(remote, items.len()));
                ui.add_space(6.0);
                for e in items.iter().take(8) {
                    ui.label(RichText::new(format!("• {}", e.name)).monospace());
                }
                if items.len() > 8 {
                    ui.weak(i18n::files_delete_more(items.len() - 8));
                }
                ui.add_space(6.0);
                ui.label(i18n::FILES_DELETE_PERMANENT());
                if needs_ack {
                    ui.add_space(4.0);
                    ui.colored_label(err_color(ui), i18n::FILES_DELETE_RECURSIVE());
                    ui.checkbox(ack, i18n::FILES_DELETE_ACK());
                }
                ui.add_space(8.0);
                decision = button_row(ui, i18n::FILES_DELETE_CONFIRM(), !needs_ack || *ack);
            });
            match decision {
                Some(true) if !needs_ack || *ack => {
                    return DialogResult::Delete(*side, std::mem::take(items));
                }
                Some(false) => return DialogResult::Close,
                None if resp.should_close() => return DialogResult::Close,
                _ => {}
            }
        }
    }
    DialogResult::Keep
}

pub fn conflict_dialog(
    ctx: &egui::Context,
    c: &PendingConflict,
    pending: usize,
    all: &mut bool,
) -> Option<ConflictChoice> {
    let mut choice = None;
    egui::Modal::new(egui::Id::new("files-conflict")).show(ctx, |ui| {
        ui.set_max_width(460.0);
        ui.heading(i18n::CONFLICT_TITLE());
        ui.add_space(4.0);
        ui.label(RichText::new(&c.path).monospace());
        let p = i18n::conflict_pending(pending);
        if !p.is_empty() {
            ui.weak(p);
        }
        ui.add_space(6.0);
        egui::Grid::new("conflict-grid")
            .num_columns(3)
            .spacing([14.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new(i18n::CONFLICT_EXISTING()).strong());
                ui.label(RichText::new(i18n::CONFLICT_INCOMING()).strong());
                ui.end_row();
                ui.label(i18n::CONFLICT_SIZE());
                ui.label(meta_size(Some(&c.existing)));
                ui.label(meta_size(c.incoming.as_ref()));
                ui.end_row();
                ui.label(i18n::CONFLICT_DATE());
                ui.label(meta_date(Some(&c.existing)));
                ui.label(meta_date(c.incoming.as_ref()));
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.checkbox(all, i18n::CONFLICT_ALL());
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n::CONFLICT_OVERWRITE()).clicked() {
                choice = Some(ConflictChoice::Overwrite);
            }
            if ui.button(i18n::CONFLICT_SKIP()).clicked() {
                choice = Some(ConflictChoice::Skip);
            }
            if ui.button(i18n::CONFLICT_RENAME()).clicked() {
                choice = Some(ConflictChoice::Rename);
            }
            if ui.button(i18n::CONFLICT_CANCEL()).clicked() {
                choice = Some(ConflictChoice::Cancel);
            }
        });
    });
    choice
}

fn meta_size(m: Option<&RemoteOrLocalMeta>) -> String {
    m.map_or_else(|| i18n::CONFLICT_UNKNOWN().to_string(), |m| fmt_size(m.size))
}

fn meta_date(m: Option<&RemoteOrLocalMeta>) -> String {
    m.map_or_else(
        || i18n::CONFLICT_UNKNOWN().to_string(),
        |m| fmt_mtime(m.mtime),
    )
}
