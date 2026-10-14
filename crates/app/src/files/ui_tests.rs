//! Testes headless (egui_kittest) do painel de arquivos com `MemBackend`, e renders offscreen.
//!
//! Renders: `MYSSH_DEMO_SHOTS=dir cargo test -p myssh-app m3b_screenshots -- --ignored`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::backend::{MemBackend, MemHandle, mem_entry};
use super::contract::*;
use super::model::Side;
use super::queue::State;
use super::{EVENT_BUDGET, FilesPanel};

struct Fx {
    h: Harness<'static, FilesPanel>,
    mem: MemHandle,
    tmp: tempfile::TempDir,
    _rt: Arc<tokio::runtime::Runtime>,
}

fn secs(n: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(n)
}

fn build(policy: Overwrite, wgpu: bool) -> Fx {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    std::fs::write(d.join("a.txt"), b"12345").unwrap();
    std::fs::write(d.join("b.log"), vec![0u8; 2048]).unwrap();
    std::fs::create_dir(d.join("sub")).unwrap();
    std::fs::write(d.join(".oculto"), b"x").unwrap();

    let ctx = egui::Context::default();
    let notify: myssh_ssh::Notify = {
        let c = ctx.clone();
        Arc::new(move || c.request_repaint())
    };
    let (be, mem) = MemBackend::new(notify);
    mem.add(mem_entry("/home/ana/docs", Kind::Dir, 0, 1_700_000_000));
    mem.add(mem_entry(
        "/home/ana/notas.txt",
        Kind::File,
        1200,
        1_700_100_000,
    ));
    mem.add(mem_entry(
        "/home/ana/backup.tar",
        Kind::File,
        5 * 1024 * 1024,
        1_690_000_000,
    ));
    mem.add(mem_entry("/home/ana/.config", Kind::Dir, 0, 1_600_000_000));
    mem.add(mem_entry(
        "/home/ana/docs/leia-me.md",
        Kind::File,
        300,
        1_700_200_000,
    ));

    let panel = FilesPanel::new(
        rt.handle().clone(),
        ctx,
        Some(Box::new(be)),
        d.to_path_buf(),
        policy,
    );
    let mut b = Harness::builder().with_size(egui::vec2(1100.0, if wgpu { 760.0 } else { 560.0 }));
    if wgpu {
        b = b.with_pixels_per_point(1.5).wgpu();
    }
    let mut h = b.build_ui_state(
        |ui, p: &mut FilesPanel| {
            p.pump();
            let ctx = ui.ctx().clone();
            p.ui(ui);
            p.modal_ui(&ctx);
        },
        panel,
    );
    let mut fx = Fx {
        h: {
            h.run_steps(2);
            h
        },
        mem,
        tmp,
        _rt: rt,
    };
    fx.wait("listagens iniciais", |p| {
        !p.local.entries.is_empty()
            && p.remote_pane.path == "/home/ana"
            && !p.remote_pane.entries.is_empty()
    });
    fx
}

fn fixture() -> Fx {
    build(Overwrite::Ask, false)
}

impl Fx {
    fn wait(&mut self, what: &str, cond: impl Fn(&FilesPanel) -> bool) {
        let start = Instant::now();
        while !cond(self.h.state()) {
            assert!(start.elapsed() < Duration::from_secs(10), "timeout: {what}");
            self.h.run_steps(2);
            std::thread::sleep(Duration::from_millis(5));
        }
        self.h.run_steps(2);
    }

    fn p(&self) -> &FilesPanel {
        self.h.state()
    }

    fn names(&self, side: Side) -> Vec<String> {
        let p = self.p().pane(side);
        p.visible()
            .iter()
            .map(|i| p.entries[*i].name.clone())
            .collect()
    }

    fn click_row(&mut self, name_and_kind: &str) {
        self.h.get_by_label_contains(name_and_kind).click();
        self.h.run_steps(2);
    }

    fn push(&mut self, ev: TransferEvent) {
        self.mem.push(ev);
        self.h.run_steps(3);
    }
}

fn tid(n: u64) -> TransferId {
    TransferId(n)
}

