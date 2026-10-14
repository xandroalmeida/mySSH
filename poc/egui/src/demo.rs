//! Modo de demonstração automatizado (`MYSSH_POC_DEMO=1`) para provar a GUI real sem
//! acessibilidade/automação do SO: injeta eventos egui reais (Text/Key), pede screenshots
//! via `ViewportCommand::Screenshot` e valida o conteúdo da grade.
//!
//! Variáveis: MYSSH_POC_HOST, MYSSH_POC_PORT, MYSSH_POC_USER, MYSSH_POC_KEY,
//! MYSSH_POC_FINGERPRINT (esperado; sem ele o diálogo é capturado e rejeitado),
//! MYSSH_POC_OUT (diretório dos PNG), MYSSH_POC_DATA_DIR (known_hosts isolado).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};

use crate::terminal::{Status, TermSession};

pub static DEMO_FAILED: AtomicBool = AtomicBool::new(false);

pub struct DemoConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: String,
    pub expected_fp: Option<String>,
    pub out_dir: PathBuf,
}

enum Op {
    WaitConnected,
    WaitQuiet(u64),
    Sleep(u64),
    Type(&'static str),
    Enter,
    Shot(&'static str),
    WaitAlt(bool),
    SetSize(f32, f32),
    WaitGridChange,
    MarkGrid,
    Check(&'static str, fn(&TermSession, &DemoState) -> bool),
    /// Fica ocioso (sem pedir repaint) para medir CPU/RSS externamente.
    Idle(u64),
    Finish,
}

#[derive(Default)]
pub struct DemoState {
    grid_before: (u16, u16),
    stty_before: Option<(u16, u16)>,
}

pub struct Demo {
    pub cfg: DemoConfig,
    ops: Vec<Op>,
    pc: usize,
    started: Instant,
    op_started: Instant,
    last_bytes: u64,
    last_change: Instant,
    shot_requested: bool,
    shot_saved: bool,
    shot_name: Option<&'static str>,
    dialog_shot: Option<bool>, // Some(false)=pedido, Some(true)=salvo
    dialog_seen: Option<Instant>,
    idle_until: Option<Instant>,
    state: DemoState,
    results: Vec<(String, bool)>,
}

fn log(msg: impl AsRef<str>) {
    println!("[demo] {}", msg.as_ref());
}

/// Último `R C` numérico na tela (saída de `stty size`).
fn last_stty(sess: &TermSession) -> Option<(u16, u16)> {
    sess.screen_lines().iter().rev().find_map(|l| {
        let mut it = l.split_whitespace();
        let (a, b, c) = (it.next()?, it.next()?, it.next());
        if c.is_some() {
            return None;
        }
        Some((a.parse().ok()?, b.parse().ok()?))
    })
}

impl Demo {
    pub fn from_env() -> Option<Self> {
        if std::env::var("MYSSH_POC_DEMO").ok().as_deref() != Some("1") {
            return None;
        }
        let get = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        let cfg = DemoConfig {
            host: get("MYSSH_POC_HOST", "127.0.0.1"),
            port: get("MYSSH_POC_PORT", "2222").parse().unwrap_or(2222),
            user: get("MYSSH_POC_USER", "tester"),
            key_path: get("MYSSH_POC_KEY", ""),
            expected_fp: std::env::var("MYSSH_POC_FINGERPRINT")
                .ok()
                .filter(|s| !s.is_empty()),
            out_dir: PathBuf::from(get("MYSSH_POC_OUT", "evidence")),
        };
        let _ = std::fs::create_dir_all(&cfg.out_dir);
        let ops = vec![
            Op::WaitConnected,
            Op::WaitQuiet(800),
            Op::Type("echo acentos: á é ç ã"),
            Op::Enter,
            Op::WaitQuiet(500),
            Op::Check(
                "texto Unicode com acento (Event::Text) eco do shell",
                |s, _| {
                    s.screen_lines()
                        .iter()
                        .filter(|l| l.contains("acentos: á é ç ã"))
                        .count()
                        >= 2
                },
            ),
            Op::Type("stty size"),
            Op::Enter,
            Op::WaitQuiet(500),
            Op::MarkGrid,
            Op::Shot("02-shell-stty-antes"),
            Op::Type("vim"),
            Op::Enter,
            Op::WaitAlt(true),
            Op::Sleep(1000),
            Op::Check("vim em tela alternativa mostrando '~'", |s, _| {
                s.is_alt_screen()
                    && s.screen_lines()
                        .iter()
                        .filter(|l| l.trim_start().starts_with('~'))
                        .count()
                        >= 5
            }),
            Op::Shot("03-vim"),
            Op::Type(":q"),
            Op::Enter,
            Op::WaitAlt(false),
            Op::WaitQuiet(400),
            Op::Type("top"),
            Op::Enter,
            Op::Sleep(2000),
            Op::Check("top mostra texto esperado", |s, _| {
                let t = s.screen_text().to_lowercase();
                t.contains("load") || t.contains("mem") || t.contains("tasks")
            }),
            Op::Shot("04-top"),
            Op::Type("q"),
            Op::Sleep(800),
            Op::SetSize(1280.0, 800.0),
            Op::WaitGridChange,
            Op::WaitQuiet(600),
            Op::Type("stty size"),
            Op::Enter,
            Op::WaitQuiet(700),
            Op::Check("stty size após resize == cols/rows do widget", |s, _| {
                last_stty(s) == Some((s.rows, s.cols))
            }),
            Op::Check("grade mudou de tamanho após resize", |s, st| {
                (s.cols, s.rows) != st.grid_before
            }),
            Op::Shot("05-stty-apos-resize"),
            Op::Idle(10),
            Op::Finish,
        ];
        Some(Self {
            cfg,
            ops,
            pc: 0,
            started: Instant::now(),
            op_started: Instant::now(),
            last_bytes: 0,
            last_change: Instant::now(),
            shot_requested: false,
            shot_saved: false,
            shot_name: None,
            dialog_shot: None,
            dialog_seen: None,
            idle_until: None,
            state: DemoState::default(),
            results: Vec::new(),
        })
    }

    fn fail(&mut self, ctx: &egui::Context, why: &str) {
        if DEMO_FAILED.swap(true, Ordering::SeqCst) {
            return;
        }
        log(format!("FALHA: {why}"));
        DEMO_FAILED.store(true, Ordering::SeqCst);
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    pub fn on_screenshot(&mut self, img: &egui::ColorImage) {
        let name = match (self.dialog_shot, self.shot_name) {
            (Some(false), _) => "01-dialogo-fingerprint",
            (_, Some(n)) => n,
            _ => return,
        };
        let path = self.cfg.out_dir.join(format!("{name}.png"));
        match save_png(&path, img) {
            Ok(()) => log(format!("screenshot salvo: {}", path.display())),
            Err(e) => {
                log(format!("erro ao salvar screenshot: {e}"));
                DEMO_FAILED.store(true, Ordering::SeqCst);
            }
        }
        if self.dialog_shot == Some(false) {
            self.dialog_shot = Some(true);
        } else {
            self.shot_saved = true;
        }
    }

    fn inject_text(ctx: &egui::Context, t: &str) {
        ctx.input_mut(|i| i.events.push(Event::Text(t.to_string())));
    }

    fn inject_key(ctx: &egui::Context, key: Key) {
        ctx.input_mut(|i| {
            i.events.push(Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
        });
    }

    pub fn tick(&mut self, ctx: &egui::Context, session: &mut Option<TermSession>) {
        if let Some(until) = self.idle_until {
            if Instant::now() < until {
                return; // ocioso de verdade: nenhum repaint pedido
            }
            self.idle_until = None;
            log("fim da janela ociosa");
            self.pc += 1;
        }
        ctx.request_repaint_after(Duration::from_millis(40));
        if self.started.elapsed() < Duration::from_secs(2) {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        if self.started.elapsed() > Duration::from_secs(120) {
            return self.fail(ctx, "tempo total excedido");
        }
        let Some(sess) = session.as_mut() else {
            return;
        };

        // Diálogo de host key: aceita SOMENTE se o fingerprint bater com o esperado.
        if sess.status == Status::AwaitingHostKey {
            let Some(p) = sess.prompt.as_ref() else {
                return;
            };
            let fp = p.fingerprint.clone();
            match &self.cfg.expected_fp {
                Some(exp) if *exp == fp => {
                    log(format!("fingerprint confere ({fp}); aceitando"));
                    sess.answer_host_key(true);
                }
                Some(exp) => {
                    log(format!(
                        "fingerprint {fp} DIFERE do esperado {exp}; rejeitando"
                    ));
                    sess.answer_host_key(false);
                    DEMO_FAILED.store(true, Ordering::SeqCst);
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                None => match self.dialog_shot {
                    None if self
                        .dialog_seen
                        .is_none_or(|t| t.elapsed() < Duration::from_millis(700)) =>
                    {
                        // Espera a animação de entrada do modal terminar antes de capturar.
                        self.dialog_seen.get_or_insert_with(Instant::now);
                    }
                    None => {
                        log(format!("diálogo exibido (fingerprint {fp}); capturando"));
                        ctx.send_viewport_cmd(ViewportCommand::Screenshot(
                            egui::UserData::default(),
                        ));
                        self.dialog_shot = Some(false);
                    }
                    Some(true) => {
                        log("sem fingerprint esperado: rejeitando após captura");
                        sess.answer_host_key(false);
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    Some(false) => {}
                },
            }
            return;
        }
        if let Status::Closed(e) = &sess.status {
            if self.dialog_shot == Some(true) {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            let why = format!("sessão encerrada: {e:?}");
            return self.fail(ctx, &why);
        }

        if sess.bytes_received != self.last_bytes {
            self.last_bytes = sess.bytes_received;
            self.last_change = Instant::now();
        }
        let waited = self.op_started.elapsed();
        if waited > Duration::from_secs(30) {
            return self.fail(ctx, &format!("op #{} excedeu 30s", self.pc));
        }

        let Some(op) = self.ops.get(self.pc) else {
            return;
        };
        let advance = match op {
            Op::WaitConnected => sess.status == Status::Connected,
            Op::WaitQuiet(ms) => {
                sess.bytes_received > 0 && self.last_change.elapsed() >= Duration::from_millis(*ms)
            }
            Op::Sleep(ms) => waited >= Duration::from_millis(*ms),
            Op::Type(t) => {
                Self::inject_text(ctx, t);
                true
            }
            Op::Enter => {
                Self::inject_key(ctx, Key::Enter);
                true
            }
            Op::Shot(name) => {
                if !self.shot_requested {
                    self.shot_requested = true;
                    self.shot_saved = false;
                    self.shot_name = Some(name);
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                }
                if self.shot_saved {
                    self.shot_requested = false;
                    true
                } else {
                    false
                }
            }
            Op::WaitAlt(want) => sess.is_alt_screen() == *want,
            Op::SetSize(w, h) => {
                self.state.grid_before = (sess.cols, sess.rows);
                log(format!(
                    "redimensionando janela para {w}x{h}; grade antes: {}x{}",
                    sess.cols, sess.rows
                ));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(*w, *h)));
                true
            }
            Op::WaitGridChange => (sess.cols, sess.rows) != self.state.grid_before,
            Op::MarkGrid => {
                self.state.stty_before = last_stty(sess);
                log(format!(
                    "grade inicial {}x{} (cols x rows); stty size inicial = {:?}",
                    sess.cols, sess.rows, self.state.stty_before
                ));
                true
            }
            Op::Check(name, f) => {
                let ok = f(sess, &self.state);
                log(format!(
                    "CHECK [{}] {name}",
                    if ok { "OK" } else { "FALHOU" }
                ));
                if !ok {
                    log(format!(
                        "--- tela ---\n{}\n------------",
                        sess.screen_text()
                    ));
                }
                let name = name.to_string();
                self.results.push((name, ok));
                true
            }
            Op::Idle(secs) => {
                log(format!(
                    "início da janela ociosa de {secs}s (pid {})",
                    std::process::id()
                ));
                let d = Duration::from_secs(*secs);
                self.idle_until = Some(Instant::now() + d);
                ctx.request_repaint_after(d + Duration::from_millis(200));
                false
            }
            Op::Finish => {
                let ok = self.results.iter().all(|(_, o)| *o);
                log(format!(
                    "RESULTADO: {}/{} checks OK; bytes recebidos={}; grade final {}x{}",
                    self.results.iter().filter(|(_, o)| *o).count(),
                    self.results.len(),
                    sess.bytes_received,
                    sess.cols,
                    sess.rows
                ));
                if !ok {
                    DEMO_FAILED.store(true, Ordering::SeqCst);
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
                true
            }
        };
        if advance {
            self.pc += 1;
            self.op_started = Instant::now();
            // Dá um quadro para os eventos injetados serem processados antes do próximo passo.
            ctx.request_repaint();
        }
    }
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> Result<(), String> {
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
