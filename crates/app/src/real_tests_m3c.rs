//! M3-C: painel de arquivos contra o servidor real do `testenv` (SFTP de verdade).
//!
//! ```text
//! MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys \
//!   cargo test -p myssh-app real_arquivos -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Com `MYSSH_DEMO_SHOTS=dir` (e GPU) o mesmo fluxo também grava PNGs.
//! O diretório de trabalho remoto é `~/m3c-ui` (recriado); `/home/tester/small` é um tmpfs
//! de 2 MiB (disco cheio).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use myssh_core::AuthMethod;
use myssh_store::Store;

use crate::app::App;
use crate::files::FilesPanel;
use crate::files::model::Side;
use crate::files::queue::State;
use crate::i18n;
use crate::real_tests::{
    add_profile, keys_dir, make_app, percentile, profile, ready, screen, status, trust_server,
    type_line, wait,
};

pub(crate) const REMOTE_HOME: &str = "/home/tester";
pub(crate) const WORK: &str = "/home/tester/m3c-ui";

pub(crate) fn fp(a: &App) -> &FilesPanel {
    a.tabs[1].body.files_panel().expect("painel de arquivos")
}

pub(crate) fn fpm<'a>(h: &'a mut Harness<'static, App>) -> &'a mut FilesPanel {
    h.state_mut().tabs[1]
        .body
        .files_panel_mut()
        .expect("painel de arquivos")
}

pub(crate) struct Ctx {
    pub(crate) h: Harness<'static, App>,
    pub(crate) shots: Option<PathBuf>,
    pub(crate) frame_ms: Vec<f64>,
    /// Diretório de dados do app nesta execução (journal da sincronização).
    pub(crate) _data: tempfile::TempDir,
}

impl Ctx {
    pub(crate) fn shot(&mut self, name: &str) {
        let Some(dir) = self.shots.clone() else {
            return;
        };
        self.h.run_steps(6);
        let img = self.h.render().expect("render");
        let path = dir.join(format!("{name}.png"));
        img.save(&path).unwrap();
        eprintln!("screenshot: {}", path.display());
    }

    pub(crate) fn click(&mut self, label: &str) {
        self.h.get_by_label(label).click();
        self.h.run_steps(3);
    }

    /// Linha por texto; se o nome existir nas duas colunas, escolhe pelo lado (local = primeira).
    fn row_click(&mut self, side: Side, text: &str, mods: Modifiers) {
        let nodes: Vec<_> = self.h.query_all_by_label_contains(text).collect();
        let n = match (nodes.len(), side) {
            (0, _) => panic!("linha não encontrada: {text}"),
            (1, _) => &nodes[0],
            (_, Side::Local) => &nodes[0],
            (_, Side::Remote) => &nodes[1],
        };
        n.click_modifiers(mods);
        self.h.run_steps(2);
    }

    /// Botão do diálogo modal (o rótulo também existe na barra lateral; o modal vem por último).
    pub(crate) fn click_dialog(&mut self, label: &str) {
        let n = self.h.query_all_by_label(label).last().expect("botão");
        n.click();
        self.h.run_steps(3);
    }

    pub(crate) fn click_row(&mut self, side: Side, text: &str) {
        self.row_click(side, text, Modifiers::NONE);
    }

    pub(crate) fn cmd_click_row(&mut self, side: Side, text: &str) {
        self.row_click(side, text, Modifiers::COMMAND);
    }

    pub(crate) fn wait_panel(&mut self, what: &str, secs: u64, cond: impl Fn(&FilesPanel) -> bool) {
        wait(&mut self.h, what, secs, |a| cond(fp(a)));
    }

