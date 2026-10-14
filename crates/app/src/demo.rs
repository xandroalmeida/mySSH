//! Modo demo (`MYSSH_DEMO_SHOTS=dir`): cria perfis pelo mesmo caminho da UI, percorre telas e
//! salva screenshots **somente da janela do app** via `ViewportCommand::Screenshot`.
//!
//! Exige `MYSSH_DATA_DIR` (dados isolados); com isso o cofre é só em memória, então nada
//! é gravado no Keychain/Credential Manager/Secret Service reais.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::ViewportCommand;
use myssh_core::Environment;

use crate::app::{App, Dialog, KeyringStatus, ProfileDialog, QuickDialog};
use crate::form::{AuthKind, ProfileForm, SecretMode};
use crate::settings::ThemePref;

pub static FAILED: AtomicBool = AtomicBool::new(false);

pub(crate) type SeedFn = fn(&App) -> ProfileForm;
pub(crate) type ActFn = fn(&mut App, &egui::Context);

enum Phase {
    Seed,
    Act,
    Settle(u32),
    WaitShot(Instant),
}

pub struct Demo {
    dir: PathBuf,
    step: usize,
    phase: Phase,
    seeds: VecDeque<SeedFn>,
}

fn log(msg: impl AsRef<str>) {
    eprintln!("[demo] {}", msg.as_ref());
}

fn base(name: &str, host: &str, user: &str) -> ProfileForm {
    let mut f = ProfileForm::blank();
    f.name = name.into();
    f.host = host.into();
    f.username = user.into();
    f
}

pub(crate) fn id_of(app: &App, name: &str) -> Option<myssh_core::ProfileId> {
    app.profiles.iter().find(|p| p.name == name).map(|p| p.id)
}

pub(crate) fn seeds() -> VecDeque<SeedFn> {
    let list: [SeedFn; 6] = [
        |_| {
            let mut f = base("bastion", "bastion.empresa.com.br", "ops");
            f.auth = AuthKind::Agent;
            f.new_folder = "Infra".into();
            f.tags = "infra, acesso".into();
            f
        },
        |app| {
            let mut f = base("web-prod-01", "web01.empresa.com.br", "deploy");
            f.auth = AuthKind::Key;
            f.key_path = "/Users/demo/.ssh/id_ed25519".into();
            f.passphrase_mode = SecretMode::Save;
            f.passphrase = "frase-demo".into();
            f.environment = Environment::Production;
            f.favorite = true;
            f.new_folder = "Produção".into();
            f.tags = "web, producao".into();
            f.jump = id_of(app, "bastion");
            f
        },
        |app| {
            let mut f = base("db-prod", "10.20.0.15", "postgres");
            f.password_mode = SecretMode::Save;
            f.password = "senha-demo-123".into();
            f.environment = Environment::Production;
            f.new_folder = "Produção".into();
            f.tags = "db".into();
            f.jump = id_of(app, "bastion");
            f
        },
        |_| {
            let mut f = base("web-hml", "web.hml.empresa.com.br", "deploy");
            f.environment = Environment::Staging;
            f.new_folder = "Homologação".into();
            f.tags = "web".into();
            f
        },
        |_| {
            let mut f = base("dev-local", "127.0.0.1", "tester");
            f.port = "2222".into();
            f.auth = AuthKind::KeyboardInteractive;
            f.environment = Environment::Development;
            f.favorite = true;
            f.new_folder = "Laboratório".into();
            f
        },
        |_| {
            let mut f = base("raspberry", "192.168.0.50", "pi");
            f.password_mode = SecretMode::Save;
            f.password = "framboesa".into();
            f
        },
    ];
    list.into()
}

