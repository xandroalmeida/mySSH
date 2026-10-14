//! Tema e estilo do terminal.

use egui::{Color32, FontId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermTheme {
    pub fg: Color32,
    pub bg: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    /// Cores ANSI 0..=15 (8 normais + 8 brilhantes).
    pub ansi: [Color32; 16],
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl TermTheme {
    pub fn dark() -> Self {
        Self {
            fg: c(0xc5, 0xc8, 0xc6),
            bg: c(0x1d, 0x1f, 0x21),
            cursor: c(0xc5, 0xc8, 0xc6),
            selection: c(0x44, 0x55, 0x77),
            ansi: [
                c(0x1d, 0x1f, 0x21),
                c(0xcc, 0x66, 0x66),
                c(0xb5, 0xbd, 0x68),
                c(0xf0, 0xc6, 0x74),
                c(0x81, 0xa2, 0xbe),
                c(0xb2, 0x94, 0xbb),
                c(0x8a, 0xbe, 0xb7),
                c(0xc5, 0xc8, 0xc6),
                c(0x66, 0x69, 0x6b),
                c(0xd5, 0x4e, 0x53),
                c(0xb9, 0xca, 0x4a),
                c(0xe7, 0xc5, 0x47),
                c(0x7a, 0xa6, 0xda),
                c(0xc3, 0x97, 0xd8),
                c(0x70, 0xc0, 0xb1),
                c(0xea, 0xea, 0xea),
            ],
        }
    }

    pub fn light() -> Self {
        Self {
            fg: c(0x24, 0x29, 0x2f),
            bg: c(0xff, 0xff, 0xff),
            cursor: c(0x24, 0x29, 0x2f),
            selection: c(0xb6, 0xd6, 0xfb),
            ansi: [
                c(0x24, 0x29, 0x2f),
                c(0xcf, 0x22, 0x2e),
                c(0x11, 0x63, 0x29),
                c(0x4d, 0x2d, 0x00),
                c(0x09, 0x69, 0xda),
                c(0x82, 0x50, 0xdf),
                c(0x1b, 0x7c, 0x83),
                c(0x6e, 0x77, 0x81),
                c(0x57, 0x60, 0x6a),
                c(0xa4, 0x0e, 0x26),
                c(0x1a, 0x7f, 0x37),
                c(0x63, 0x3c, 0x01),
                c(0x21, 0x8b, 0xff),
                c(0xa4, 0x75, 0xf9),
                c(0x31, 0x92, 0xaa),
                c(0x8c, 0x95, 0x9f),
            ],
        }
    }
}

impl Default for TermTheme {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Debug, Clone)]
pub struct TermStyle {
    pub font: FontId,
    pub theme: TermTheme,
}

impl TermStyle {
    pub fn new(font: FontId, theme: TermTheme) -> Self {
        Self { font, theme }
    }
}

impl Default for TermStyle {
    fn default() -> Self {
        Self::new(FontId::monospace(14.0), TermTheme::dark())
    }
}
