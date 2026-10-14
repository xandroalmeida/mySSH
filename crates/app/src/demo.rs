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
static FAIL_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(crate) type SeedFn = fn(&App) -> ProfileForm;
pub(crate) type ActFn = fn(&mut App, &egui::Context);

enum Phase {
    Seed,
    Act,
    Settle(u32),
    WaitShot(Instant),
    /// Janela ociosa: sem repaint (mede CPU ociosa com sessão conectada).
    Idle(Instant),
}

pub struct Demo {
    dir: PathBuf,
    step: usize,
    phase: Phase,
    seeds: VecDeque<SeedFn>,
    steps: &'static [(Option<&'static str>, ActFn)],
    /// Quadros de espera antes de cada captura (conexões reais precisam de mais tempo).
    settle: u32,
    /// Modo SSH real: depois da última captura, fica ocioso (sem repaint) por alguns segundos.
    idle_after: Option<Duration>,
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

/// Modo `MYSSH_DEMO_SSH=host:porta` + `MYSSH_DEMO_KEY=caminho` (chave sem passphrase): cria um
/// perfil para o servidor de teste, mostra o diálogo de host desconhecido e conecta de verdade.
fn ssh_seed(_: &App) -> ProfileForm {
    let target = std::env::var("MYSSH_DEMO_SSH").unwrap_or_default();
    let (host, port) = target.rsplit_once(':').unwrap_or(("127.0.0.1", "2222"));
    let mut f = base("servidor-de-teste", host, "tester");
    f.port = port.into();
    f.auth = AuthKind::Key;
    f.key_path = std::env::var("MYSSH_DEMO_KEY").unwrap_or_default();
    f.environment = Environment::Development;
    f
}

fn open_ssh(app: &mut App, _: &egui::Context) {
    if let Some(id) = id_of(app, "servidor-de-teste") {
        app.open_profile(id);
    }
}

fn accept_ssh(app: &mut App, _: &egui::Context) {
    for t in &mut app.tabs {
        t.body.demo_accept_host_key();
    }
}

/// Abre mais 9 abas do mesmo perfil (a chave do host já foi salva no passo anterior).
fn open_more(app: &mut App, _: &egui::Context) {
    if let Some(id) = id_of(app, "servidor-de-teste") {
        for _ in 0..9 {
            app.open_profile(id);
        }
    }
}

fn open_files(app: &mut App, _: &egui::Context) {
    if let Some(t) = app.tabs.last_mut() {
        app.active = Some(t.id);
        t.body.toggle_files();
    }
}

pub(crate) const SSH_STEPS: &[(Option<&str>, ActFn)] = &[
    (Some("30-ssh-host-desconhecido"), open_ssh),
    (Some("31-ssh-conectado"), accept_ssh),
    (Some("32-dez-abas"), open_more),
    (Some("33-painel-de-arquivos"), open_files),
];

impl Demo {
    pub fn from_env(data_dir_overridden: bool) -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("MYSSH_DEMO_SHOTS").filter(|d| !d.is_empty())?);
        if !data_dir_overridden {
            eprintln!("MYSSH_DEMO_SHOTS exige MYSSH_DATA_DIR (para não tocar nos dados reais).");
            std::process::exit(2);
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[demo] não foi possível criar {}: {e}", dir.display());
            std::process::exit(2);
        }
        let ssh = std::env::var_os("MYSSH_DEMO_SSH").is_some();
        Some(Self {
            dir,
            step: 0,
            phase: Phase::Seed,
            seeds: if ssh {
                VecDeque::from([ssh_seed as SeedFn])
            } else {
                seeds()
            },
            steps: if ssh { SSH_STEPS } else { STEPS },
            settle: if ssh { 60 } else { 14 },
            idle_after: ssh.then_some(Duration::from_secs(12)),
        })
    }

    /// O modo SSH conecta de verdade (as demais capturas não abrem conexões).
    pub fn connects(&self) -> bool {
        self.idle_after.is_some()
    }

    fn fail(&self, ctx: &egui::Context, why: &str) {
        log(format!("FALHA: {why}"));
        FAILED.store(true, Ordering::SeqCst);
        ctx.send_viewport_cmd(ViewportCommand::Close);
        // Com a tela do Mac bloqueada a janela não pinta e o `Close` nunca é processado:
        // encerra de vez em vez de ficar em laço (e gastando CPU).
        if FAIL_COUNT.fetch_add(1, Ordering::SeqCst) >= 2 {
            eprintln!("[demo] a janela não pintou (tela bloqueada?): abortando");
            std::process::exit(1);
        }
    }

    pub fn tick(&mut self, app: &mut App, ctx: &egui::Context) {
        if let Phase::Idle(since) = self.phase {
            let total = self.idle_after.unwrap_or_default();
            if since.elapsed() >= total {
                log("ocioso: fim");
                // Sessões ativas pedem confirmação ao fechar: a demo confirma sozinha.
                app.confirm_quit(ctx);
            } else {
                ctx.request_repaint_after(total - since.elapsed());
            }
            return;
        }
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
            Phase::Act => match self.steps.get(self.step) {
                None => match self.idle_after {
                    Some(_) => {
                        log("capturas concluídas; janela ociosa (sem repaint)");
                        self.phase = Phase::Idle(Instant::now());
                    }
                    None => {
                        log("concluído");
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                },
                Some((_, act)) => {
                    act(app, ctx);
                    self.phase = Phase::Settle(self.settle);
                }
            },
            Phase::Settle(n) if n > 1 => self.phase = Phase::Settle(n - 1),
            Phase::Settle(_) => {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                self.phase = Phase::WaitShot(Instant::now());
            }
            Phase::Idle(_) => {}
            Phase::WaitShot(since) => {
                let img = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = img {
                    let name = self.steps[self.step].0.unwrap_or("shot");
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