pub(crate) const STEPS: &[(Option<&str>, ActFn)] = &[
    (Some("01-lista-perfis"), |app, _| {
        app.selected = id_of(app, "web-prod-01");
    }),
    (Some("02-aba-producao"), |app, _| {
        if let Some(id) = id_of(app, "web-prod-01") {
            app.open_profile(id);
        }
    }),
    (Some("03-aba-homologacao"), |app, _| {
        if let Some(id) = id_of(app, "web-hml") {
            app.open_profile(id);
        }
    }),
    (Some("04-aba-dev"), |app, _| {
        if let Some(id) = id_of(app, "dev-local") {
            app.open_profile(id);
        }
    }),
    (Some("05-busca"), |app, _| {
        app.filter.search = "db".into();
    }),
    (Some("06-editar-perfil"), |app, _| {
        app.filter = Default::default();
        if let Some(id) = id_of(app, "web-prod-01") {
            app.edit_profile_dialog(id);
        }
    }),
    (Some("07-novo-perfil-validacao"), |app, ctx| {
        app.close_dialog();
        let mut f = base("", "srv.exemplo.com", "ana");
        f.password_mode = SecretMode::Save;
        f.password = "segredo".into();
        f.tags = "teste".into();
        app.dialog = Dialog::Profile(Box::new(ProfileDialog::new(f)));
        app.submit_profile_dialog(ctx);
    }),
    (Some("08-configuracoes-escuro"), |app, _| {
        app.close_dialog();
        app.settings.theme = ThemePref::Dark;
        app.dialog = Dialog::Settings;
    }),
    (Some("09-configuracoes-claro"), |app, _| {
        app.settings.theme = ThemePref::Light;
    }),
    (Some("10-conexao-rapida"), |app, _| {
        app.close_dialog();
        app.dialog = Dialog::Quick(QuickDialog {
            text: "ana@10.0.0.9:2222".into(),
            error: None,
        });
    }),
    (Some("11-confirmar-fechar-aba"), |app, _| {
        app.close_dialog();
        app.settings.theme = ThemePref::Dark;
        if let Some(t) = app.tabs.iter_mut().find(|t| Some(t.id) == app.active) {
            t.body.simulate_busy(true);
        }
        if let Some(id) = app.active {
            app.request_close_tab(id);
        }
    }),
    (Some("12-aviso-cofre-indisponivel"), |app, _| {
        app.close_dialog();
        app.keyring = KeyringStatus::Unavailable("nenhum cofre do sistema inicializado".into());
    }),
];

impl Demo {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("MYSSH_DEMO_SHOTS").filter(|d| !d.is_empty())?);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[demo] não foi possível criar {}: {e}", dir.display());
            std::process::exit(2);
        }
        Some(Self {
            dir,
            step: 0,
            phase: Phase::Seed,
            seeds: seeds(),
        })
    }

    fn fail(&self, ctx: &egui::Context, why: &str) {
        log(format!("FALHA: {why}"));
        FAILED.store(true, Ordering::SeqCst);
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    pub fn tick(&mut self, app: &mut App, ctx: &egui::Context) {
        ctx.request_repaint();
        match self.phase {
            Phase::Seed => {
                if let Dialog::Profile(d) = &app.dialog {
                    if let Some(e) = &d.error {
                        return self.fail(ctx, &format!("erro ao criar perfil de teste: {e}"));
                    }
                    return;
                }
                if !app.is_idle() {
                    return;
                }
                match self.seeds.pop_front() {
                    Some(f) => {
                        let form = f(app);
                        app.dialog = Dialog::Profile(Box::new(ProfileDialog::new(form)));
                        app.submit_profile_dialog(ctx);
                    }
                    None => {
                        log(format!("{} perfis criados", app.profiles.len()));
                        self.phase = Phase::Act;
                    }
                }
            }
            Phase::Act => match STEPS.get(self.step) {
                None => {
                    log("concluído");
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                Some((_, act)) => {
                    act(app, ctx);
                    self.phase = Phase::Settle(14);
                }
            },
            Phase::Settle(n) if n > 1 => self.phase = Phase::Settle(n - 1),
            Phase::Settle(_) => {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                self.phase = Phase::WaitShot(Instant::now());
            }
            Phase::WaitShot(since) => {
                let img = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = img {
                    let name = STEPS[self.step].0.unwrap_or("shot");
                    let path = self.dir.join(format!("{name}.png"));
                    match save_png(&path, &img) {
                        Ok(()) => log(format!("screenshot: {}", path.display())),
                        Err(e) => return self.fail(ctx, &format!("salvar png: {e}")),
                    }
                    self.step += 1;
                    self.phase = Phase::Act;
                } else if since.elapsed() > Duration::from_secs(8) {
                    self.fail(ctx, "screenshot não chegou");
                }
            }
        }
    }
}

fn save_png(path: &Path, img: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        img.width() as u32,
        img.height() as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
    w.write_image_data(&data).map_err(|e| e.to_string())
}