fn progress(
    id: u64,
    cur: &str,
    done: u64,
    total: u64,
    fd: u64,
    ft: u64,
    rate: u64,
) -> TransferEvent {
    TransferEvent::Progress {
        id: tid(id),
        current: cur.into(),
        bytes_done: done,
        bytes_total: total,
        files_done: fd,
        files_total: ft,
        rate_bps: rate,
    }
}

#[derive(Debug)]
struct Dropped(PathBuf);
impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------------- testes

#[test]
fn lista_as_duas_colunas_e_alterna_ocultos() {
    let mut fx = fixture();
    // pastas primeiro; ocultos escondidos
    assert_eq!(fx.names(Side::Local), ["sub", "a.txt", "b.log"]);
    assert_eq!(fx.names(Side::Remote), ["docs", "backup.tar", "notas.txt"]);
    fx.h.get_all_by_label("Ocultos").next().unwrap().click();
    fx.h.run_steps(2);
    assert!(fx.names(Side::Local).contains(&".oculto".to_string()));
    assert!(!fx.names(Side::Remote).contains(&".config".to_string()));
}

#[test]
fn navega_entrando_subindo_e_voltando() {
    let mut fx = fixture();
    // O duplo clique do egui_kittest não gera `double_clicked()`; a ação que ele dispara
    // (`Act::Open`) é a mesma do Enter, coberta em `teclado_setas_enter_e_backspace`.
    fx.h.state_mut().handle_for_test(super::column::Act::Open(
        Side::Remote,
        "/home/ana/docs".into(),
    ));
    fx.wait("entrar em docs", |p| p.remote_pane.path == "/home/ana/docs");
    assert_eq!(fx.names(Side::Remote), ["leia-me.md"]);
    // Subir (coluna remota é a segunda)
    fx.h.get_all_by_label("Subir").nth(1).unwrap().click();
    fx.wait("subir", |p| p.remote_pane.path == "/home/ana");
    // Voltar (histórico) → docs de novo
    fx.h.get_all_by_label("Voltar").nth(1).unwrap().click();
    fx.wait("voltar", |p| p.remote_pane.path == "/home/ana/docs");
}

#[test]
fn caminho_digitado_e_erro_de_listagem() {
    let mut fx = fixture();
    fx.mem
        .fail_list("/proibido", TransferError::PermissionDenied);
    fx.h.state_mut()
        .navigate(Side::Remote, "/proibido".into(), true);
    fx.wait("erro", |p| p.remote_pane.error.is_some());
    // a pasta exibida não muda quando a listagem falha
    assert_eq!(fx.p().remote_pane.path, "/home/ana");
    assert_eq!(
        fx.p().remote_pane.error.as_deref(),
        Some("Permissão negada.")
    );
    fx.h.get_by_label("Tentar novamente").click();
    fx.wait("recarrega", |p| p.remote_pane.error.is_none());
}

#[test]
fn clique_ordena_por_coluna() {
    let mut fx = fixture();
    fx.h.get_all_by_label_contains("Tamanho")
        .nth(1)
        .unwrap()
        .click();
    fx.h.run_steps(2);
    assert_eq!(fx.names(Side::Remote), ["docs", "notas.txt", "backup.tar"]);
    fx.h.get_all_by_label_contains("Tamanho")
        .nth(1)
        .unwrap()
        .click();
    fx.h.run_steps(2);
    assert_eq!(fx.names(Side::Remote), ["docs", "backup.tar", "notas.txt"]);
}

#[test]
fn selecao_multipla_com_cmd_e_shift() {
    let mut fx = fixture();
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label_contains("b.log, Arquivo")
        .click_modifiers(Modifiers::COMMAND);
    fx.h.run_steps(2);
    let sel: Vec<_> = fx.p().local.sel.names.iter().cloned().collect();
    assert_eq!(sel, ["a.txt", "b.log"]);
    fx.h.get_by_label_contains("sub, Pasta")
        .click_modifiers(Modifiers::SHIFT);
    fx.h.run_steps(2);
    assert_eq!(fx.p().local.sel.names.len(), 3);
}

