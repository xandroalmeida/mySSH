//! Tema por painel (sonda) e renders offscreen do M2-B.
//!
//! `MYSSH_DEMO_SHOTS=dir cargo test -p myssh-app m2b_screenshots -- --ignored`

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use myssh_core::{Environment, Profile};
use myssh_term::TermTheme;

use crate::app::{App, Dialog, ImportStage};
use crate::split::Dir;
use crate::split_body::SplitBody;
use crate::tabs::{Tab, TabBody, TabEnv, TabKind};
use crate::theming::{self, Resolved};
use crate::ui_tests::{harness, memory_app, wait_until};

type Seen = Arc<Mutex<Vec<(Option<TermTheme>, f32)>>>;

/// Corpo de aba de teste: desenha uma amostra com o tema que recebeu em `TabEnv`
/// (o mesmo que o terminal real recebe) e registra o que viu.
struct Probe {
    title: String,
    seen: Seen,
}

impl TabBody for Probe {
    fn title(&self) -> String {
        self.title.clone()
    }
    fn ui(&mut self, ui: &mut egui::Ui, env: &TabEnv) {
        self.seen
            .lock()
            .unwrap()
            .push((env.theme.clone(), env.font_size));
        let term = env
            .theme
            .clone()
            .unwrap_or_else(|| theming::global_term_theme(env.dark));
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, term.bg);
        theming::preview(
            ui,
            &Resolved {
                term,
                font_size: env.font_size,
                accent: None,
                palette_overridden: false,
            },
        );
        ui.allocate_space(ui.available_size());
    }
    fn is_busy(&self) -> bool {
        false
    }
    fn close(&mut self) {}
}

fn themed(name: &str, palette: Option<&str>, accent: Option<[u8; 3]>, env: Environment) -> Profile {
    let mut p = Profile::new(name, format!("{name}.example.com"), "ana");
    p.theme.palette = palette.map(str::to_string);
    p.theme.accent = accent;
    p.environment = env;
    p
}

fn kind(p: &Profile) -> TabKind {
    TabKind::Terminal {
        profile: Box::new(p.clone()),
        saved: false,
    }
}

fn probe(p: &Profile, seen: &Seen) -> Box<dyn TabBody> {
    Box::new(Probe {
        title: p.name.clone(),
        seen: seen.clone(),
    })
}

fn probe_tab(app: &mut App, id: u64, p: &Profile, seen: &Seen) {
    app.tabs.push(Tab {
        id,
        kind: kind(p),
        body: probe(p, seen),
    });
    app.active = Some(id);
}

fn split_tab(app: &mut App, id: u64, panes: &[(&Profile, Dir)], seen: &Seen) {
    let (first, _) = panes[0];
    let mut sp = SplitBody::new(kind(first), probe(first, seen));
    for (p, dir) in &panes[1..] {
        sp.split(*dir, kind(p), probe(p, seen));
    }
    app.tabs.push(Tab {
        id,
        kind: kind(first),
        body: Box::new(sp),
    });
    app.active = Some(id);
}

#[test]
fn cada_painel_recebe_o_proprio_tema() {
    let mut h = harness(memory_app());
    h.state_mut().settings.theme = crate::settings::ThemePref::Dark;
    let seen: Seen = Arc::default();
    let neon = themed(
        "neon",
        Some("cyberpunk"),
        Some([255, 43, 214]),
        Environment::None,
    );
    let mut big = themed("grande", None, None, Environment::None);
    big.theme.font_size_tenths = Some(200);
    let plain = themed("plain", None, None, Environment::None);
    split_tab(
        h.state_mut(),
        900,
        &[
            (&neon, Dir::Vertical),
            (&big, Dir::Vertical),
            (&plain, Dir::Horizontal),
        ],
        &seen,
    );
    h.run_steps(4);
    let seen = seen.lock().unwrap();
    // cada painel foi desenhado com a própria paleta/fonte (e o global onde herda)
    assert!(seen.contains(&(Some(TermTheme::cyberpunk()), 14.0)));
    assert!(seen.contains(&(None, 20.0)));
    assert!(seen.contains(&(None, 14.0)));
}

