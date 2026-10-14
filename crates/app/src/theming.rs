//! Tema por perfil: resolução da herança (`ThemeOverride` + global) e editor com prévia ao vivo.
//!
//! O tema do perfil vale só para a aba/painel do perfil (terminal + cor de destaque); o tema
//! global (claro/escuro/sistema) continua controlando o restante da aplicação.

use egui::{Color32, FontId, RichText};
use myssh_core::ThemeOverride;
use myssh_term::{PALETTES, TermTheme, palette_by_id};

use crate::i18n;
use crate::settings::{FONT_MAX, FONT_MIN};

/// Tema efetivo de um painel.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub term: TermTheme,
    pub font_size: f32,
    /// Cor de destaque da aba/painel (`None` = sem destaque próprio).
    pub accent: Option<Color32>,
    /// A paleta veio do perfil (e não do tema global).
    pub palette_overridden: bool,
}

/// Paleta padrão para o tema global claro/escuro.
pub fn global_term_theme(dark: bool) -> TermTheme {
    if dark {
        TermTheme::dark()
    } else {
        TermTheme::light()
    }
}

/// Resolve o tema efetivo: cada campo `None` herda o global. Paleta desconhecida também herda.
pub fn resolve(ov: &ThemeOverride, global_dark: bool, global_font: f32) -> Resolved {
    let from_profile = ov.palette.as_deref().and_then(TermTheme::from_palette_id);
    let palette_overridden = from_profile.is_some();
    Resolved {
        term: from_profile.unwrap_or_else(|| global_term_theme(global_dark)),
        font_size: ov.font_size_tenths.map_or(global_font, |t| {
            (f32::from(t) / 10.0).clamp(FONT_MIN, FONT_MAX)
        }),
        accent: ov.accent.map(|[r, g, b]| Color32::from_rgb(r, g, b)),
        palette_overridden,
    }
}

/// Nenhum campo sobrescrito?
pub fn is_inherited(ov: &ThemeOverride) -> bool {
    ov.palette.is_none() && ov.font_size_tenths.is_none() && ov.accent.is_none()
}

/// Nome de exibição da paleta (ou `None` se desconhecida).
pub fn palette_name(id: &str) -> Option<&'static str> {
    palette_by_id(id).map(|p| p.name)
}

/// Cor de texto legível sobre `bg`.
pub fn contrast_text(bg: Color32) -> Color32 {
    let [r, g, b, _] = bg.to_array();
    if 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b) > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Editor do tema de um perfil, com prévia ao vivo logo abaixo.
pub fn editor(ui: &mut egui::Ui, ov: &mut ThemeOverride, global_font: f32) {
    let global_dark = ui.visuals().dark_mode;
    egui::Grid::new("theme-grid")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label(i18n::TH_PALETTE);
            let cur = ov
                .palette
                .as_deref()
                .and_then(palette_name)
                .unwrap_or(i18n::TH_INHERIT);
            egui::ComboBox::from_id_salt("theme-palette")
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut ov.palette, None, i18n::TH_INHERIT);
                    for p in PALETTES {
                        ui.selectable_value(&mut ov.palette, Some(p.id.to_string()), p.name);
                    }
                });
            ui.end_row();

            ui.label(i18n::TH_FONT);
            ui.horizontal(|ui| {
                let mut own = ov.font_size_tenths.is_some();
                if ui.checkbox(&mut own, i18n::TH_FONT_OWN).changed() {
                    ov.font_size_tenths = own.then(|| (global_font * 10.0).round() as u16);
                }
                if let Some(t) = &mut ov.font_size_tenths {
                    let mut pt = f32::from(*t) / 10.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut pt, FONT_MIN..=FONT_MAX)
                                .suffix(" pt")
                                .step_by(0.5),
                        )
                        .changed()
                    {
                        *t = (pt * 10.0).round() as u16;
                    }
                } else {
                    ui.weak(i18n::th_font_inherited(global_font));
                }
            });
            ui.end_row();

            ui.label(i18n::TH_ACCENT);
            ui.horizontal(|ui| {
                let mut own = ov.accent.is_some();
                if ui.checkbox(&mut own, i18n::TH_ACCENT_OWN).changed() {
                    ov.accent = own.then(|| {
                        ov.palette
                            .as_deref()
                            .and_then(palette_by_id)
                            .map_or([0x7a, 0x3c, 0xff], |p| p.accent)
                    });
                }
                if let Some(rgb) = &mut ov.accent {
                    ui.color_edit_button_srgb(rgb);
                } else {
                    ui.weak(i18n::TH_ACCENT_NONE);
                }
            });
            ui.end_row();
        });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!is_inherited(ov), egui::Button::new(i18n::TH_RESET).small())
            .clicked()
        {
            *ov = ThemeOverride::default();
        }
        ui.weak(i18n::TH_SCOPE_HELP);
    });
    ui.add_space(4.0);
    preview(ui, &resolve(ov, global_dark, global_font));
}