#[test]
fn teclado_setas_enter_e_backspace() {
    let mut fx = fixture();
    fx.click_row("sub, Pasta");
    fx.h.key_press(Key::ArrowDown);
    fx.h.run_steps(2);
    assert_eq!(fx.p().local.sel.cursor, Some(1));
    assert!(fx.p().local.sel.contains("a.txt"));
    fx.h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    fx.h.run_steps(2);
    assert_eq!(fx.p().local.sel.names.len(), 2);
    // Enter na pasta entra nela
    fx.click_row("sub, Pasta");
    fx.h.key_press(Key::Enter);
    let sub = fx.tmp.path().join("sub").to_string_lossy().into_owned();
    fx.wait("entrar com Enter", |p| p.local.path == sub);
    fx.h.key_press(Key::Backspace);
    let root = fx.tmp.path().to_string_lossy().into_owned();
    fx.wait("subir com Backspace", |p| p.local.path == root);
}

#[test]
fn enviar_monta_pedido_e_mostra_na_fila() {
    let mut fx = fixture();
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label("⬆ Enviar").click();
    fx.h.run_steps(3);
    let reqs = fx.mem.requests();
    assert_eq!(reqs.len(), 1);
    let (id, r) = &reqs[0];
    assert_eq!(r.direction, Direction::Upload);
    assert_eq!(r.local, fx.tmp.path().join("a.txt"));
    assert_eq!(r.remote, "/home/ana");
    assert_eq!(r.overwrite, Overwrite::Ask);
    assert_eq!(fx.p().queue.get(*id).unwrap().state, State::Queued);
    assert_eq!(fx.p().active(), 1);
    fx.h.get_by_label_contains("Na fila");
}

#[test]
fn baixar_usa_pasta_local_aberta_e_a_politica_escolhida() {
    let mut fx = build(Overwrite::Skip, false);
    fx.click_row("notas.txt, Arquivo");
    fx.h.get_by_label("⬇ Baixar").click();
    fx.h.run_steps(3);
    let (_, r) = fx.mem.requests().remove(0);
    assert_eq!(r.direction, Direction::Download);
    assert_eq!(r.remote, "/home/ana/notas.txt");
    assert_eq!(r.local, fx.tmp.path());
    assert_eq!(r.overwrite, Overwrite::Skip);
}

#[test]
fn politica_por_transferencia_vale_para_os_proximos_envios() {
    let mut fx = fixture();
    fx.h.state_mut().overwrite = Overwrite::Rename;
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label("⬆ Enviar").click();
    fx.h.run_steps(3);
    assert_eq!(fx.mem.requests()[0].1.overwrite, Overwrite::Rename);
}

#[test]
fn enviar_sem_selecao_avisa_e_nao_enfileira() {
    let mut fx = fixture();
    fx.h.state_mut()
        .handle_for_test(super::column::Act::Transfer(Side::Local));
    fx.h.run_steps(2);
    assert!(fx.mem.requests().is_empty());
    assert!(
        fx.p()
            .notice
            .as_deref()
            .is_some_and(|n| n.contains("Selecione"))
    );
}

#[test]
fn progresso_conclusao_e_falhas_aparecem_na_fila() {
    let mut fx = fixture();
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label("⬆ Enviar").click();
    fx.h.run_steps(2);
    fx.push(TransferEvent::Scanning {
        id: tid(1),
        files: 3,
        bytes: 3000,
    });
    assert!(
        fx.h.query_all_by_label_contains("Contando")
            .next()
            .is_some()
    );
    fx.push(progress(1, "/x/a.bin", 1500, 3000, 1, 3, 500));
    assert!(
        fx.h.query_all_by_label_contains("Transferindo")
            .next()
            .is_some()
    );
    fx.h.get_by_label_contains("1,5 KB / 2,9 KB");
    fx.h.get_by_label_contains("arquivos 1/3");
    fx.h.get_by_label_contains("500 B/s");
    fx.h.get_by_label_contains("falta 3 s");
    fx.push(TransferEvent::FileError {
        id: tid(1),
        path: "/x/c.bin".into(),
        message: "Permissão negada".into(),
    });
    fx.push(TransferEvent::Finished {
        id: tid(1),
        ok: 2,
        failed: 1,
        skipped: 0,
    });
    assert_eq!(fx.p().queue.get(tid(1)).unwrap().state, State::Failed);
    fx.h.get_by_label_contains("Com falhas");
    // erros por arquivo ficam expansíveis (já abre quando há falha)
    fx.h.get_by_label_contains("/x/c.bin: Permissão negada");
    assert_eq!(fx.p().active(), 0);
}

