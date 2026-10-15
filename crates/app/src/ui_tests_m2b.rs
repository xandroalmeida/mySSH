//! Testes de UI headless do M2-B: tema por perfil, painéis divididos, importação do ssh config,
//! indicador do cofre e exportar/importar configuração.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::{App, Dialog, ImportStage, SplitDialog};
use crate::split::Dir;
use crate::tabs::TabKind;
use crate::ui_tests::{form, harness, memory_app, submit, wait_until};

fn app_with_profile(name: &str) -> Harness<'static, App> {
    let mut h = harness(memory_app());
    submit(&mut h, form(name));
    wait_until(&mut h, "perfil salvo", |a| a.profiles.len() == 1);
    h
}

fn split_of<'a>(h: &'a Harness<'static, App>) -> &'a crate::split_body::SplitBody {
    let app = h.state();
    app.tabs
        .iter()
        .find(|t| Some(t.id) == app.active)
        .and_then(|t| t.body.as_split_ref())
        .expect("aba dividida")
}

const CS: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

#[test]
fn editar_tema_mostra_previa_e_salva() {
    let mut h = app_with_profile("srv");
    let id = h.state().profiles[0].id;
    h.state_mut().edit_profile_dialog(id);
    h.run_steps(3);
    // seção recolhida (perfil herda tudo): abrir
    h.get_by_label_contains("Tema deste perfil").click();
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("ls --color").is_some(),
        "prévia visível"
    );
    // escolher a paleta (o popup do ComboBox não é endereçável no kittest: muda o estado)
    if let Dialog::Profile(d) = &mut h.state_mut().dialog {
        d.form.theme.palette = Some("cyberpunk".into());
    }
    h.run_steps(3);
    assert!(
        h.query_by_value("Cyberpunk Neon").is_some(),
        "combo mostra a paleta"
    );
    // "Usar destaque" liga a cor de destaque sugerida pela paleta
    h.get_by_label("Usar destaque").click();
    h.run_steps(3);
    // "Tamanho próprio" liga a fonte própria com o tamanho global como ponto de partida
    h.get_by_label("Tamanho próprio").click();
    h.run_steps(3);
    let Dialog::Profile(d) = &h.state().dialog else {
        panic!("diálogo esperado");
    };
    assert_eq!(d.form.theme.palette.as_deref(), Some("cyberpunk"));
    assert_eq!(d.form.theme.accent, Some([0xff, 0x2b, 0xd6]));
    assert_eq!(d.form.theme.font_size_tenths, Some(140));
    // salvar grava o tema no perfil
    let ctx = h.ctx.clone();
    h.state_mut().submit_profile_dialog(&ctx);
    wait_until(&mut h, "salvo", |a| matches!(a.dialog, Dialog::None));
    let t = &h.state().profiles[0].theme;
    assert_eq!(t.palette.as_deref(), Some("cyberpunk"));
    assert_eq!(t.accent, Some([0xff, 0x2b, 0xd6]));
    assert_eq!(t.font_size_tenths, Some(140));
}

#[test]
fn tema_editado_vale_para_aba_ja_aberta() {
    let mut h = app_with_profile("srv");
    let id = h.state().profiles[0].id;
    h.state_mut().open_profile(id);
    let mut p = h.state().profiles[0].clone();
    p.theme.accent = Some([255, 43, 214]);
    h.state().store.update_profile(&p).unwrap();
    h.state_mut().reload();
    let app = h.state();
    let TabKind::Terminal { profile, .. } = &app.tabs.last().unwrap().kind else {
        panic!()
    };
    assert_eq!(profile.theme.accent, Some([255, 43, 214]));
}