/// Amostra de terminal com texto ANSI colorido e as 16 cores da paleta.
pub fn preview(ui: &mut egui::Ui, r: &Resolved) {
    let t = &r.term;
    let font = FontId::monospace(r.font_size.min(16.0));
    let stroke = match r.accent {
        Some(a) => egui::Stroke::new(2.0, a),
        None => egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    };
    egui::Frame::new()
        .fill(t.bg)
        .stroke(stroke)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.push_id("theme-preview", |ui| {
                let line = |ui: &mut egui::Ui, parts: &[(&str, Color32)]| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (s, c) in parts {
                            ui.label(RichText::new(*s).font(font.clone()).color(*c));
                        }
                    });
                };
                line(
                    ui,
                    &[
                        ("ana@srv", t.ansi[10]),
                        (":", t.fg),
                        ("~/projeto", t.ansi[12]),
                        ("$ ", t.fg),
                        ("ls --color", t.fg),
                    ],
                );
                line(
                    ui,
                    &[
                        ("src/  ", t.ansi[12]),
                        ("run.sh  ", t.ansi[10]),
                        ("dados.tar.gz  ", t.ansi[9]),
                        ("link -> /srv", t.ansi[14]),
                    ],
                );
                line(
                    ui,
                    &[
                        ("aviso: ", t.ansi[11]),
                        ("disco em 91%  ", t.fg),
                        ("erro: ", t.ansi[9]),
                        ("falha ao conectar", t.fg),
                    ],
                );
                line(
                    ui,
                    &[("ana@srv", t.ansi[10]), (":~$ ", t.fg), ("█", t.cursor)],
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for c in t.ansi {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 1.0, c);
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn herda_tudo_do_global() {
        let r = resolve(&ThemeOverride::default(), true, 14.0);
        assert_eq!(r.term, TermTheme::dark());
        assert_eq!(r.font_size, 14.0);
        assert_eq!(r.accent, None);
        assert!(!r.palette_overridden);
        assert_eq!(
            resolve(&ThemeOverride::default(), false, 12.0).term,
            TermTheme::light()
        );
    }

    #[test]
    fn perfil_sobrescreve_so_o_que_define() {
        let ov = ThemeOverride {
            palette: Some("cyberpunk".into()),
            font_size_tenths: Some(165),
            accent: Some([1, 2, 3]),
            ..Default::default()
        };
        let r = resolve(&ov, false, 14.0);
        assert_eq!(r.term, TermTheme::cyberpunk());
        assert_eq!(r.font_size, 16.5);
        assert_eq!(r.accent, Some(Color32::from_rgb(1, 2, 3)));
        assert!(r.palette_overridden);

        let so_fonte = ThemeOverride {
            font_size_tenths: Some(200),
            ..Default::default()
        };
        let r = resolve(&so_fonte, true, 14.0);
        assert_eq!(r.term, TermTheme::dark());
        assert_eq!(r.font_size, 20.0);
    }

    #[test]
    fn paleta_desconhecida_herda_e_fonte_e_limitada() {
        let ov = ThemeOverride {
            palette: Some("não-existe".into()),
            font_size_tenths: Some(9000),
            ..Default::default()
        };
        let r = resolve(&ov, true, 14.0);
        assert_eq!(r.term, TermTheme::dark());
        assert!(!r.palette_overridden);
        assert_eq!(r.font_size, FONT_MAX);
    }

    #[test]
    fn heranca_detectada() {
        assert!(is_inherited(&ThemeOverride::default()));
        assert!(!is_inherited(&ThemeOverride {
            accent: Some([0, 0, 0]),
            ..Default::default()
        }));
    }

    #[test]
    fn contraste() {
        assert_eq!(contrast_text(Color32::WHITE), Color32::BLACK);
        assert_eq!(
            contrast_text(Color32::from_rgb(0x12, 0x06, 0x26)),
            Color32::WHITE
        );
    }
}