#[test]
fn conflito_pergunta_e_responde() {
    let mut fx = fixture();
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label("⬆ Enviar").click();
    fx.h.run_steps(2);
    let mut rx = fx.mem.push_conflict(
        tid(1),
        "/home/ana/a.txt",
        RemoteOrLocalMeta {
            kind: Kind::File,
            size: 4096,
            mtime: Some(secs(1_700_000_000)),
        },
    );
    fx.h.run_steps(3);
    fx.h.get_by_label_contains("Já existe um item");
    fx.h.get_by_label("/home/ana/a.txt");
    fx.h.get_by_label("4,0 KB");
    fx.h.get_by_label("5 B");
    assert!(fx.p().needs_modal());
    fx.h.get_by_label("Pular").click();
    fx.h.run_steps(3);
    assert_eq!(rx.try_recv().unwrap(), ConflictDecision::Skip);
    assert!(!fx.p().needs_modal());
}

#[test]
fn conflito_aplicar_a_todos_e_cancelar() {
    let mut fx = fixture();
    let meta = || RemoteOrLocalMeta {
        kind: Kind::File,
        size: 1,
        mtime: None,
    };
    let mut rx1 = fx.mem.push_conflict(tid(1), "/a", meta());
    let mut rx2 = fx.mem.push_conflict(tid(1), "/b", meta());
    fx.h.run_steps(3);
    fx.h.get_by_label("Aplicar a todos os conflitos desta transferência")
        .click();
    fx.h.run_steps(2);
    fx.h.get_by_label("Sobrescrever").click();
    fx.h.run_steps(3);
    assert_eq!(rx1.try_recv().unwrap(), ConflictDecision::OverwriteAll);
    // o segundo ainda está na frente da fila (o motor responderia por conta própria ao "All")
    assert_eq!(fx.p().queue.conflicts.len(), 1);
    fx.h.get_by_label("Cancelar transferência").click();
    fx.h.run_steps(3);
    assert_eq!(rx2.try_recv().unwrap(), ConflictDecision::Cancel);
    assert!(!fx.p().needs_modal());
}

#[test]
fn cancelar_item_e_cancelar_todas() {
    let mut fx = fixture();
    for n in ["a.txt, Arquivo", "b.log, Arquivo"] {
        fx.click_row(n);
        fx.h.get_by_label("⬆ Enviar").click();
        fx.h.run_steps(2);
    }
    fx.push(progress(1, "a", 1, 10, 0, 1, 1));
    fx.push(progress(2, "b", 1, 10, 0, 1, 1));
    assert_eq!(fx.p().active(), 2);
    // cancelar o primeiro item
    fx.h.get_all_by_label("Cancelar").next().unwrap().click();
    fx.h.run_steps(3);
    assert_eq!(fx.mem.state.lock().unwrap().cancelled, vec![tid(1)]);
    assert_eq!(fx.p().queue.get(tid(1)).unwrap().state, State::Cancelled);
    assert_eq!(fx.p().active(), 1);
    fx.h.get_by_label_contains("Cancelada");
    fx.h.get_by_label("Cancelar todas").click();
    fx.h.run_steps(3);
    assert_eq!(fx.mem.state.lock().unwrap().cancel_all_calls, 1);
    assert_eq!(fx.p().active(), 0);
    // limpar concluídas esvazia a fila
    fx.h.get_by_label("Limpar concluídas").click();
    fx.h.run_steps(2);
    assert!(fx.p().queue.items.is_empty());
}

