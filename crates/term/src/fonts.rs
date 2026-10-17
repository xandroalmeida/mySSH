//! Fonte monoespaçada embutida do terminal: Cascadia Mono PL (SIL OFL 1.1, ver
//! `assets/fonts/CascadiaCode-LICENSE.txt`). Cobre box drawing, blocos, Braille, Symbols for
//! Legacy Computing e Powerline, que faltam nas fontes padrão do egui (Hack segue como fallback).

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily};

/// Nome interno da fonte nas definições do egui.
pub const MONO_FONT: &str = "cascadia-mono-pl";

const MONO_TTF: &[u8] = include_bytes!("../assets/fonts/CascadiaMonoPL-Regular.ttf");

/// Põe a fonte do terminal na frente da família monoespaçada do contexto.
pub fn install(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        MONO_FONT,
        FontData::from_static(MONO_TTF),
        vec![InsertFontFamily {
            family: FontFamily::Monospace,
            priority: FontPriority::Highest,
        }],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cobre_os_simbolos_de_tui() {
        let face = ttf_parser::Face::parse(MONO_TTF, 0).unwrap();
        let has = |c: u32| face.glyph_index(char::from_u32(c).unwrap()).is_some();
        // box drawing, blocos, geométricos e Braille (gráficos do btop) completos
        for c in (0x2500..=0x25FF).chain(0x2800..=0x28FF) {
            assert!(has(c), "sem glifo U+{c:04X}");
        }
        // sextantes (Legacy Computing) e separadores Powerline
        for c in [0x1FB00, 0x1FB3B, 0xE0A0, 0xE0B0, 0xE0B2] {
            assert!(has(c), "sem glifo U+{c:04X}");
        }
    }

    #[test]
    fn instala_na_frente_da_familia_monoespacada() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut out = ctx.run_ui(Default::default(), |_| {});
        out.textures_delta.clear();
        let first = ctx.fonts(|f| f.definitions().families[&FontFamily::Monospace][0].clone());
        assert_eq!(first, MONO_FONT);
    }
}
