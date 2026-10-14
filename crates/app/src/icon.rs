//! Ícone da aplicação (janela/Dock/barra de tarefas).

/// Decodifica o PNG 256x256 embutido.
pub fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
        .expect("icon-256.png embutido é válido")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icone_decodifica_256_rgba() {
        let i = app_icon();
        assert_eq!((i.width, i.height), (256, 256));
        assert_eq!(i.rgba.len(), 256 * 256 * 4);
        assert!(i.rgba.chunks(4).any(|p| p[3] > 0));
    }
}