    /// Espera a fila esvaziar e mede o tempo de cada frame enquanto há transferência ativa.
    pub(crate) fn wait_idle(&mut self, what: &str, secs: u64) {
        let t0 = Instant::now();
        loop {
            let busy = {
                let p = fp(self.h.state());
                p.active() > 0 || p.queue.items.is_empty()
            };
            if !busy {
                break;
            }
            assert!(t0.elapsed() < Duration::from_secs(secs), "timeout: {what}");
            let f = Instant::now();
            self.h.step();
            self.frame_ms.push(f.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        self.h.run_steps(3);
    }

    /// Esvazia as concluídas (a lista da fila rola; mantém o item novo visível).
    pub(crate) fn clear_queue(&mut self) {
        self.h.get_by_label("Limpar concluídas").click();
        self.h.run_steps(2);
    }

    pub(crate) fn nav(&mut self, side: Side, path: &str) {
        fpm(&mut self.h).navigate(side, path.to_string(), true);
        let want = path.to_string();
        self.wait_panel("navegação", 10, move |p| {
            let pane = p.pane(side);
            pane.path == want && !pane.loading
        });
    }

    pub(crate) fn remote_names(&self) -> Vec<String> {
        fp(self.h.state())
            .remote_pane
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    fn state_of(&self, idx: usize) -> State {
        fp(self.h.state()).queue.items[idx].state
    }
}

pub(crate) fn setup() -> Ctx {
    ready();
    let shots = std::env::var_os("MYSSH_DEMO_SHOTS").map(PathBuf::from);
    if let Some(d) = &shots {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut app = make_app(Store::open_in_memory().unwrap());
    trust_server(&app);
    let p = profile(
        "arquivos",
        AuthMethod::KeyFile {
            path: keys_dir().join("id_ed25519"),
            passphrase: None,
        },
    );
    add_profile(&mut app, &p);
    let data = tempfile::tempdir().unwrap();
    app.data_dir = Some(data.path().to_path_buf());
    app.open_profile(p.id);
    let mut b = Harness::builder().with_size(egui::vec2(1200.0, 800.0));
    if shots.is_some() {
        b = b.with_pixels_per_point(1.25).wgpu();
    }
    let h = b.build_eframe(move |_| app);
    let mut c = Ctx {
        h,
        shots,
        frame_ms: Vec::new(),
        _data: data,
    };
    wait(&mut c.h, "conectado", 20, |a| {
        status(a, 1) == i18n::ST_CONNECTED
    });
    wait(&mut c.h, "prompt", 10, |a| !screen(a, 1).trim().is_empty());
    type_line(
        &mut c.h,
        &format!("rm -rf {WORK} && mkdir {WORK} && echo PRONTO-$((6*7))"),
    );
    wait(&mut c.h, "diretório de trabalho", 10, |a| {
        screen(a, 1).contains("PRONTO-42")
    });
    // atalho global: abre o painel
    c.h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    c.h.run_steps(3);
    c.wait_panel("home remoto listado", 20, |p| {
        p.remote_pane.path == REMOTE_HOME && !p.remote_pane.entries.is_empty()
    });
    c
}

fn write(p: &Path, content: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

#[test]
#[ignore = "requer servidor de teste (MYSSH_TEST_SSH, MYSSH_TEST_KEYS)"]
fn real_arquivos_fluxo_completo() {
    let mut c = setup();

    // 1. home remoto listado (inclui o tmpfs `small` e o diretório de trabalho)
    let names = c.remote_names();
    assert!(names.contains(&"small".to_string()), "{names:?}");
    assert!(names.contains(&"m3c-ui".to_string()), "{names:?}");
    c.nav(Side::Remote, WORK);

    // 2. envia arquivo e pasta com Unicode e espaços
    let up = tempfile::tempdir().unwrap();
    let u = up.path();
    write(&u.join("relatório final.txt"), "olá, mundo");
    write(&u.join("pasta com espaço/ação ü.txt"), "conteúdo ü");
    write(&u.join("pasta com espaço/sub/b.txt"), "bbb");
    c.nav(Side::Local, &u.to_string_lossy());
    c.shot("m3c-01-painel-real-listagens");
    c.click_row(Side::Local, "relatório final.txt, Arquivo");
    c.cmd_click_row(Side::Local, "pasta com espaço, Pasta");
    c.click("⬆ Enviar");
    c.wait_idle("upload", 30);
    assert_eq!(c.state_of(0), State::Done);
    assert_eq!(c.state_of(1), State::Done);
    c.wait_panel("remoto atualizado", 10, |p| {
        p.remote_pane
            .entries
            .iter()
            .any(|e| e.name == "pasta com espaço")
    });
    let names = c.remote_names();
    assert!(
        names.contains(&"relatório final.txt".to_string()),
        "{names:?}"
    );
    c.nav(Side::Remote, &format!("{WORK}/pasta com espaço"));
    let names = c.remote_names();
    assert!(names.contains(&"ação ü.txt".to_string()), "{names:?}");
    assert!(names.contains(&"sub".to_string()), "{names:?}");
    c.nav(Side::Remote, WORK);

    // 3. baixa de volta para outra pasta e compara o conteúdo
    let down = tempfile::tempdir().unwrap();
    c.nav(Side::Local, &down.path().to_string_lossy());
    c.click_row(Side::Remote, "relatório final.txt, Arquivo");
    c.cmd_click_row(Side::Remote, "pasta com espaço, Pasta");
    c.click("⬇ Baixar");
    c.wait_idle("download", 30);
    c.wait_panel("local atualizado", 10, |p| {
        p.local.entries.iter().any(|e| e.name == "pasta com espaço")
    });
    let d = down.path();
    assert_eq!(
        std::fs::read_to_string(d.join("relatório final.txt")).unwrap(),
        "olá, mundo"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("pasta com espaço/ação ü.txt")).unwrap(),
        "conteúdo ü"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("pasta com espaço/sub/b.txt")).unwrap(),
        "bbb"
    );

    // 4. conflito: Pular e depois Sobrescrever
    c.nav(Side::Local, &u.to_string_lossy());
    write(&u.join("relatório final.txt"), "conteúdo novo e maior");
    fpm(&mut c.h).refresh(Side::Local);
    c.wait_panel("local recarregado", 10, |p| {
        p.local
            .entries
            .iter()
            .any(|e| e.name == "relatório final.txt" && e.size == 22)
    });
    c.click_row(Side::Local, "relatório final.txt, Arquivo");
    c.click("⬆ Enviar");
    c.wait_panel("conflito", 15, |p| p.needs_modal());
    c.h.get_by_label_contains("Já existe um item");
    c.shot("m3c-03-conflito");
    c.click("Pular");
    c.wait_idle("pular", 15);
    let it = fp(c.h.state()).queue.items.last().unwrap().clone();
    assert_eq!((it.state, it.skipped), (State::Done, 1), "{it:?}");
    c.click_row(Side::Local, "relatório final.txt, Arquivo");
    c.click("⬆ Enviar");
    c.wait_panel("conflito 2", 15, |p| p.needs_modal());
    c.click("Sobrescrever");
    c.wait_idle("sobrescrever", 15);
    c.wait_panel("tamanho novo", 10, |p| {
        p.remote_pane
            .entries
            .iter()
            .any(|e| e.name == "relatório final.txt" && e.size == 22)
    });

    // 5. arquivo grande: cancelar no meio remove o temporário; mede o frame com fila ativa
    let big = std::fs::File::create(u.join("grande.bin")).unwrap();
    big.set_len(768 * 1024 * 1024).unwrap();
    drop(big);
    fpm(&mut c.h).refresh(Side::Local);
    c.wait_panel("grande listado", 10, |p| {
        p.local.entries.iter().any(|e| e.name == "grande.bin")
    });
    c.clear_queue();
    c.click_row(Side::Local, "grande.bin, Arquivo");
    c.click("⬆ Enviar");
    let idx = fp(c.h.state()).queue.items.len() - 1;
    let t0 = Instant::now();
    let mut ms = Vec::new();
    while fp(c.h.state()).queue.items[idx].bytes_done < 64 * 1024 * 1024 {
        assert!(
            t0.elapsed() < Duration::from_secs(30),
            "o upload não avançou"
        );
        let f = Instant::now();
        c.h.step();
        ms.push(f.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(5));
    }
    c.shot("m3c-02-fila-upload-grande");
    let (_, bytes_total) = {
        let it = &fp(c.h.state()).queue.items[idx];
        (it.bytes_done, it.bytes_total)
    };
    assert_eq!(bytes_total, 768 * 1024 * 1024);
    c.click("Cancelar");
    c.wait_idle("cancelamento", 20);
    assert_eq!(c.state_of(idx), State::Cancelled);
    // o remoto não tem o arquivo nem o temporário (mostra ocultos para ver `.x.myssh-part-*`)
    c.h.get_all_by_label("Ocultos").nth(1).unwrap().click();
    c.h.run_steps(2);
    fpm(&mut c.h).refresh(Side::Remote);
    c.h.run_steps(5);
    std::thread::sleep(Duration::from_millis(300));
    fpm(&mut c.h).refresh(Side::Remote);
    c.wait_panel("remoto recarregado", 10, |p| !p.remote_pane.loading);
    let names = c.remote_names();
    assert!(
        !names
            .iter()
            .any(|n| n.contains("grande") || n.contains("myssh-part")),
        "sobrou lixo remoto: {names:?}"
    );
    let mut all = ms;
    all.extend(&c.frame_ms);
    let p95 = percentile(&mut all.clone(), 0.95);
    let max = all.iter().cloned().fold(0.0, f64::max);
    eprintln!(
        "[medida] frame com fila ativa: n={} p50={:.1} ms p95={:.1} ms máx={:.1} ms",
        all.len(),
        percentile(&mut all, 0.5),
        p95,
        max
    );
    assert!(p95 < 100.0, "a UI travou com a fila ativa: p95={p95:.1} ms");

    // 6. disco cheio (tmpfs de 2 MiB): o erro aparece claro na fila
    let cheio = std::fs::File::create(u.join("cheio.bin")).unwrap();
    cheio.set_len(4 * 1024 * 1024).unwrap();
    drop(cheio);
    fpm(&mut c.h).refresh(Side::Local);
    c.wait_panel("cheio listado", 10, |p| {
        p.local.entries.iter().any(|e| e.name == "cheio.bin")
    });
    c.nav(Side::Remote, &format!("{REMOTE_HOME}/small"));
    c.clear_queue();
    c.click_row(Side::Local, "cheio.bin, Arquivo");
    c.click("⬆ Enviar");
    c.wait_idle("disco cheio", 30);
    let idx = fp(c.h.state()).queue.items.len() - 1;
    let it = fp(c.h.state()).queue.items[idx].clone();
    assert_eq!(it.state, State::Failed, "{it:?}");
    assert!(
        it.errors.iter().any(|(_, m)| m.contains("espaço")),
        "mensagem de disco cheio não apareceu: {it:?}"
    );
    c.h.get_by_label_contains("Sem espaço");
    c.shot("m3c-04-disco-cheio");
    c.wait_panel("small sem sobras", 10, |p| {
        !p.remote_pane.loading
            && !p
                .remote_pane
                .entries
                .iter()
                .any(|e| e.name.contains("cheio"))
    });

    // 7. excluir remoto com confirmação
    c.nav(Side::Remote, REMOTE_HOME);
    c.click_row(Side::Remote, "m3c-ui, Pasta");
    c.h.key_press(Key::Delete);
    c.h.run_steps(3);
    c.h.get_by_label_contains("Excluir 1 item (remota)");
    c.shot("m3c-05-confirmar-exclusao");
    c.click_dialog("Excluir"); // sem "entendo" não exclui
    assert!(c.remote_names().contains(&"m3c-ui".to_string()));
    c.h.get_by_label_contains("Entendo que isto exclui").click();
    c.h.run_steps(2);
    c.click_dialog("Excluir");
    c.wait_panel("exclusão", 15, |p| {
        !p.needs_modal() && !p.remote_pane.entries.iter().any(|e| e.name == "m3c-ui")
    });
}
