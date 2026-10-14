//! Integração com o app: confirmação ao fechar com fila ativa e atalho do painel.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

use super::backend::MemHandle;
use super::contract::*;
use super::{FilesPanel, test_panel};
use crate::app::Dialog;
use crate::split::Dir;
use crate::split_body::SplitBody;
use crate::tabs::{Tab, TabBody, TabEnv, TabKind};
use crate::ui_tests::{harness, memory_app};

/// Corpo de aba que só tem o painel de arquivos (a aba de terminal real exige servidor SSH).
struct FilesBody {
    panel: FilesPanel,
    toggles: Arc<AtomicUsize>,
    _rt: Arc<tokio::runtime::Runtime>,
}

impl TabBody for FilesBody {
    fn title(&self) -> String {
        "servidor".into()
    }
    fn ui(&mut self, ui: &mut egui::Ui, _env: &TabEnv) {
        self.panel.ui(ui);
    }
    fn pump(&mut self, _env: &TabEnv) {
        self.panel.pump();
    }
    fn needs_modal(&self) -> bool {
        self.panel.needs_modal()
    }
    fn modal_ui(&mut self, ctx: &egui::Context, _env: &TabEnv) {
        self.panel.modal_ui(ctx);
    }
    fn active_transfers(&self) -> usize {
        self.panel.active()
    }
    fn toggle_files(&mut self) {
        self.toggles.fetch_add(1, Ordering::SeqCst);
    }
    fn is_busy(&self) -> bool {
        self.panel.active() > 0
    }
    fn close(&mut self) {
        self.panel.cancel_all();
    }
}

fn body(ctx: &egui::Context, toggles: &Arc<AtomicUsize>) -> (Box<FilesBody>, MemHandle) {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );
    let (panel, mem) = test_panel(rt.handle().clone(), ctx.clone());
    (
        Box::new(FilesBody {
            panel,
            toggles: toggles.clone(),
            _rt: rt,
        }),
        mem,
    )
}

/// Simula duas transferências em andamento no motor.
fn enqueue_two(mem: &MemHandle) {
    let mut s = mem.state.lock().unwrap();
    s.active.insert(TransferId(1));
    s.active.insert(TransferId(2));
}

#[test]
fn fechar_aba_com_fila_ativa_pede_confirmacao_com_o_numero() {
    let mut h = harness(memory_app());
    let toggles = Arc::new(AtomicUsize::new(0));
    let (b, mem) = body(&h.ctx, &toggles);
    enqueue_two(&mem);
    let base = h.state().tabs.len();
    h.state_mut().tabs.push(Tab {
        id: 700,
        kind: TabKind::Welcome,
        body: b,
    });
    h.state_mut().active = Some(700);
    h.run_steps(3);

    assert!(h.state().any_busy());
    assert_eq!(h.state().active_transfers(), 2);
    h.state_mut().request_close_tab(700);
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::ConfirmCloseTab(700)));
    h.get_by_label_contains("Há 2 transferências em andamento");

    // cancelar o diálogo mantém tudo como está
    h.get_by_label("Cancelar").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::None));
    assert_eq!(h.state().tabs.len(), base + 1);
    assert_eq!(mem.state.lock().unwrap().cancel_all_calls, 0);

    // confirmar fecha a aba e cancela a fila de forma determinística
    h.state_mut().request_close_tab(700);
    h.run_steps(3);
    h.get_by_label("Fechar aba").click();
    h.run_steps(3);
    assert_eq!(h.state().tabs.len(), base);
    assert_eq!(mem.state.lock().unwrap().cancel_all_calls, 1);
    assert!(mem.state.lock().unwrap().active.is_empty());
}

#[test]
fn sair_do_app_menciona_as_transferencias() {
    let mut h = harness(memory_app());
    let toggles = Arc::new(AtomicUsize::new(0));
    let (b, mem) = body(&h.ctx, &toggles);
    enqueue_two(&mem);
    h.state_mut().tabs.push(Tab {
        id: 701,
        kind: TabKind::Welcome,
        body: b,
    });
    h.state_mut().active = Some(701);
    h.state_mut().dialog = Dialog::ConfirmQuit;
    h.run_steps(3);
    h.get_by_label_contains("Há 2 transferências em andamento");
    // sem fila ativa o aviso some
    mem.state.lock().unwrap().active.clear();
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("transferências em andamento")
            .is_none()
    );
}

#[test]
fn fechar_painel_dividido_com_fila_ativa_pede_confirmacao() {
    let mut h = harness(memory_app());
    let toggles = Arc::new(AtomicUsize::new(0));
    let (b, mem) = body(&h.ctx, &toggles);
    enqueue_two(&mem);
    let (b2, _) = body(&h.ctx, &toggles);
    let mut sp = SplitBody::new(TabKind::Welcome, b);
    let second = sp.split(Dir::Vertical, TabKind::Welcome, b2).unwrap();
    assert_eq!(second, 2);
    // o painel 1 é o que tem a fila; o foco está no 2
    h.state_mut().tabs.push(Tab {
        id: 702,
        kind: TabKind::Welcome,
        body: Box::new(sp),
    });
    h.state_mut().active = Some(702);
    h.run_steps(3);
    h.state_mut().request_close_pane(702, 1);
    h.run_steps(3);
    assert!(matches!(
        h.state().dialog,
        Dialog::ConfirmClosePane { tab: 702, pane: 1 }
    ));
    h.get_by_label_contains("Há 2 transferências em andamento");
    h.get_by_label("Cancelar").click();
    h.run_steps(2);
    // fechar o painel sem fila não pergunta
    h.state_mut().request_close_pane(702, 2);
    h.run_steps(2);
    assert!(matches!(h.state().dialog, Dialog::None));
}

#[test]
fn atalho_cmd_shift_f_alterna_o_painel_da_aba_ativa() {
    let mut h = harness(memory_app());
    let toggles = Arc::new(AtomicUsize::new(0));
    let (b, _mem) = body(&h.ctx, &toggles);
    h.state_mut().tabs.push(Tab {
        id: 703,
        kind: TabKind::Welcome,
        body: b,
    });
    h.state_mut().active = Some(703);
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    h.run_steps(2);
    assert_eq!(toggles.load(Ordering::SeqCst), 1);
    // Cmd+F puro continua sendo a busca de perfis
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run_steps(2);
    assert_eq!(toggles.load(Ordering::SeqCst), 1);
}