#[test]
fn dividir_e_fechar_painel() {
    let mut h = app_with_profile("srv");
    let id = h.state().profiles[0].id;
    h.state_mut().open_profile(id);
    h.run_steps(2);
    // Cmd/Ctrl+Shift+D: divide lado a lado com o mesmo perfil
    h.key_press_modifiers(CS, Key::D);
    h.run_steps(3);
    assert_eq!(split_of(&h).pane_count(), 2);
    // Cmd/Ctrl+Shift+E: empilhado
    h.key_press_modifiers(CS, Key::E);
    h.run_steps(3);
    assert_eq!(split_of(&h).pane_count(), 3);
    assert_eq!(split_of(&h).tree().count(), 3);
    let ids = split_of(&h).pane_ids();
    assert_eq!(
        split_of(&h).focused_id(),
        ids[2],
        "novo painel ganha o foco"
    );
    // próximo painel
    h.key_press_modifiers(CS, Key::J);
    h.run_steps(2);
    assert_eq!(split_of(&h).focused_id(), ids[0]);
    // fechar o painel focado (atalho): sem sessão ativa, fecha direto
    h.key_press_modifiers(CS, Key::W);
    h.run_steps(3);
    assert_eq!(split_of(&h).pane_count(), 2);
    assert_eq!(h.state().tabs.len(), 2, "a aba não foi fechada");
    // botão × do painel
    h.get_all_by_label("×").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(split_of(&h).pane_count(), 1);
    // último painel: fechar o painel fecha a aba
    let (tab, pane) = (h.state().active.unwrap(), split_of(&h).focused_id());
    h.state_mut().request_close_pane(tab, pane);
    assert_eq!(h.state().tabs.len(), 1);
}

#[test]
fn dividir_com_outro_perfil_e_fechar_com_confirmacao() {
    let mut h = app_with_profile("srv");
    submit(&mut h, form("outro"));
    wait_until(&mut h, "segundo perfil", |a| a.profiles.len() == 2);
    let first = h
        .state()
        .profiles
        .iter()
        .find(|p| p.name == "srv")
        .unwrap()
        .id;
    h.state_mut().open_profile(first);
    h.state_mut().open_split_picker(Dir::Horizontal);
    assert!(matches!(
        h.state().dialog,
        Dialog::Split(SplitDialog { .. })
    ));
    h.run_steps(3);
    h.get_by_label_contains("outro  (ana@srv.local)").click();
    h.run_steps(2);
    h.get_all_by_label("Dividir").last().unwrap().click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::None));
    let sp = split_of(&h);
    assert_eq!(sp.pane_count(), 2);
    assert_eq!(sp.pane_title(sp.focused_id()), "outro");

    // painel com sessão ativa pede confirmação
    let (tab, pane) = (h.state().active.unwrap(), split_of(&h).focused_id());
    h.state_mut()
        .tabs
        .last_mut()
        .unwrap()
        .body
        .simulate_busy(true);
    h.state_mut().request_close_pane(tab, pane);
    assert!(matches!(h.state().dialog, Dialog::ConfirmClosePane { .. }));
    h.run_steps(2);
    h.get_by_label("Fechar painel").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::None));
    assert_eq!(split_of(&h).pane_count(), 1);
}

#[test]
fn atalho_de_divisao_exige_shift() {
    let mut h = app_with_profile("srv");
    let id = h.state().profiles[0].id;
    h.state_mut().open_profile(id);
    h.run_steps(2);
    // Ctrl+D puro (EOF no shell remoto) não divide
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run_steps(3);
    let app = h.state();
    let t = app.tabs.iter().find(|t| Some(t.id) == app.active).unwrap();
    assert!(t.body.as_split_ref().is_none());
}

