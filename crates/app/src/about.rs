//! Janela "Sobre": cartão escuro com a identidade cyberpunk do ícone (gradiente roxo,
//! borda neon ciano→magenta, grade em perspectiva), informações de build e créditos.
//!
//! Decisão de tema: o cartão mantém sempre a identidade escura (também no tema claro),
//! como uma "placa" da marca; só o fundo esmaecido do modal segue o tema do app.
//! Animação: fade-in de ~200 ms com leve pulso do brilho; depois é estático (sem repaint).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::epaint::{Mesh, WHITE_UV};
use egui::{
    Align2, Color32, FontId, Id, Margin, Pos2, Rect, RichText, Shape, Stroke, Vec2, WidgetInfo,
    WidgetType, pos2, vec2,
};

use crate::dialogs::Next;
use crate::i18n;

const CARD_W: f32 = 456.0;
const PAD_X: i8 = 30;
const RADIUS: f32 = 20.0;

const CYAN: Color32 = Color32::from_rgb(0, 240, 255);
const MAGENTA: Color32 = Color32::from_rgb(255, 43, 214);
/// Rótulos e textos secundários (contraste >= 6:1 sobre o fundo do cartão).
const TEXT_DIM: Color32 = Color32::from_rgb(168, 154, 204);
/// Valores (contraste >= 11:1).
const TEXT_VALUE: Color32 = Color32::from_rgb(226, 220, 244);

const TOP_BG: (u8, u8, u8) = (40, 15, 74);
const BOTTOM_BG: (u8, u8, u8) = (6, 3, 14);

/// Dependências diretas de `crates/app/Cargo.toml` (e as principais dos demais crates),
/// com licença; derivada de `docs/DEPENDENCIAS.md`.
pub const THIRD_PARTY: &[(&str, &str)] = &[
    ("alacritty_terminal", "Apache-2.0"),
    ("chrono", "MIT OR Apache-2.0"),
    ("directories", "MIT OR Apache-2.0"),
    ("eframe", "MIT OR Apache-2.0"),
    ("egui", "MIT OR Apache-2.0"),
    ("globset", "Unlicense OR MIT"),
    ("keyring", "MIT OR Apache-2.0"),
    ("png", "MIT OR Apache-2.0"),
    ("rfd", "MIT"),
    ("rusqlite", "MIT"),
    ("russh", "Apache-2.0"),
    ("russh-sftp", "MIT OR Apache-2.0"),
    ("secrecy", "Apache-2.0 OR MIT"),
    ("serde", "MIT OR Apache-2.0"),
    ("serde_json", "MIT OR Apache-2.0"),
    ("sha2", "MIT OR Apache-2.0"),
    ("thiserror", "MIT OR Apache-2.0"),
    ("tokio", "MIT"),
    ("tokio-util", "MIT"),
    ("unicode-normalization", "MIT OR Apache-2.0"),
    ("uuid", "Apache-2.0 OR MIT"),
    ("zeroize", "Apache-2.0 OR MIT"),
];

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Estado da janela (vive em `Dialog::About`).
#[derive(Debug, Clone)]
pub struct AboutState {
    token: u64,
    started: bool,
    copied: bool,
}

impl AboutState {
    pub fn new() -> Self {
        Self {
            token: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            started: false,
            copied: false,
        }
    }
}

// ------------------------------------------------------------------ informações

/// Informações de build embutidas em tempo de compilação.
pub struct BuildInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub dirty: bool,
    pub date: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
}

pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        commit: env!("MYSSH_GIT_COMMIT"),
        dirty: env!("MYSSH_GIT_DIRTY") == "1",
        date: env!("MYSSH_BUILD_DATE"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

/// Hash curto, `desconhecido` sem git, e `+modificado` se a árvore estava suja.
pub fn commit_label(commit: &str, dirty: bool) -> String {
    let commit = commit.trim();
    if commit.is_empty() {
        return i18n::ABOUT_UNKNOWN.to_string();
    }
    if dirty {
        format!("{commit} {}", i18n::ABOUT_DIRTY)
    } else {
        commit.to_string()
    }
}

/// Troca o prefixo da pasta pessoal por `~` (não expõe o nome de usuário).
pub fn abbreviate_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        return if rest.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", rest.display())
        };
    }
    path.display().to_string()
}