fn shot(h: &mut Harness<'static, App>, dir: &std::path::Path, name: &str) {
    h.run_steps(8);
    let img = h.render().expect("render");
    let path = dir.join(format!("{name}.png"));
    img.save(&path).unwrap();
    eprintln!("screenshot: {}", path.display());
}

const SSH_CONFIG: &str = "Host *
  ForwardAgent yes
  ServerAliveInterval 30

Host bastion
  HostName bastion.empresa.com.br
  User ops
  IdentityFile ~/.ssh/id_bastion

Host web-prod
  HostName web01.empresa.com.br
  User deploy
  Port 2200
  ProxyJump bastion

Host legado
  HostName 10.0.0.9
  User root
  ProxyCommand ssh -W %h:%p gw

Host db-solarized
  HostName db-solarized.example.com
  User ana

Host nuvem
  HostName nuvem.empresa.com.br
  User ec2-user
  PermitLocalCommand yes
  LocalCommand notify-send conectado

Match host *.interno exec \"test -f /tmp/x\"
  User outro
";

#[test]
#[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn m2b_screenshots() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS").map(std::path::PathBuf::from) else {
        eprintln!("MYSSH_DEMO_SHOTS não definido: nada a fazer");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_eframe(|_| memory_app());
    h.state_mut().settings.theme = crate::settings::ThemePref::Dark;
    h.run_steps(3);
    let seen: Seen = Arc::default();

    let neon = themed(
        "web-neon",
        Some("cyberpunk"),
        Some([255, 43, 214]),
        Environment::Development,
    );
    let sol = themed(
        "db-solarized",
        Some("solarized-dark"),
        Some([38, 139, 210]),
        Environment::Staging,
    );
    let prod = themed("api-prod", Some("dracula"), None, Environment::Production);
    for p in [&neon, &sol, &prod] {
        h.state().store.create_profile(p).unwrap();
    }
    h.state_mut().reload();
    h.state_mut().selected = Some(neon.id);

    // 1. aba com tema Cyberpunk
    probe_tab(h.state_mut(), 901, &neon, &seen);
    shot(&mut h, &dir, "m2b-01-aba-cyberpunk");

    // 2. editor do tema no diálogo do perfil
    h.state_mut().edit_profile_dialog(neon.id);
    h.run_steps(3);
    h.get_by_label_contains("Tema deste perfil").click();
    shot(&mut h, &dir, "m2b-02-editor-tema");
    h.state_mut().close_dialog();

    // 3. painéis divididos (um de cada perfil; um de produção)
    split_tab(
        h.state_mut(),
        902,
        &[
            (&neon, Dir::Vertical),
            (&sol, Dir::Vertical),
            (&prod, Dir::Horizontal),
        ],
        &seen,
    );
    shot(&mut h, &dir, "m2b-03-paineis-divididos");

    // 4. prévia da importação do ssh config (arquivo sintético)
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("config");
    std::fs::write(&cfg, SSH_CONFIG).unwrap();
    h.state_mut().open_import_ssh();
    let ctx = h.ctx.clone();
    h.state_mut().start_import_load(&ctx, Some(cfg));
    wait_until(
        &mut h,
        "prévia",
        |a| matches!(&a.dialog, Dialog::ImportSsh(d) if d.stage == ImportStage::Preview),
    );
    shot(&mut h, &dir, "m2b-04-importacao-previa");
    h.state_mut().close_dialog();

    // 5. cofre bloqueado na barra + configurações (exportar/importar)
    h.state_mut().lock_vault();
    h.state_mut().dialog = Dialog::Settings;
    shot(&mut h, &dir, "m2b-05-configuracoes-cofre");
}
