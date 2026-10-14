//! Versão vazia do modo demo, usada em builds de release sem a feature `demo`.
//! Não lê nenhuma variável de ambiente de demonstração nem tem caminho de aceitação
//! automática de chave de host.

use crate::app::App;

pub static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Demo;

impl Demo {
    pub fn from_env(_data_dir_overridden: bool) -> Option<Self> {
        None
    }

    pub fn connects(&self) -> bool {
        true
    }

    pub fn tick(&mut self, _app: &mut App, _ctx: &egui::Context) {}
}
