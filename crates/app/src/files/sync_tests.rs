//! Testes headless do diálogo de sincronização com `MemSync` (executor simulado).

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use egui_kittest::kittest::Queryable;
use myssh_syncrun::myssh_sync::{
    Action, Changed, ConflictReason, Direction, EntryKind, Journal, Plan, PlanItem, RelPath,
    SkipReason, Snapshot, UpdateReason,
};
use myssh_syncrun::{ConflictChoice, RunSummary, SyncEvent, SyncState};

use super::model::Side;
use super::sync_engine::{ExecScript, MemSync};
use super::sync_ui::Phase;
use super::ui_tests::{Fx, fixture};

fn snap(size: u64) -> Option<Snapshot> {
    Some(Snapshot {
        kind: EntryKind::File,
        size,
        mtime: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
    })
}

fn item(path: &str, action: Action) -> PlanItem {
    PlanItem {
        path: RelPath::parse(path).unwrap(),
        kind: EntryKind::File,
        action,
        src: snap(100),
        dst: snap(50),
    }
}

pub(super) fn plan(mirror: bool) -> Plan {
    let mut items = vec![
        item("novo ação.txt", Action::Create),
        item(
            "mudou.txt",
            Action::Update {
                reason: UpdateReason::SizeDiffers,
            },
        ),
        item(
            "disputa.txt",
            Action::Conflict {
                reason: ConflictReason::DestNewer,
            },
        ),
        item(
            "colide.txt",
            Action::Conflict {
                reason: ConflictReason::DestCollision {
                    existing: "COLIDE.txt".into(),
                },
            },
        ),
        item(
            "igual.txt",
            Action::Skip {
                reason: SkipReason::Identical,
            },
        ),
    ];
    if mirror {
        items.push(item("velho.txt", Action::Delete));
        items.push(item("outro velho.txt", Action::Delete));
        items.push(item("terceiro.txt", Action::Delete));
    }
    Plan {
        direction: Direction::LocalToRemote,
        mirror_deletes: mirror,
        items,
    }
}

fn with_engine(fx: &mut Fx, mem: &Arc<MemSync>) {
    fx.h.state_mut().set_sync_engine(mem.clone());
}

fn open(fx: &mut Fx) {
    fx.h.get_by_label("Sincronizar…").click();
    fx.h.run_steps(3);
}

fn has(fx: &Fx, text: &str) -> bool {
    fx.h.query_all_by_label_contains(text).next().is_some()
}

fn click_dialog(fx: &mut Fx, label: &str) {
    fx.h.query_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("botão {label}"))
        .click();
    fx.h.run_steps(3);
}

fn to_preview(fx: &mut Fx, mem: &Arc<MemSync>, mirror: bool) {
    *mem.plan_result.lock().unwrap() = Some(plan(mirror));
    open(fx);
    if mirror {
        fx.h.get_by_label_contains("Modo espelho").click();
        fx.h.run_steps(2);
    }
    click_dialog(fx, "Gerar prévia");
    fx.wait("prévia", |p| matches!(p.sync.phase, Phase::Preview(_)));
}

#[test]
fn sem_executor_o_botao_avisa() {
    let mut fx = fixture();
    // o botão existe e está habilitado (há remoto), mas o executor ainda não foi ligado
    open(&mut fx);
    assert!(!fx.p().sync.open);
    assert!(
        fx.p()
            .notice
            .as_deref()
            .is_some_and(|n| n.contains("indisponível"))
    );
}

#[test]
fn dialogo_de_opcoes_e_avisos() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    open(&mut fx);
    assert!(fx.p().sync.open && fx.p().needs_modal());
    // raízes = pastas abertas das duas colunas
    assert_eq!(fx.p().sync.remote_root, "/home/ana");
    assert_eq!(fx.p().sync.local_root, fx.tmp.path());
    // direção explícita, com o destino dito
    assert!(has(&fx, "O destino (Remoto) será alterado"));
    fx.h.get_by_label("Remoto ➡ Local").click();
    fx.h.run_steps(2);
    assert!(has(&fx, "O destino (Local) será alterado"));
    // espelho desligado por padrão; ligar mostra o aviso
    assert!(!fx.p().sync.mirror);
    assert!(!has(&fx, "ATENÇÃO"));
    fx.h.get_by_label_contains("Modo espelho").click();
    fx.h.run_steps(2);
    assert!(has(&fx, "ATENÇÃO"));
    // conteúdo: aviso de custo de rede
    assert!(!has(&fx, "Custo de rede"));
    fx.h.get_by_label_contains("Conteúdo").click();
    fx.h.run_steps(2);
    assert!(has(&fx, "Custo de rede"));
    // .git/ é o padrão das exclusões
    assert_eq!(fx.p().sync.excl_text, ".git/");
}

