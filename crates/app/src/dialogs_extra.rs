//! Diálogos do M2-B: importação do `~/.ssh/config` e escolha de perfil ao dividir.

use egui::{Key, RichText};
use myssh_sshconfig::Warning;

use crate::app::{App, ImportDialog, ImportStage, SplitDialog};
use crate::dialogs::{Next, error_color};
use crate::i18n;
use crate::split::Dir;
use crate::ssh_import::is_local_command_warning;
use crate::tabs::TabKind;
use crate::view::warn_color;

fn warning_ui(ui: &mut egui::Ui, w: &Warning) {
    let text = i18n::import_warning_at(&w.file, w.line, &w.msg);
    if is_local_command_warning(w) {
        // destaque: a diretiva executaria um comando local e foi ignorada
        ui.label(
            RichText::new(format!("⛔ {text}"))
                .strong()
                .color(error_color(ui)),
        );
    } else {
        ui.label(
            RichText::new(format!("⚠ {text}"))
                .small()
                .color(warn_color(ui)),
        );
    }
}

impl App {
    pub(crate) fn import_dialog(&self, ctx: &egui::Context, d: &mut ImportDialog) -> Next {
        let mut next = Next::Keep;
        let max_h = (ctx.content_rect().height() - 280.0).max(200.0);
        let resp = egui::Modal::new(egui::Id::new("import-dialog")).show(ctx, |ui| {
            ui.set_width(680.0);
            ui.heading(i18n::IMPORT_TITLE);
            ui.add_space(6.0);
            match d.stage {
                ImportStage::Intro => {
                    ui.label(i18n::IMPORT_INTRO);
                    if let Some(e) = &d.error {
                        ui.add_space(4.0);
                        ui.colored_label(error_color(ui), e);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(i18n::IMPORT_USE_DEFAULT).clicked() {
                            next = Next::ImportLoad(None);
                        }
                        if ui.button(i18n::IMPORT_PICK_FILE).clicked() {
                            next = Next::ImportPick;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(i18n::CANCEL).clicked() {
                                next = Next::Close;
                            }
                        });
                    });
                }
                ImportStage::Loading | ImportStage::Saving => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if d.stage == ImportStage::Loading {
                            i18n::IMPORT_LOADING
                        } else {
                            i18n::IMPORT_SAVING
                        });
                    });
                    if d.stage == ImportStage::Loading {
                        ui.add_space(8.0);
                        if ui.button(i18n::CANCEL).clicked() {
                            next = Next::Close;
                        }
                    }
                }
                ImportStage::Preview => self.import_preview_ui(ui, d, max_h, &mut next),
            }
        });
        if resp.is_top_modal
            && !resp.any_popup_open
            && d.stage != ImportStage::Saving
            && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            next = Next::Close;
        }
        next
    }

    fn import_preview_ui(
        &self,
        ui: &mut egui::Ui,
        d: &mut ImportDialog,
        max_h: f32,
        next: &mut Next,
    ) {
        let total = d.preview.items.len();
        ui.label(i18n::import_summary(total, d.preview.conflict_count()));
        ui.weak(i18n::IMPORT_TARGET);
        if !d.preview.global_warnings.is_empty() {
            ui.add_space(4.0);
            egui::CollapsingHeader::new(
                RichText::new(format!(
                    "{} ({})",
                    i18n::IMPORT_GLOBAL_WARNINGS,
                    d.preview.global_warnings.len()
                ))
                .strong(),
            )
            .id_salt("import-global-warnings")
            .default_open(true)
            .show(ui, |ui| {
                for w in &d.preview.global_warnings {
                    warning_ui(ui, w);
                }
            });
        }
        ui.add_space(4.0);
        if total == 0 {
            ui.label(i18n::IMPORT_NONE);
        } else {
            ui.horizontal(|ui| {
                if ui.small_button(i18n::IMPORT_SELECT_ALL).clicked() {
                    d.preview.select_all(true);
                }
                if ui.small_button(i18n::IMPORT_SELECT_NEW).clicked() {
                    d.preview.select_all(false);
                }
                if ui.small_button(i18n::IMPORT_SELECT_NONE).clicked() {
                    d.preview.select_none();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("import-list")
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, item) in d.preview.items.iter_mut().enumerate() {
                        ui.push_id(("import-item", i), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.add_enabled(
                                    !item.invalid,
                                    egui::Checkbox::new(
                                        &mut item.selected,
                                        RichText::new(&item.cand.alias).strong(),
                                    ),
                                );
                                let p = &item.cand.profile;
                                ui.label(
                                    RichText::new(format!("{}@{}:{}", p.username, p.host, p.port))
                                        .monospace()
                                        .weak(),
                                );
                                if let Some(j) = &item.cand.jump_alias {
                                    ui.weak(format!("{}: {j}", i18n::IMPORT_JUMP));
                                }
                                if item.conflict.is_some() {
                                    ui.label(
                                        RichText::new(format!(" {} ", i18n::IMPORT_EXISTS))
                                            .small()
                                            .strong()
                                            .color(egui::Color32::BLACK)
                                            .background_color(egui::Color32::from_rgb(
                                                230, 180, 20,
                                            )),
                                    )
                                    .on_hover_text(i18n::IMPORT_EXISTS_HELP);
                                }
                                if item.invalid {
                                    ui.colored_label(error_color(ui), i18n::IMPORT_INVALID);
                                }
                            });
                            ui.indent(("import-warn", i), |ui| {
                                for w in &item.cand.warnings {
                                    warning_ui(ui, w);
                                }
                            });
                        });
                    }
                });
            ui.separator();
        }
        if let Some(e) = &d.error {
            ui.colored_label(error_color(ui), e);
        }
        let n = d.preview.selected_count();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(n > 0, egui::Button::new(i18n::import_button(n)))
                    .clicked()
                {
                    *next = Next::ImportConfirm;
                }
                if ui.button(i18n::CANCEL).clicked() {
                    *next = Next::Close;
                }
            });
        });
    }

    pub(crate) fn split_dialog(&self, ctx: &egui::Context, d: &mut SplitDialog) -> Next {
        let mut next = Next::Keep;
        let base = self.focused_kind().and_then(|k| match k {
            TabKind::Terminal { profile, .. } => Some(profile.name.clone()),
            TabKind::Welcome => None,
        });
        let title = match d.dir {
            Dir::Vertical => i18n::SPLIT_PICK_TITLE_SIDE,
            Dir::Horizontal => i18n::SPLIT_PICK_TITLE_STACK,
        };
        let resp = egui::Modal::new(egui::Id::new("split-dialog")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label(i18n::SPLIT_PICK_HELP);
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 300.0).max(160.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let same = format!(
                        "{} ({})",
                        i18n::SPLIT_SAME,
                        base.clone().unwrap_or_default()
                    );
                    ui.selectable_value(&mut d.choice, None, same);
                    for p in &self.profiles {
                        let r = ui.selectable_value(
                            &mut d.choice,
                            Some(p.id),
                            format!("{}  ({}@{})", p.name, p.username, p.host),
                        );
                        if r.double_clicked() {
                            next = Next::SplitConfirm(d.dir, Some(p.id));
                        }
                    }
                });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(i18n::SPLIT_OPEN).clicked() {
                    next = Next::SplitConfirm(d.dir, d.choice);
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
}
