//! Renderiza `crates/app/assets/icon*.svg` em PNGs, `icon.ico` e `icon.icns`.
//! Uso: `cargo run -p myssh-icon-tool` (a partir da raiz do workspace).
//! O `.icns` usa `iconutil` (somente macOS); se ausente, é pulado.

use std::{fs, path::Path, process::Command};

use resvg::{tiny_skia, usvg};

const SIZES: [u32; 8] = [16, 32, 48, 64, 128, 256, 512, 1024];
const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn render(tree: &usvg::Tree, size: u32) -> Vec<u8> {
    let mut pm = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let s = size as f32 / tree.size().width();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(s, s),
        &mut pm.as_mut(),
    );
    pm.encode_png().expect("png")
}

fn load(path: &Path) -> usvg::Tree {
    let data = fs::read(path).expect("ler svg");
    usvg::Tree::from_data(&data, &usvg::Options::default()).expect("svg")
}

fn main() {
    let dir = Path::new("crates/app/assets");
    let big = load(&dir.join("icon.svg"));
    let small = load(&dir.join("icon-small.svg"));
    let pick = |s: u32| if s <= 32 { &small } else { &big };

    let total = SIZES.len();
    for (i, s) in SIZES.iter().enumerate() {
        let png = render(pick(*s), *s);
        fs::write(dir.join(format!("icon-{s}.png")), png).expect("escrever png");
        println!("[{}/{}] icon-{s}.png", i + 1, total);
    }

    // ICO com entradas PNG (suportado desde o Windows Vista).
    let imgs: Vec<(u32, Vec<u8>)> = ICO_SIZES.iter().map(|&s| (s, render(pick(s), s))).collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(imgs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * imgs.len() as u32;
    for (s, png) in &imgs {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &imgs {
        ico.extend_from_slice(png);
    }
    fs::write(dir.join("icon.ico"), ico).expect("ico");
    println!("icon.ico");

    // ICNS via iconutil.
    let set = std::env::temp_dir().join("myssh.iconset");
    let _ = fs::remove_dir_all(&set);
    fs::create_dir_all(&set).expect("iconset");
    for (name, s) in [
        ("icon_16x16", 16),
        ("icon_16x16@2x", 32),
        ("icon_32x32", 32),
        ("icon_32x32@2x", 64),
        ("icon_128x128", 128),
        ("icon_128x128@2x", 256),
        ("icon_256x256", 256),
        ("icon_256x256@2x", 512),
        ("icon_512x512", 512),
        ("icon_512x512@2x", 1024),
    ] {
        fs::write(set.join(format!("{name}.png")), render(pick(s), s)).expect("png");
    }
    match Command::new("iconutil")
        .args(["-c", "icns", "-o"])
        .arg(dir.join("icon.icns"))
        .arg(&set)
        .status()
    {
        Ok(st) if st.success() => println!("icon.icns"),
        _ => eprintln!("iconutil indisponível: icon.icns não gerado"),
    }
}