#[test]
fn padroes_de_exclusao_invalidos_bloqueiam_a_previa() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    open(&mut fx);
    fx.h.state_mut().sync.excl_text = "[abc".into();
    fx.h.run_steps(3);
    assert!(has(&fx, "Padrão inválido"));
    click_dialog(&mut fx, "Gerar prévia");
    assert!(mem.plan_requests.lock().unwrap().is_empty());
}

#[test]
fn previa_mostra_resumo_motivos_e_conflitos_com_padrao_pular() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    to_preview(&mut fx, &mem, false);
    let req = mem.plan_requests.lock().unwrap()[0].clone();
    assert_eq!(req.direction, Direction::LocalToRemote);
    assert!(!req.mirror && !req.content);
    assert_eq!(req.exclusions, vec![".git/"]);
    assert_eq!(req.remote_root, "/home/ana");
    assert!(has(
        &fx,
        "Criar 1 · Atualizar 1 · Conflitos 2 · Excluir 0 · Pular 1"
    ));
    assert!(has(&fx, "O tamanho é diferente."));
    assert!(has(&fx, "não dá para saber qual vale"));
    assert!(has(&fx, "colide com «COLIDE.txt»"));
    // sem espelho: nenhuma seção ou filtro de exclusão
    assert!(!has(&fx, "Confirmo excluir"));
    assert!(
        fx.h.query_all_by_label_contains("Excluir (")
            .next()
            .is_none()
    );
    // só o conflito resolvível tem escolha; o padrão é Pular
    assert!(fx.p().sync.choices.is_empty());
    assert!(has(&fx, "(não resolvido automaticamente)"));
    // filtro por ação
    fx.h.get_by_label_contains("Conflitos (2)").click();
    fx.h.run_steps(2);
    assert!(!has(&fx, "novo ação.txt"));
    assert!(has(&fx, "disputa.txt"));
}

#[test]
fn espelho_exige_o_checkbox_e_o_botao_traz_o_numero() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    to_preview(&mut fx, &mem, true);
    assert!(mem.plan_requests.lock().unwrap()[0].mirror);
    assert!(has(&fx, "serão APAGADOS"));
    assert!(has(&fx, "Confirmo excluir 3 itens no destino"));
    // sem marcar: o botão "Executar" existe, mas não executa
    click_dialog(&mut fx, "Executar");
    assert!(matches!(fx.p().sync.phase, Phase::Preview(_)));
    assert!(mem.executions.lock().unwrap().is_empty());
    fx.h.get_by_label_contains("Confirmo excluir 3").click();
    fx.h.run_steps(2);
    assert!(has(&fx, "Executar e excluir 3"));
}

#[test]
fn executar_envia_as_decisoes_e_mostra_o_resumo() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    *mem.exec.lock().unwrap() = Some(ExecScript::Events(vec![
        SyncEvent::Progress {
            items_done: 2,
            items_total: 5,
            bytes_done: 100,
            bytes_total: 500,
            current: "novo ação.txt".into(),
        },
        SyncEvent::ItemFailed {
            path: RelPath::parse("mudou.txt").unwrap(),
            op: myssh_syncrun::myssh_sync::Op::Update,
            message: "Permissão negada.".into(),
        },
        SyncEvent::Finished(RunSummary {
            done: 4,
            failed: 1,
            skipped: 1,
            pending: 0,
            bytes: 500,
            cancelled: false,
        }),
    ]));
    to_preview(&mut fx, &mem, true);
    // conflito resolvível: usar origem; o outro (colisão) continua sem escolha
    fx.h.get_by_label("Usar origem").click();
    fx.h.run_steps(2);
    fx.h.get_by_label_contains("Confirmo excluir 3").click();
    fx.h.run_steps(2);
    click_dialog(&mut fx, "Executar e excluir 3");
    fx.wait("fim", |p| matches!(p.sync.phase, Phase::Finished(_)));
    let ex = mem.executions.lock().unwrap();
    assert_eq!(ex.len(), 1);
    let (_, d, resumed) = &ex[0];
    assert!(d.confirm_deletes && !resumed);
    let keys: Vec<_> = d
        .conflicts
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    assert_eq!(
        keys,
        vec![("disputa.txt".to_string(), ConflictChoice::UseSource)]
    );
    drop(ex);
    assert!(has(&fx, "Sincronização concluída com falhas"));
    assert!(has(&fx, "4 ok · 1 com falha · 1 pulados"));
    assert!(has(&fx, "mudou.txt: Permissão negada."));
}