#[test]
fn exclusao_remota_de_pasta_exige_confirmar_entendimento() {
    let mut fx = fixture();
    fx.click_row("docs, Pasta");
    fx.h.key_press(Key::Delete);
    fx.h.run_steps(3);
    fx.h.get_by_label_contains("Excluir 1 item (remota)");
    fx.h.get_by_label_contains("recursivamente");
    // sem marcar, "Excluir" não faz nada
    fx.h.get_by_label("Excluir").click();
    fx.h.run_steps(3);
    assert!(fx.mem.state.lock().unwrap().removed.is_empty());
    assert!(fx.p().needs_modal());
    fx.h.get_by_label_contains("Entendo que isto exclui")
        .click();
    fx.h.run_steps(2);
    fx.h.get_by_label("Excluir").click();
    fx.wait("remoção", |p| !p.needs_modal());
    let removed = fx.mem.state.lock().unwrap().removed.clone();
    assert_eq!(removed, vec![("/home/ana/docs".to_string(), true)]);
    fx.wait("recarregou sem docs", |p| {
        !p.remote_pane.entries.iter().any(|e| e.name == "docs")
    });
}

#[test]
fn exclusao_de_arquivo_nao_pede_o_entendo_e_cancelar_nao_exclui() {
    let mut fx = fixture();
    fx.click_row("notas.txt, Arquivo");
    fx.h.key_press(Key::Delete);
    fx.h.run_steps(3);
    assert!(fx.h.query_by_label_contains("Entendo que").is_none());
    fx.h.get_by_label("Cancelar").click();
    fx.h.run_steps(3);
    assert!(!fx.p().needs_modal());
    assert!(fx.mem.state.lock().unwrap().removed.is_empty());
    // agora confirmando (o foco volta para a lista ao fechar o diálogo)
    fx.h.key_press(Key::Delete);
    fx.h.run_steps(3);
    fx.h.get_by_label("Excluir").click();
    fx.wait("remoção", |p| !p.needs_modal());
    assert_eq!(
        fx.mem.state.lock().unwrap().removed,
        vec![("/home/ana/notas.txt".to_string(), false)]
    );
}

#[test]
fn exclusao_local_recursiva_apaga_do_disco() {
    let mut fx = fixture();
    std::fs::write(fx.tmp.path().join("sub").join("dentro"), b"1").unwrap();
    fx.click_row("sub, Pasta");
    fx.h.key_press(Key::Delete);
    fx.h.run_steps(3);
    fx.h.get_by_label_contains("Excluir 1 item (local)");
    fx.h.get_by_label_contains("Entendo que isto exclui")
        .click();
    fx.h.run_steps(2);
    fx.h.get_by_label("Excluir").click();
    fx.wait("remoção local", |p| {
        !p.local.entries.iter().any(|e| e.name == "sub")
    });
    assert!(!fx.tmp.path().join("sub").exists());
}

#[test]
fn nova_pasta_e_renomear_validam_nome() {
    let mut fx = fixture();
    fx.h.state_mut()
        .handle_for_test(super::column::Act::NewDir(Side::Remote));
    fx.h.run_steps(3);
    fx.h.get_by_label("Nome do item").type_text("docs");
    fx.h.run_steps(2);
    fx.h.get_by_label("Criar").click();
    fx.h.run_steps(3);
    // já existe: o diálogo continua aberto com a mensagem
    fx.h.get_by_label("Já existe um item com esse nome.");
    assert!(
        fx.mem
            .state
            .lock()
            .unwrap()
            .dirs
            .contains_key("/home/ana/docs")
    );
    let before = fx.mem.names_in("/home/ana");
    assert_eq!(before.iter().filter(|n| *n == "docs").count(), 1);
    // nome inválido
    fx.h.get_by_label("Nome do item").focus();
    fx.h.run_steps(2);
    fx.h.get_by_label("Nome do item").type_text("/x");
    fx.h.run_steps(2);
    fx.h.get_by_label("Criar").click();
    fx.h.run_steps(3);
    assert!(fx.h.query_by_label_contains("Nome inválido").is_some());
    fx.h.get_by_label("Cancelar").click();
    fx.h.run_steps(2);
    assert!(!fx.p().needs_modal());
}