/// Texto copiado para relatar bugs.
pub fn report_text(b: &BuildInfo, data_dir: &str) -> String {
    format!(
        "mySSH {}\nCommit: {}\nBuild: {}\nPlataforma: {} ({})\nDados: {}",
        b.version,
        commit_label(b.commit, b.dirty),
        b.date,
        b.os,
        b.arch,
        data_dir
    )
}

fn data_dir_shown(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir
        .map(Path::to_path_buf)
        .or_else(|| crate::paths::data_dir().map(|(d, _)| d))
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

/// Abre a pasta no gerenciador de arquivos do SO (ação explícita do usuário).
fn open_folder(dir: &Path) {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";
    if !dir.is_dir() {
        return;
    }
    if let Ok(mut child) = std::process::Command::new(program).arg(dir).spawn() {
        // Recolhe o processo filho sem bloquear a UI.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

// ------------------------------------------------------------------ desenho

fn rgba(c: (u8, u8, u8), a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0, c.1, c.2, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Contorno de retângulo arredondado (sentido horário), com `seg` passos por canto.
fn round_rect_path(rect: Rect, r: f32, seg: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(4 * (seg + 1));
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=seg {
            let a = (start + 90.0 * i as f32 / seg as f32).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// Extensão horizontal do retângulo arredondado na altura `y`.
fn span_at(rect: Rect, r: f32, y: f32) -> (f32, f32) {
    let dy = if y < rect.top() + r {
        rect.top() + r - y
    } else if y > rect.bottom() - r {
        y - (rect.bottom() - r)
    } else {
        0.0
    };
    let dx = r - (r * r - dy.min(r).powi(2)).max(0.0).sqrt();
    (rect.left() + dx, rect.right() - dx)
}

/// Preenche o cartão em faixas horizontais; `color_at(y)` dá a cor de cada altura.
fn gradient_card(rect: Rect, r: f32, color_at: impl Fn(f32) -> Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let bands = ((rect.height() / 2.0).ceil() as usize).max(8);
    for i in 0..bands {
        let y0 = rect.top() + rect.height() * i as f32 / bands as f32;
        let y1 = rect.top() + rect.height() * (i + 1) as f32 / bands as f32;
        // Em faixas que cruzam um canto, usa a extensão mais larga do trecho (y mais interno).
        let (xl0, xr0) = span_at(rect, r, y0);
        let (xl1, xr1) = span_at(rect, r, y1);
        let base = mesh.vertices.len() as u32;
        for (p, c) in [
            (pos2(xl0, y0), color_at(y0)),
            (pos2(xr0, y0), color_at(y0)),
            (pos2(xr1, y1), color_at(y1)),
            (pos2(xl1, y1), color_at(y1)),
        ] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p,
                uv: WHITE_UV,
                color: c,
            });
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    mesh
}

/// Halo radial: `center` no meio, transparente na borda.
fn radial_glow(center: Pos2, radius: f32, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let n = 48;
    mesh.colored_vertex(center, color);
    for i in 0..n {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        mesh.colored_vertex(
            center + vec2(a.cos(), a.sin()) * radius,
            Color32::TRANSPARENT,
        );
    }
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    mesh
}

/// Contorno com gradiente diagonal ciano→magenta.
fn gradient_outline(rect: Rect, r: f32, width: f32, alpha: f32) -> Vec<Shape> {
    let pts = round_rect_path(rect, r, 8);
    let n = pts.len();
    let diag = rect.width() + rect.height();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let mid = a.lerp(b, 0.5) - rect.min;
            let t = ((mid.x + mid.y) / diag).clamp(0.0, 1.0);
            let c = lerp_color(CYAN, MAGENTA, t).gamma_multiply(alpha);
            Shape::line_segment([a, b], Stroke::new(width, c))
        })
        .collect()
}

/// Grade em perspectiva magenta (eco do ícone), muito sutil, restrita ao cartão.
fn perspective_grid(rect: Rect, r: f32, horizon: f32, alpha: f32) -> Vec<Shape> {
    let mut shapes = Vec::new();
    let h = rect.bottom() - horizon;
    let cx = rect.center().x;
    let line = |a: Pos2, b: Pos2, al: f32, shapes: &mut Vec<Shape>| {
        let inside = |p: Pos2| {
            let (l, rr) = span_at(rect, r, p.y.min(rect.bottom() - 0.5));
            p.x > l + 1.0 && p.x < rr - 1.0 && p.y < rect.bottom() - 0.5
        };
        if inside(a) && inside(b) {
            shapes.push(Shape::line_segment(
                [a, b],
                Stroke::new(1.0, MAGENTA.gamma_multiply(al * alpha)),
            ));
        }
    };
    // Horizontais, adensando perto do horizonte.
    for k in 1..=6 {
        let f = (k as f32 / 6.0).powi(2);
        let y = horizon + h * f;
        let (l, rr) = (rect.left(), rect.right());
        line(pos2(l, y), pos2(rr, y), 0.10 + 0.50 * f, &mut shapes);
    }
    // Verticais convergindo ao ponto de fuga, em trechos com opacidade crescente.
    for i in -9..=9 {
        let top = pos2(cx + i as f32 * 14.0, horizon);
        let bot = pos2(cx + i as f32 * 62.0, rect.bottom());
        let steps = 6;
        for s in 0..steps {
            let (t0, t1) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
            line(
                top.lerp(bot, t0),
                top.lerp(bot, t1),
                0.08 + 0.50 * t1,
                &mut shapes,
            );
        }
    }
    shapes
}

fn icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = Id::new("about-icon-texture");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let icon = crate::icon::app_icon();
    let img = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    let tex = ctx.load_texture("about-icon", img, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Estilo escuro do cartão (independente do tema do app).
fn card_style(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    *v = egui::Visuals::dark();
    v.override_text_color = None;
    v.hyperlink_color = CYAN;
    v.selection.bg_fill = Color32::from_rgb(90, 50, 170);
    v.selection.stroke = Stroke::new(2.0, CYAN);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    let rounding = egui::CornerRadius::same(8);
    for (w, fill, stroke, fg) in [
        (
            &mut v.widgets.inactive,
            Color32::from_rgb(28, 16, 52),
            Color32::from_rgb(96, 66, 150),
            TEXT_VALUE,
        ),
        (
            &mut v.widgets.hovered,
            Color32::from_rgb(52, 28, 92),
            CYAN,
            Color32::WHITE,
        ),
        (
            &mut v.widgets.active,
            Color32::from_rgb(78, 36, 120),
            MAGENTA,
            Color32::WHITE,
        ),
    ] {
        w.weak_bg_fill = fill;
        w.bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.fg_stroke = Stroke::new(1.0, fg);
        w.corner_radius = rounding;
    }
    ui.spacing_mut().button_padding = vec2(12.0, 6.0);
    ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui(vec2(96.0, 0.0), |ui| {
            ui.set_min_width(96.0);
            ui.label(RichText::new(label).monospace().size(11.5).color(TEXT_DIM));
        });
        ui.add(
            egui::Label::new(
                RichText::new(value)
                    .monospace()
                    .size(11.5)
                    .color(TEXT_VALUE),
            )
            .selectable(true)
            .wrap(),
        );
    });
}

fn link(ui: &mut egui::Ui, text: &str, url: &str) {
    let r = ui
        .add(egui::Link::new(RichText::new(text).size(12.5)))
        .on_hover_text(format!("{} — {url}", i18n::ABOUT_LINK_HELP));
    if r.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

/// Desenha a janela. `Next::Close` ao fechar (Esc, clique fora ou botão).
pub(crate) fn show(ctx: &egui::Context, st: &mut AboutState, data_dir: Option<&Path>) -> Next {
    // Fade-in: a primeira passagem registra 0.0; depois anima até 1.0 (200 ms).
    let anim_id = Id::new(("about-fade", st.token));
    let t = if st.started {
        ctx.animate_value_with_time(anim_id, 1.0, 0.2)
    } else {
        st.started = true;
        ctx.animate_value_with_time(anim_id, 0.0, 0.2)
    };
    // Pulso discreto do brilho durante a abertura; em repouso fica fixo em 0.75.
    let glow_k = 0.75 + 0.55 * (std::f32::consts::PI * t).sin();

    let info = build_info();
    let shown_dir = data_dir_shown(data_dir);
    let dir_text = shown_dir
        .as_deref()
        .map(|d| abbreviate_home(d, home_dir().as_deref()))
        .unwrap_or_else(|| i18n::ABOUT_UNKNOWN.to_string());
    let tex = icon_texture(ctx);

    let modal = egui::Modal::new(Id::new("about-dialog"))
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            // Reserva a camada de fundo; é preenchida depois que a altura é conhecida.
            let bg_slot = ui.painter().add(Shape::Noop);

            let inner = egui::Frame::NONE
                .inner_margin(Margin::symmetric(PAD_X, 26))
                .show(ui, |ui| {
                    card_style(ui);
                    ui.set_width(CARD_W - 2.0 * f32::from(PAD_X));
                    card_content(ui, ctx, &tex, &info, &dir_text, shown_dir.as_deref(), st)
                });
            let action = inner.inner;
            let card = inner.response.rect;

            if card.width() < 10.0 || card.height() < 10.0 {
                return action; // passagem de medição do modal
            }
            // --- fundo, halo, grade e bordas ---
            let painter = ui.painter().clone();
            let horizon = card.bottom() - 96.0;
            let bg = gradient_card(card, RADIUS, |y| {
                let k = ((y - card.top()) / card.height()).clamp(0.0, 1.0);
                let base = lerp_color(
                    Color32::from_rgb(TOP_BG.0, TOP_BG.1, TOP_BG.2),
                    Color32::from_rgb(BOTTOM_BG.0, BOTTOM_BG.1, BOTTOM_BG.2),
                    k.powf(0.8),
                );
                let ramp = ((y - horizon) / (card.bottom() - horizon)).clamp(0.0, 1.0);
                lerp_color(base, Color32::from_rgb(92, 14, 84), ramp * 0.55)
            });
            let mut shapes = vec![Shape::mesh(bg)];
            let halo_c = pos2(card.center().x, card.top() + 26.0 + 60.0);
            shapes.push(Shape::mesh(radial_glow(
                halo_c,
                150.0,
                rgba((130, 70, 255), 0.30 * glow_k),
            )));
            shapes.extend(perspective_grid(card, RADIUS, horizon, 0.42));
            // brilho externo: camadas de contorno com alfa decrescente
            for i in (1..=7).rev() {
                let e = i as f32 * 1.1;
                let a = 0.11 * glow_k * (1.0 - i as f32 / 8.5);
                shapes.extend(gradient_outline(card.expand(e), RADIUS + e, 1.6, a));
            }
            shapes.extend(gradient_outline(card, RADIUS, 1.25, 0.95));
            painter.set(bg_slot, Shape::Vec(shapes));
            action
        });

    if modal.should_close() {
        Next::Close
    } else {
        modal.inner
    }
}

fn card_content(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    tex: &egui::TextureHandle,
    info: &BuildInfo,
    dir_text: &str,
    dir: Option<&Path>,
    st: &mut AboutState,
) -> Next {
    let mut next = Next::Keep;

    // Ícone
    ui.vertical_centered(|ui| {
        let size = 112.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, i18n::ABOUT_TITLE));
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    });

    // Título com aberração cromática
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 52.0), egui::Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, "mySSH"));
    let font = FontId::proportional(42.0);
    let c = rect.center();
    let p = ui.painter();
    p.text(
        c + vec2(-1.2, 0.0),
        Align2::CENTER_CENTER,
        "mySSH",
        font.clone(),
        CYAN.gamma_multiply(0.85),
    );
    p.text(
        c + vec2(1.2, 0.0),
        Align2::CENTER_CENTER,
        "mySSH",
        font.clone(),
        MAGENTA.gamma_multiply(0.85),
    );
    p.text(c, Align2::CENTER_CENTER, "mySSH", font, Color32::WHITE);

    ui.vertical_centered(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(i18n::ABOUT_TAGLINE)
                    .size(13.0)
                    .color(TEXT_DIM),
            )
            .halign(egui::Align::Center),
        );
    });

    ui.add_space(16.0);
    divider(ui);
    ui.add_space(12.0);

    // Informações
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        info_row(ui, i18n::ABOUT_VERSION, info.version);
        info_row(
            ui,
            i18n::ABOUT_COMMIT,
            &commit_label(info.commit, info.dirty),
        );
        info_row(ui, i18n::ABOUT_BUILD, info.date);
        info_row(
            ui,
            i18n::ABOUT_PLATFORM,
            &format!("{} ({})", info.os, info.arch),
        );
        info_row(ui, i18n::ABOUT_DATA, dir_text);
    });

    ui.add_space(12.0);
    divider(ui);
    ui.add_space(10.0);

    // Créditos
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            RichText::new(i18n::ABOUT_MADE_WITH)
                .size(12.5)
                .color(TEXT_DIM),
        );
        link(ui, "egui", "https://github.com/emilk/egui");
        ui.label(RichText::new("·").color(TEXT_DIM));
        link(
            ui,
            "alacritty_terminal",
            "https://github.com/alacritty/alacritty",
        );
        ui.label(RichText::new("·").color(TEXT_DIM));
        link(ui, "russh", "https://github.com/Eugeny/russh");
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(i18n::ABOUT_LICENSE_LABEL)
                .size(12.5)
                .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(i18n::ABOUT_LICENSE)
                .monospace()
                .size(12.0)
                .color(TEXT_VALUE),
        );
    });

    ui.add_space(6.0);
    egui::CollapsingHeader::new(
        RichText::new(i18n::about_third_party(THIRD_PARTY.len()))
            .size(12.5)
            .color(TEXT_DIM),
    )
    .id_salt("about-third-party")
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .max_height(130.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("about-deps")
                    .num_columns(2)
                    .spacing([16.0, 2.0])
                    .show(ui, |ui| {
                        for (name, lic) in THIRD_PARTY {
                            ui.label(
                                RichText::new(*name)
                                    .monospace()
                                    .size(11.0)
                                    .color(TEXT_VALUE),
                            );
                            ui.label(RichText::new(*lic).monospace().size(11.0).color(TEXT_DIM));
                            ui.end_row();
                        }
                    });
            });
    });

    ui.add_space(14.0);

    // Botões
    ui.horizontal(|ui| {
        let copy_label = if st.copied {
            i18n::ABOUT_COPIED
        } else {
            i18n::ABOUT_COPY
        };
        if ui
            .button(copy_label)
            .on_hover_text(i18n::ABOUT_COPY_HELP)
            .clicked()
        {
            ctx.copy_text(report_text(info, dir_text));
            st.copied = true;
        }
        if ui
            .add_enabled(dir.is_some(), egui::Button::new(i18n::ABOUT_OPEN_DATA))
            .on_hover_text(i18n::ABOUT_OPEN_DATA_HELP)
            .clicked()
            && let Some(d) = dir
        {
            open_folder(d);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(i18n::CLOSE).clicked() {
                next = Next::Close;
            }
        });
    });
    next
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    let n = 24;
    let mut mesh = Mesh::default();
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = rect.left() + rect.width() * f;
        // some nas pontas; ciano → magenta no meio
        let a = (std::f32::consts::PI * f).sin().max(0.0).powf(0.7) * 0.5;
        let c = lerp_color(CYAN, MAGENTA, f).gamma_multiply(a);
        mesh.colored_vertex(pos2(x, rect.top()), c);
        mesh.colored_vertex(pos2(x, rect.bottom()), c);
    }
    for i in 0..n {
        let b = 2 * i as u32;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 3, b + 2);
    }
    ui.painter().add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_desconhecido_e_modificado() {
        assert_eq!(commit_label("", false), "desconhecido");
        assert_eq!(commit_label("  ", true), "desconhecido");
        assert_eq!(commit_label("abc1234", false), "abc1234");
        assert_eq!(commit_label("abc1234", true), "abc1234 +modificado");
    }

    #[test]
    fn caminho_com_home_abreviado() {
        let home = Path::new("/Users/ana");
        assert_eq!(
            abbreviate_home(Path::new("/Users/ana/Library/mySSH"), Some(home)),
            "~/Library/mySSH"
        );
        assert_eq!(abbreviate_home(home, Some(home)), "~");
        assert_eq!(
            abbreviate_home(Path::new("/var/lib/myssh"), Some(home)),
            "/var/lib/myssh"
        );
        assert_eq!(abbreviate_home(Path::new("/x"), None), "/x");
        // prefixo textual parecido não é a home
        assert_eq!(
            abbreviate_home(Path::new("/Users/anabela/x"), Some(home)),
            "/Users/anabela/x"
        );
    }

    #[test]
    fn texto_do_relatorio() {
        let b = BuildInfo {
            version: "1.2.3",
            commit: "",
            dirty: false,
            date: "2026-10-14",
            os: "macos",
            arch: "aarch64",
        };
        let t = report_text(&b, "~/dados");
        assert_eq!(
            t,
            "mySSH 1.2.3\nCommit: desconhecido\nBuild: 2026-10-14\nPlataforma: macos (aarch64)\nDados: ~/dados"
        );
    }

    #[test]
    fn dependencias_diretas_do_app_estao_listadas() {
        let toml = include_str!("../Cargo.toml");
        let mut in_deps = false;
        for line in toml.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_deps = l == "[dependencies]";
                continue;
            }
            if !in_deps || l.is_empty() || l.starts_with('#') {
                continue;
            }
            let Some((name, _)) = l.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.starts_with("myssh-") {
                continue;
            }
            assert!(
                THIRD_PARTY.iter().any(|(n, _)| *n == name),
                "dependência `{name}` ausente em about::THIRD_PARTY"
            );
        }
    }

    #[test]
    fn lista_de_terceiros_ordenada_e_sem_duplicatas() {
        let names: Vec<_> = THIRD_PARTY.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    // ---------------------------------------------------------- UI headless

    use crate::app::Dialog;
    use crate::ui_tests::{harness, memory_app};
    use egui_kittest::kittest::Queryable;

    fn is_about(h: &egui_kittest::Harness<'static, crate::app::App>) -> bool {
        matches!(h.state().dialog, Dialog::About(_))
    }

    #[test]
    fn abre_e_fecha_com_esc() {
        let mut h = harness(memory_app());
        h.get_by_label(i18n::ABOUT_BUTTON).click();
        h.run_steps(3);
        assert!(is_about(&h));
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(!is_about(&h));
    }

    #[test]
    fn abre_e_fecha_com_botao() {
        let mut h = harness(memory_app());
        h.state_mut().open_about();
        h.run_steps(3);
        assert!(is_about(&h));
        h.get_by_label(i18n::CLOSE).click();
        h.run_steps(3);
        assert!(!is_about(&h));
    }

    #[test]
    fn copiar_informacoes_coloca_texto_no_clipboard() {
        let mut h = harness(memory_app());
        h.state_mut().open_about();
        h.run_steps(3);
        h.get_by_label(i18n::ABOUT_COPY).click();
        let mut copied = None;
        for _ in 0..4 {
            h.step();
            for c in &h.output().platform_output.commands {
                if let egui::OutputCommand::CopyText(t) = c {
                    copied = Some(t.clone());
                }
            }
        }
        let t = copied.expect("texto copiado");
        let head = format!("mySSH {}\n", env!("CARGO_PKG_VERSION"));
        assert!(t.starts_with(&head), "{t}");
        assert!(t.contains("Commit: "));
        assert!(t.contains(&format!("Plataforma: {}", std::env::consts::OS)));
        h.run_steps(2);
        assert!(h.query_by_label(i18n::ABOUT_COPIED).is_some());
    }

    #[test]
    #[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
    fn about_screenshots() {
        let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&dir).unwrap();
        for (name, theme) in [
            ("about-escuro", crate::settings::ThemePref::Dark),
            ("about-claro", crate::settings::ThemePref::Light),
        ] {
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 780.0))
                .with_pixels_per_point(2.0)
                .wgpu()
                .build_eframe(|_| memory_app());
            h.state_mut().settings.theme = theme;
            h.run_steps(3);
            h.state_mut().open_about();
            h.run_steps(40);
            std::thread::sleep(std::time::Duration::from_millis(300));
            h.run_steps(10);
            let img = h.render().expect("render");
            img.save(dir.join(format!("{name}.png"))).unwrap();
            if theme == crate::settings::ThemePref::Dark {
                h.get_by_label_contains("Componentes de terceiros").click();
                h.run_steps(10);
                let img = h.render().expect("render");
                img.save(dir.join("about-escuro-terceiros.png")).unwrap();
            }
        }
    }
}