#[test]
fn plano_velho_nunca_executa_e_oferece_nova_previa() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    *mem.exec.lock().unwrap() = Some(ExecScript::Stale(vec![Changed {
        path: RelPath::parse("mudou.txt").unwrap(),
        side: myssh_syncrun::myssh_sync::Side::Dest,
        expected: snap(50),
        actual: snap(77),
    }]));
    to_preview(&mut fx, &mem, false);
    click_dialog(&mut fx, "Executar");
    fx.wait("stale", |p| matches!(p.sync.phase, Phase::Stale(_)));
    assert!(has(&fx, "A prévia ficou velha: nada foi executado"));
    assert!(has(&fx, "mudou depois da prévia (1 item(ns))"));
    assert!(has(&fx, "mudou.txt  (mudou no destino)"));
    // o botão gera uma NOVA prévia (nova chamada ao planejador)
    click_dialog(&mut fx, "Gerar nova prévia");
    fx.wait("nova prévia", |p| {
        matches!(p.sync.phase, Phase::Preview(_))
    });
    assert_eq!(mem.plan_requests.lock().unwrap().len(), 2);
}

#[test]
fn execucao_em_andamento_pode_ser_ocultada_e_cancelada() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    *mem.exec.lock().unwrap() = Some(ExecScript::UntilCancelled(vec![SyncEvent::Progress {
        items_done: 1,
        items_total: 4,
        bytes_done: 50,
        bytes_total: 200,
        current: "x.bin".into(),
    }]));
    to_preview(&mut fx, &mem, false);
    click_dialog(&mut fx, "Executar");
    fx.wait("rodando", |p| {
        p.sync_running() && p.sync.progress.items_done == 1
    });
    assert!(has(&fx, "itens 1/4"));
    assert!(has(&fx, "x.bin"));
    // ocultar: continua rodando, e o painel mostra o aviso para reabrir
    click_dialog(&mut fx, "Ocultar");
    assert!(!fx.p().sync.open && fx.p().sync_running());
    assert!(has(&fx, "Sincronização em andamento"));
    fx.h.get_by_label("Mostrar").click();
    fx.h.run_steps(3);
    assert!(fx.p().sync.open);
    click_dialog(&mut fx, "Cancelar sincronização");
    fx.wait("cancelada", |p| matches!(p.sync.phase, Phase::Finished(_)));
    assert!(has(&fx, "Sincronização cancelada"));
    assert!(!fx.p().sync_running());
}

#[test]
fn cancelar_tudo_ao_fechar_cancela_a_sincronizacao() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    *mem.exec.lock().unwrap() = Some(ExecScript::UntilCancelled(vec![]));
    to_preview(&mut fx, &mem, false);
    click_dialog(&mut fx, "Executar");
    fx.wait("rodando", |p| p.sync_running());
    fx.h.state_mut().cancel_all();
    fx.wait("cancelada", |p| !p.sync_running());
}