#[test]
fn importar_ssh_config_fluxo_completo() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("config");
    std::fs::write(
        &cfg,
        "Host bastion\n  HostName bastion.example.com\n  User ops\n\
         Host web\n  HostName web.example.com\n  User deploy\n  ProxyJump bastion\n\
         Host evil\n  HostName evil.example.com\n  ProxyCommand nc %h %p\n\
         Host dup\n  HostName dup.example.com\n  User ana\n",
    )
    .unwrap();
    let mut h = harness(memory_app());
    // perfil existente que conflita com "dup"
    let mut f = form("dup antigo");
    f.host = "dup.example.com".into();
    submit(&mut h, f);
    wait_until(&mut h, "perfil existente", |a| a.profiles.len() == 1);

    h.state_mut().open_import_ssh();
    h.run_steps(3);
    assert!(h.query_by_label("Usar ~/.ssh/config").is_some());
    let ctx = h.ctx.clone();
    h.state_mut().start_import_load(&ctx, Some(cfg.clone()));
    wait_until(
        &mut h,
        "prévia",
        |a| matches!(&a.dialog, Dialog::ImportSsh(d) if d.stage == ImportStage::Preview),
    );
    h.run_steps(3);
    // conflito rotulado e desmarcado; aviso de comando local destacado
    assert!(h.query_all_by_label_contains("já existe").next().is_some());
    assert!(h.query_by_label_contains("executa comando local").is_some());
    {
        let Dialog::ImportSsh(d) = &h.state().dialog else {
            unreachable!()
        };
        assert_eq!(d.preview.items.len(), 4);
        assert_eq!(d.preview.selected_count(), 3);
        assert!(!d.preview.items[3].selected);
    }
    assert!(h.query_by_label("Importar 3 perfis").is_some());
    // desmarcar "evil" pela UI
    h.get_by_label("evil").click();
    h.run_steps(2);
    h.get_by_label("Importar 2 perfis").click();
    wait_until(&mut h, "importado", |a| {
        a.profiles.len() == 3 && matches!(a.dialog, Dialog::None)
    });
    let app = h.state();
    let names: Vec<_> = app.profiles.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"bastion") && names.contains(&"web"));
    let web = app.profiles.iter().find(|p| p.name == "web").unwrap();
    let bastion = app.profiles.iter().find(|p| p.name == "bastion").unwrap();
    assert_eq!(web.jump, Some(bastion.id));
    let folder = app
        .folders
        .iter()
        .find(|f| f.name == crate::ssh_import::TARGET_FOLDER)
        .unwrap();
    assert_eq!(web.folder, Some(folder.id));
    assert!(app.notice.is_some());
    // o arquivo do usuário não foi tocado
    assert!(
        std::fs::read_to_string(&cfg)
            .unwrap()
            .contains("ProxyCommand nc")
    );
}

#[test]
fn importar_arquivo_inexistente_mostra_erro() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(memory_app());
    h.state_mut().open_import_ssh();
    let ctx = h.ctx.clone();
    h.state_mut()
        .start_import_load(&ctx, Some(tmp.path().join("nao-existe")));
    wait_until(
        &mut h,
        "erro",
        |a| matches!(&a.dialog, Dialog::ImportSsh(d) if d.error.is_some()),
    );
    h.run_steps(2);
    assert!(h.query_by_label("Arquivo não encontrado.").is_some());
}

#[test]
fn cadeado_bloqueia_sem_encerrar_sessoes_e_encerrar_pede_confirmacao() {
    let mut h = app_with_profile("srv");
    let id = h.state().profiles[0].id;
    h.state_mut().open_profile(id);
    h.state_mut()
        .tabs
        .last_mut()
        .unwrap()
        .body
        .simulate_busy(true);
    h.run_steps(3);
    h.get_by_label_contains("Cofre aberto").click();
    h.run_steps(3);
    assert!(h.state().vault.is_locked());
    assert!(h.state().any_busy(), "bloquear o cofre não encerra sessões");
    assert!(
        h.query_all_by_label_contains("Cofre bloqueado")
            .next()
            .is_some()
    );
    // encerrar todas as sessões: botão separado, com confirmação
    h.get_by_label("Encerrar todas as sessões").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::ConfirmEndAll));
    h.get_by_label("Encerrar sessões").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Dialog::None));
    assert_eq!(h.state().tabs.len(), 2, "as abas continuam abertas");
}

#[test]
fn suspensao_bloqueia_cofre_se_configurado() {
    let mut h = harness(memory_app());
    h.state_mut().on_suspend(false);
    assert!(!h.state().vault.is_locked());
    h.state_mut().on_suspend(true);
    assert!(h.state().vault.is_locked());
    assert_eq!(h.state().tabs.len(), 1, "sessões/abas intactas");

    let mut h = harness(memory_app());
    h.state_mut().settings.vault_lock_on_suspend = false;
    h.state_mut().on_suspend(true);
    assert!(!h.state().vault.is_locked());
}

#[test]
fn configuracoes_tem_exportar_importar_com_aviso() {
    let mut h = harness(memory_app());
    h.state_mut().dialog = Dialog::Settings;
    h.run_steps(3);
    assert!(h.query_by_label("Exportar configuração…").is_some());
    assert!(h.query_by_label("Importar configuração…").is_some());
    assert!(
        h.query_by_label_contains("Credenciais NÃO são exportadas")
            .is_some()
    );
}
