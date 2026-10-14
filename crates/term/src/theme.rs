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

/// Paleta embutida, selecionável por id estável (gravado em `ThemeOverride::palette`).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub id: &'static str,
    /// Nome para exibir (pt-BR).
    pub name: &'static str,
    /// Cor de destaque sugerida para a paleta (RGB).
    pub accent: [u8; 3],
    build: fn() -> TermTheme,
}

impl Palette {
    pub fn theme(&self) -> TermTheme {
        (self.build)()
    }
}

/// Paletas embutidas, na ordem de exibição.
pub const PALETTES: &[Palette] = &[
    Palette {
        id: "default-dark",
        name: "Escuro padrão",
        accent: [0x81, 0xa2, 0xbe],
        build: TermTheme::dark,
    },
    Palette {
        id: "default-light",
        name: "Claro padrão",
        accent: [0x09, 0x69, 0xda],
        build: TermTheme::light,
    },
    Palette {
        id: "solarized-dark",
        name: "Solarized Dark",
        accent: [0x26, 0x8b, 0xd2],
        build: TermTheme::solarized_dark,
    },
    Palette {
        id: "solarized-light",
        name: "Solarized Light",
        accent: [0xb5, 0x89, 0x00],
        build: TermTheme::solarized_light,
    },
    Palette {
        id: "dracula",
        name: "Dracula",
        accent: [0xbd, 0x93, 0xf9],
        build: TermTheme::dracula,
    },
    Palette {
        id: "cyberpunk",
        name: "Cyberpunk Neon",
        accent: [0xff, 0x2b, 0xd6],
        build: TermTheme::cyberpunk,
    },
];

/// Procura uma paleta embutida pelo id.
pub fn palette_by_id(id: &str) -> Option<&'static Palette> {
    PALETTES.iter().find(|p| p.id == id)
}

impl TermTheme {
    /// Tema de uma paleta embutida (`None` se o id for desconhecido).
    pub fn from_palette_id(id: &str) -> Option<Self> {
        palette_by_id(id).map(Palette::theme)
    }

    /// O fundo é escuro (luminância relativa baixa)?
    pub fn is_dark(&self) -> bool {
        let [r, g, b, _] = self.bg.to_array();
        (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) < 128.0
    }

    pub fn solarized_dark() -> Self {
        Self {
            fg: c(0x83, 0x94, 0x96),
            bg: c(0x00, 0x2b, 0x36),
            cursor: c(0x93, 0xa1, 0xa1),
            selection: c(0x07, 0x42, 0x4f),
            ansi: [
                c(0x07, 0x36, 0x42),
                c(0xdc, 0x32, 0x2f),
                c(0x85, 0x99, 0x00),
                c(0xb5, 0x89, 0x00),
                c(0x26, 0x8b, 0xd2),
                c(0xd3, 0x36, 0x82),
                c(0x2a, 0xa1, 0x98),
                c(0xee, 0xe8, 0xd5),
                c(0x00, 0x2b, 0x36),
                c(0xcb, 0x4b, 0x16),
                c(0x58, 0x6e, 0x75),
                c(0x65, 0x7b, 0x83),
                c(0x83, 0x94, 0x96),
                c(0x6c, 0x71, 0xc4),
                c(0x93, 0xa1, 0xa1),
                c(0xfd, 0xf6, 0xe3),
            ],
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            fg: c(0x65, 0x7b, 0x83),
            bg: c(0xfd, 0xf6, 0xe3),
            cursor: c(0x58, 0x6e, 0x75),
            selection: c(0xee, 0xe8, 0xd5),
            ansi: [
                c(0x07, 0x36, 0x42),
                c(0xdc, 0x32, 0x2f),
                c(0x85, 0x99, 0x00),
                c(0xb5, 0x89, 0x00),
                c(0x26, 0x8b, 0xd2),
                c(0xd3, 0x36, 0x82),
                c(0x2a, 0xa1, 0x98),
                c(0xee, 0xe8, 0xd5),
                c(0x00, 0x2b, 0x36),
                c(0xcb, 0x4b, 0x16),
                c(0x58, 0x6e, 0x75),
                c(0x65, 0x7b, 0x83),
                c(0x83, 0x94, 0x96),
                c(0x6c, 0x71, 0xc4),
                c(0x93, 0xa1, 0xa1),
                c(0xfd, 0xf6, 0xe3),
            ],
        }
    }

    pub fn dracula() -> Self {
        Self {
            fg: c(0xf8, 0xf8, 0xf2),
            bg: c(0x28, 0x2a, 0x36),
            cursor: c(0xf8, 0xf8, 0xf2),
            selection: c(0x44, 0x47, 0x5a),
            ansi: [
                c(0x21, 0x22, 0x2c),
                c(0xff, 0x55, 0x55),
                c(0x50, 0xfa, 0x7b),
                c(0xf1, 0xfa, 0x8c),
                c(0xbd, 0x93, 0xf9),
                c(0xff, 0x79, 0xc6),
                c(0x8b, 0xe9, 0xfd),
                c(0xf8, 0xf8, 0xf2),
                c(0x62, 0x72, 0xa4),
                c(0xff, 0x6e, 0x6e),
                c(0x69, 0xff, 0x94),
                c(0xff, 0xff, 0xa5),
                c(0xd6, 0xac, 0xff),
                c(0xff, 0x92, 0xdf),
                c(0xa4, 0xff, 0xff),
                c(0xff, 0xff, 0xff),
            ],
        }
    }

    /// Roxo-escuro com magenta e ciano (combina com o ícone do app).
    pub fn cyberpunk() -> Self {
        Self {
            fg: c(0xe6, 0xdc, 0xff),
            bg: c(0x12, 0x06, 0x26),
            cursor: c(0xff, 0x2b, 0xd6),
            selection: c(0x4a, 0x1f, 0x8c),
            ansi: [
                c(0x1a, 0x0c, 0x33),
                c(0xff, 0x3b, 0x6b),
                c(0x39, 0xff, 0x9c),
                c(0xff, 0xe1, 0x4d),
                c(0x7a, 0x3c, 0xff),
                c(0xff, 0x2b, 0xd6),
                c(0x00, 0xf0, 0xff),
                c(0xd9, 0xcf, 0xf5),
                c(0x5a, 0x48, 0x80),
                c(0xff, 0x6e, 0x92),
                c(0x7a, 0xff, 0xbd),
                c(0xff, 0xee, 0x8a),
                c(0xa2, 0x7a, 0xff),
                c(0xff, 0x6a, 0xe6),
                c(0x7a, 0xf8, 0xff),
                c(0xf4, 0xfd, 0xff),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unicos_e_resolvem() {
        for (i, p) in PALETTES.iter().enumerate() {
            assert!(PALETTES[..i].iter().all(|q| q.id != p.id));
            assert_eq!(TermTheme::from_palette_id(p.id), Some(p.theme()));
        }
        assert!(TermTheme::from_palette_id("inexistente").is_none());
    }

    #[test]
    fn claro_e_escuro() {
        assert!(TermTheme::dark().is_dark());
        assert!(TermTheme::cyberpunk().is_dark());
        assert!(!TermTheme::light().is_dark());
        assert!(!TermTheme::solarized_light().is_dark());
    }
}