#[test]
fn nova_pasta_local_e_renomear_pelo_disco() {
    let mut fx = fixture();
    fx.h.state_mut()
        .handle_for_test(super::column::Act::NewDir(Side::Local));
    fx.h.run_steps(3);
    fx.h.get_by_label("Nome do item").type_text("nova pasta");
    fx.h.run_steps(2);
    fx.h.get_by_label("Criar").click();
    fx.wait("mkdir", |p| {
        p.local.entries.iter().any(|e| e.name == "nova pasta")
    });
    assert!(fx.tmp.path().join("nova pasta").is_dir());

    fx.click_row("a.txt, Arquivo");
    fx.h.key_press(Key::F2);
    fx.h.run_steps(3);
    assert!(fx.h.query_all_by_label("Renomear").count() >= 2);
    fx.h.get_by_label("Nome do item").type_text("2");
    fx.h.run_steps(2);
    fx.h.get_all_by_label("Renomear").last().unwrap().click();
    fx.wait("rename", |p| {
        p.local.entries.iter().any(|e| e.name == "a.txt2")
    });
    assert!(fx.tmp.path().join("a.txt2").exists() && !fx.tmp.path().join("a.txt").exists());
}

#[test]
fn soltar_arquivos_do_so_na_coluna_remota_envia() {
    let mut fx = fixture();
    let rect = fx.p().remote_rect.unwrap();
    fx.h.hover_at(rect.center());
    fx.h.run_steps(2);
    fx.h.input_mut()
        .dropped_files
        .push(Arc::new(Dropped(PathBuf::from("/tmp/solto.bin"))));
    fx.h.run_steps(2);
    let reqs = fx.mem.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].1.direction, Direction::Upload);
    assert_eq!(reqs[0].1.local, PathBuf::from("/tmp/solto.bin"));
    assert_eq!(reqs[0].1.remote, "/home/ana");
}

#[test]
fn soltar_fora_da_coluna_remota_e_ignorado() {
    let mut fx = fixture();
    let rect = fx.p().remote_rect.unwrap();
    fx.h.hover_at(egui::pos2(rect.left() - 300.0, rect.center().y));
    fx.h.run_steps(2);
    fx.h.input_mut()
        .dropped_files
        .push(Arc::new(Dropped(PathBuf::from("/tmp/solto.bin"))));
    fx.h.run_steps(2);
    assert!(fx.mem.requests().is_empty());
}

#[test]
fn arrastar_entre_colunas_transfere_a_selecao() {
    let mut fx = fixture();
    fx.click_row("a.txt, Arquivo");
    let from = fx.h.get_by_label_contains("a.txt, Arquivo").rect().center();
    let to = fx.p().remote_rect.unwrap().center();
    fx.h.hover_at(from);
    fx.h.drag_at(from);
    fx.h.run_steps(2);
    fx.h.hover_at(from + egui::vec2(40.0, 0.0));
    fx.h.run_steps(2);
    fx.h.hover_at(to);
    fx.h.run_steps(2);
    fx.h.drop_at(to);
    fx.h.run_steps(3);
    let reqs = fx.mem.requests();
    assert_eq!(reqs.len(), 1, "arrastar-soltar deve enfileirar um upload");
    assert_eq!(reqs[0].1.direction, Direction::Upload);
}

#[test]
fn eventos_sao_drenados_com_orcamento_por_frame() {
    let mut fx = fixture();
    for i in 0..(EVENT_BUDGET * 3) {
        fx.mem.push(progress(1, "x", i as u64, 10_000, 0, 1, 1));
    }
    let p = fx.h.state_mut();
    assert!(p.pump(), "ainda há eventos");
    assert_eq!(
        fx.mem.state.lock().unwrap().events.len(),
        EVENT_BUDGET * 2,
        "um pump consome no máximo o orçamento"
    );
}

