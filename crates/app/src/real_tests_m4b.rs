//! M4-B: sincronização pela UI contra o servidor real do `testenv`.
//!
//! ```text
//! MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys \
//!   cargo test -p myssh-app real_sync -- --ignored --nocapture --test-threads=1
//! ```
//!
//! O estado remoto é alterado "por fora" com `docker exec` (o container do testenv), como faz um
//! outro usuário/processo depois da prévia. Diretório remoto: `~/m3c-ui` (recriado).

use std::path::Path;
use std::process::Command;

use crate::files::model::Side;
use crate::files::sync_ui::Phase;
use crate::real_tests_m3c::{Ctx, WORK, fp, setup};

fn remote_sh(cmd: &str) {
    let out = Command::new("docker")
        .args(["exec", "-u", "tester", "myssh-testsshd", "sh", "-c", cmd])
        .output()
        .expect("docker exec");
    assert!(
        out.status.success(),
        "{cmd}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(p: &Path, content: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn has(c: &Ctx, text: &str) -> bool {
    c.h.query_all_by_label_contains(text).next().is_some()
}

use egui_kittest::kittest::Queryable;

fn open_dialog(c: &mut Ctx) {
    c.click("Sincronizar…");
}

fn preview(c: &mut Ctx, mirror: bool) {
    open_dialog(c);
    if mirror {
        c.h.get_by_label_contains("Modo espelho").click();
        c.h.run_steps(2);
    }
    c.click_dialog("Gerar prévia");
    c.wait_panel("prévia", 30, |p| matches!(p.sync.phase, Phase::Preview(_)));
}

fn finish(c: &mut Ctx) {
    c.wait_panel("fim da sincronização", 60, |p| {
        matches!(p.sync.phase, Phase::Finished(_))
    });
}

fn remote_has(c: &mut Ctx, name: &str) -> bool {
    crate::real_tests_m3c::fpm(&mut c.h).refresh(Side::Remote);
    c.wait_panel("remoto", 10, |p| !p.remote_pane.loading);
    c.remote_names().iter().any(|n| n == name)
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_sync_fluxo_completo() {
    let mut c = setup();
    let up = tempfile::tempdir().unwrap();
    let u = up.path();
    write(&u.join("relatório ação.txt"), "olá");
    write(&u.join("dir ç/arquivo ü.txt"), "conteúdo ü");
    write(&u.join("mudar.txt"), "versão 1");
    write(&u.join(".git/config"), "segredo-git");
    c.nav(Side::Local, &u.to_string_lossy());
    c.nav(Side::Remote, WORK);

    // 1. prévia local → remoto com Unicode; `.git/` fica de fora
    open_dialog(&mut c);
    c.shot("m4b-real-01-opcoes");
    c.click_dialog("Gerar prévia");
    c.wait_panel("prévia", 30, |p| matches!(p.sync.phase, Phase::Preview(_)));
    c.shot("m4b-real-02-previa");
    {
        let Phase::Preview(plan) = &fp(c.h.state()).sync.phase else {
            unreachable!()
        };
        let paths: Vec<_> = plan.items.iter().map(|i| i.path.to_string()).collect();
        assert!(
            paths.contains(&"relatório ação.txt".to_string()),
            "{paths:?}"
        );
        assert!(
            paths.contains(&"dir ç/arquivo ü.txt".to_string()),
            "{paths:?}"
        );
        assert!(!paths.iter().any(|p| p.contains(".git")), "{paths:?}");
        assert_eq!(plan.summary().delete, 0);
    }
    assert!(has(&c, "Criar "));
    c.click_dialog("Executar");
    finish(&mut c);
    c.shot("m4b-real-03-concluida");
    c.click_dialog("Fechar");
    assert!(remote_has(&mut c, "relatório ação.txt"));
    assert!(remote_has(&mut c, "dir ç"));
    assert!(!remote_has(&mut c, ".git"));

    // 2. extra no destino continua (padrão sem espelho)
    remote_sh(&format!("echo extra > {WORK}/extra.txt"));
    preview(&mut c, false);
    {
        let Phase::Preview(plan) = &fp(c.h.state()).sync.phase else {
            unreachable!()
        };
        assert_eq!(plan.summary().delete, 0, "sem espelho nada é excluído");
    }
    assert!(!has(&c, "Confirmo excluir"));
    c.click_dialog("Fechar");
    assert!(remote_has(&mut c, "extra.txt"));

    // 3. espelho: sem marcar o checkbox o botão não executa; confirmado apaga só o planejado
    preview(&mut c, true);
    c.shot("m4b-real-04-previa-espelho");
    assert!(has(&c, "Confirmo excluir 1 itens no destino"));
    c.click_dialog("Executar");
    assert!(matches!(fp(c.h.state()).sync.phase, Phase::Preview(_)));
    assert!(
        remote_has(&mut c, "extra.txt"),
        "não pode apagar sem confirmação"
    );
    // aparece depois da prévia: não estava no plano, então não pode ser apagado
    remote_sh(&format!("echo depois > {WORK}/surgiu-depois.txt"));
    c.h.get_by_label_contains("Confirmo excluir 1").click();
    c.h.run_steps(2);
    c.click_dialog("Executar e excluir 1");
    finish(&mut c);
    c.click_dialog("Fechar");
    assert!(
        !remote_has(&mut c, "extra.txt"),
        "o item planejado foi apagado"
    );
    assert!(
        remote_has(&mut c, "surgiu-depois.txt"),
        "só o planejado é apagado"
    );
    assert!(remote_has(&mut c, "relatório ação.txt"));
    remote_sh(&format!("rm -f {WORK}/surgiu-depois.txt"));

    // 4. alteração no destino depois da prévia → PlanStale na UI, nada executado
    write(&u.join("mudar.txt"), "versão 2 bem maior que a primeira");
    fpm_refresh_local(&mut c);
    preview(&mut c, false);
    remote_sh(&format!("echo alterado-por-fora >> {WORK}/mudar.txt"));
    c.click_dialog("Executar");
    c.wait_panel("stale", 30, |p| matches!(p.sync.phase, Phase::Stale(_)));
    assert!(has(&c, "A prévia ficou velha: nada foi executado"));
    assert!(has(&c, "mudar.txt  (mudou no destino)"));
    c.shot("m4b-real-05-plano-velho");
    c.click_dialog("Gerar nova prévia");
    c.wait_panel("nova prévia", 30, |p| {
        matches!(p.sync.phase, Phase::Preview(_))
    });
    c.click_dialog("Fechar");
    // limpeza
    remote_sh(&format!("rm -rf {WORK}"));
}

fn fpm_refresh_local(c: &mut Ctx) {
    crate::real_tests_m3c::fpm(&mut c.h).refresh(Side::Local);
    c.wait_panel("local", 10, |p| !p.local.loading);
}