#[test]
fn sincronizacao_interrompida_e_oferecida_e_retomada() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    let p = plan(false);
    let mut journal = Journal::from_plan(&p);
    // o primeiro item já foi feito; os demais ficaram pendentes
    let first = journal.items[0].clone();
    journal.mark(
        &first.path,
        first.op,
        myssh_syncrun::myssh_sync::Status::Done,
    );
    *mem.state.lock().unwrap() = Some(SyncState {
        version: 1,
        profile_id: "p".into(),
        plan: p,
        journal,
    });
    *mem.exec.lock().unwrap() = Some(ExecScript::Events(vec![]));
    open(&mut fx);
    fx.wait("oferta", |p| p.sync.resume.is_some());
    assert!(has(&fx, "interrompida com 1 item(ns) pendentes"));
    click_dialog(&mut fx, "Retomar");
    // só o que falta aparece
    assert!(matches!(fx.p().sync.phase, Phase::Preview(_)));
    assert!(has(&fx, "mudou.txt"));
    assert!(!has(&fx, "novo ação.txt"));
    click_dialog(&mut fx, "Retomar (Executar)");
    fx.wait("fim", |p| matches!(p.sync.phase, Phase::Finished(_)));
    assert!(mem.executions.lock().unwrap()[0].2, "deve usar a retomada");
}

#[test]
fn descartar_a_retomada_apaga_o_estado() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    let p = plan(false);
    let journal = Journal::from_plan(&p);
    *mem.state.lock().unwrap() = Some(SyncState {
        version: 1,
        profile_id: "p".into(),
        plan: p,
        journal,
    });
    open(&mut fx);
    fx.wait("oferta", |p| p.sync.resume.is_some());
    click_dialog(&mut fx, "Descartar");
    fx.wait("sem oferta", |p| p.sync.resume.is_none());
    assert!(mem.state.lock().unwrap().is_none());
}

fn shot(fx: &mut Fx, dir: &std::path::Path, name: &str) {
    fx.h.run_steps(8);
    let img = fx.h.render().expect("render");
    let path = dir.join(format!("{name}.png"));
    img.save(&path).unwrap();
    eprintln!("screenshot: {}", path.display());
}

#[test]
#[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn m4b_screenshots() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS").map(std::path::PathBuf::from) else {
        eprintln!("MYSSH_DEMO_SHOTS não definido: nada a fazer");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut fx = super::ui_tests::build(myssh_transfer::Overwrite::Ask, true);
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);

    // 1. opções (espelho ligado: aviso vermelho)
    open(&mut fx);
    fx.h.get_by_label_contains("Modo espelho").click();
    fx.h.run_steps(2);
    shot(&mut fx, &dir, "m4b-01-opcoes");
    click_dialog(&mut fx, "Cancelar");

    // 2. prévia com conflitos (sem espelho: nenhuma seção de exclusão)
    to_preview(&mut fx, &mem, false);
    fx.h.get_by_label("Usar origem").click();
    fx.h.run_steps(2);
    shot(&mut fx, &dir, "m4b-02-previa-conflitos");
    click_dialog(&mut fx, "Fechar");

    // 3. prévia espelho com exclusões
    to_preview(&mut fx, &mem, true);
    shot(&mut fx, &dir, "m4b-03-previa-espelho");
    fx.h.get_by_label_contains("Confirmo excluir 3").click();
    fx.h.run_steps(2);
    shot(&mut fx, &dir, "m4b-03b-previa-espelho-confirmada");

    // 4. PlanStale
    *mem.exec.lock().unwrap() = Some(ExecScript::Stale(vec![Changed {
        path: RelPath::parse("mudou.txt").unwrap(),
        side: myssh_syncrun::myssh_sync::Side::Dest,
        expected: snap(50),
        actual: snap(77),
    }]));
    click_dialog(&mut fx, "Executar e excluir 3");
    fx.wait("stale", |p| matches!(p.sync.phase, Phase::Stale(_)));
    shot(&mut fx, &dir, "m4b-04-plano-velho");
}

#[test]
fn progresso_do_hash_aparece_na_geracao_da_previa() {
    let mut fx = fixture();
    let mem = Arc::new(MemSync::default());
    with_engine(&mut fx, &mem);
    open(&mut fx);
    let p = fx.h.state_mut();
    p.sync.phase = Phase::Planning {
        done: 0,
        total: 0,
        current: String::new(),
    };
    p.on_sync_event(SyncEvent::Hashing {
        done: 3,
        total: 10,
        current: "dir/grande.bin".into(),
    });
    fx.h.run_steps(3);
    assert!(has(&fx, "Comparando conteúdo: 3 de 10"));
    assert!(has(&fx, "dir/grande.bin"));
    let _ = Side::Local;
}