#[test]
fn sem_backend_a_coluna_remota_avisa_e_a_local_funciona() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("x.txt"), b"1").unwrap();
    let panel = FilesPanel::new(
        rt.handle().clone(),
        egui::Context::default(),
        None,
        tmp.path().to_path_buf(),
        Overwrite::Ask,
    );
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 500.0))
        .build_ui_state(
            |ui, p: &mut FilesPanel| {
                p.pump();
                p.ui(ui);
            },
            panel,
        );
    let start = Instant::now();
    while h.state().local.entries.is_empty() {
        assert!(start.elapsed() < Duration::from_secs(10));
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(2);
    h.get_by_label_contains("indisponível");
    h.get_by_label_contains("x.txt, Arquivo");
    assert_eq!(h.state().active(), 0);
}

// ------------------------------------------------------------------- renders (manuais)

fn shot(fx: &mut Fx, dir: &Path, name: &str) {
    fx.h.run_steps(8);
    let img = fx.h.render().expect("render");
    let path = dir.join(format!("{name}.png"));
    img.save(&path).unwrap();
    eprintln!("screenshot: {}", path.display());
}

#[test]
#[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn m3b_screenshots() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS").map(PathBuf::from) else {
        eprintln!("MYSSH_DEMO_SHOTS não definido: nada a fazer");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut fx = build(Overwrite::Ask, true);
    fx.mem.add(mem_entry(
        "/home/ana/atual",
        Kind::Symlink,
        0,
        1_700_300_000,
    ));
    fx.h.state_mut().refresh(Side::Remote);
    fx.wait("link remoto", |p| {
        p.remote_pane.entries.iter().any(|e| e.name == "atual")
    });
    fx.h.get_all_by_label("Ocultos").next().unwrap().click();
    fx.h.run_steps(2);
    fx.click_row("a.txt, Arquivo");
    fx.h.get_by_label_contains("b.log, Arquivo")
        .click_modifiers(Modifiers::COMMAND);
    fx.h.run_steps(2);
    fx.click_row("notas.txt, Arquivo");
    shot(&mut fx, &dir, "m3b-01-painel-duas-colunas");

    // fila: um em andamento, um contando, um na fila, um com falhas, um concluído
    for _ in 0..5 {
        fx.h.state_mut()
            .handle_for_test(super::column::Act::Transfer(Side::Remote));
        fx.h.run_steps(1);
    }
    fx.push(progress(
        1,
        "/home/ana/projeto/dados/relatorio-final.pdf",
        38_000_000,
        120_000_000,
        4,
        12,
        6_400_000,
    ));
    fx.push(TransferEvent::Scanning {
        id: tid(2),
        files: 214,
        bytes: 83_000_000,
    });
    fx.push(progress(4, "/home/ana/logs/app.log", 9_000, 9_000, 3, 3, 0));
    fx.push(TransferEvent::FileError {
        id: tid(4),
        path: "/home/ana/logs/lock".into(),
        message: "Permissão negada.".into(),
    });
    fx.push(TransferEvent::Finished {
        id: tid(4),
        ok: 2,
        failed: 1,
        skipped: 0,
    });
    fx.push(progress(5, "x", 1, 1, 1, 1, 0));
    fx.push(TransferEvent::Finished {
        id: tid(5),
        ok: 1,
        failed: 0,
        skipped: 0,
    });
    shot(&mut fx, &dir, "m3b-02-fila-progresso");

    // conflito
    let _rx = fx.mem.push_conflict(
        tid(1),
        "/home/ana/notas.txt",
        RemoteOrLocalMeta {
            kind: Kind::File,
            size: 118_000,
            mtime: Some(secs(1_700_000_000)),
        },
    );
    fx.h.run_steps(3);
    shot(&mut fx, &dir, "m3b-03-conflito");
    fx.h.get_by_label("Pular").click();
    fx.h.run_steps(3);

    // confirmação de exclusão remota recursiva
    fx.click_row("docs, Pasta");
    fx.h.key_press(Key::Delete);
    shot(&mut fx, &dir, "m3b-04-confirmar-exclusao");
}
