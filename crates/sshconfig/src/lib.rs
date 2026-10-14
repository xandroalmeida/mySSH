//! Importação limitada e segura de `~/.ssh/config` (ref.: man.openbsd.org/ssh_config).
//!
//! Crate puro: só lê (nunca escreve em arquivos do usuário), não usa rede e
//! **nunca executa comandos** — `ProxyCommand`, `LocalCommand`, `PermitLocalCommand`,
//! `KnownHostsCommand` e `Match exec` geram aviso e são ignorados.
//!
//! # Subconjunto suportado
//! - `Host` (vários nomes por bloco). Só blocos **sem** `*`, `?` ou `!` viram perfis;
//!   blocos com curinga aplicam valores como padrão aos aliases que casarem
//!   (regra do ssh: o **primeiro** valor encontrado vence, na ordem do arquivo).
//! - `HostName` (aceita `%h`), `Port`, `User`, `IdentityFile`, `ProxyJump`,
//!   `ServerAliveInterval` (0 = sem keepalive), `ConnectTimeout`, `ForwardAgent`
//!   (sempre importado como desligado, com aviso), `Include` (relativo a `~/.ssh`,
//!   com glob `*`/`?`, profundidade máxima 8, ciclos ignorados), `Match` (bloco inteiro
//!   ignorado, com aviso).
//! - Sintaxe: `chave valor`, `chave=valor`, aspas duplas, comentários `#`, chaves
//!   case-insensitive.
//! - `IdentityFile`: `~`, `%d` (home), `%u` (usuário local), `%h`, `%r`, `%%`,
//!   `%USERPROFILE%`/`%HOME%` (estilo Windows); caminhos unix (`/`) e Windows
//!   (`C:\...`, `~\...`); relativos partem do home. Só o primeiro arquivo é usado.
//! - `ProxyJump`: um salto `[user@]host[:port]` (ou `ssh://...`, `[::1]:22`) ou o nome
//!   de outro `Host`. Salto literal vira um candidato extra. Vários saltos: só o
//!   último (o mais próximo do destino) é usado, com aviso.
//!
//! A ligação `jump_alias` → `ProfileId` é feita por [`resolve_jumps`].

mod fsys;
mod parser;
mod paths;

pub use fsys::{DiskFs, IncludeSource, MAX_FILE_BYTES, MemFs};
pub use parser::MAX_INCLUDE_DEPTH;

use myssh_core::{AuthMethod, Profile, ProfileId};
use parser::{Block, Directive};
use paths::{Tokens, expand_path, has_wildcard, join_home, wildcard_match};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

/// Aviso de importação (pt-BR). `line` começa em 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub line: usize,
    pub file: String,
    pub msg: String,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub alias: String,
    pub profile: Profile,
    /// Alias do candidato usado como salto; ligado a `profile.jump` por [`resolve_jumps`].
    pub jump_alias: Option<String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportPreview {
    pub candidates: Vec<Candidate>,
    pub global_warnings: Vec<Warning>,
}

/// Ambiente necessário para expandir caminhos.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Diretório do usuário (ex.: `/home/ana` ou `C:\Users\Ana`).
    pub home: String,
    /// Nome do usuário local (usado em `%u` e como `User` padrão).
    pub local_user: String,
    /// Nome do arquivo raiz, usado nos avisos e na detecção de ciclos.
    pub root_name: String,
}

impl ImportOptions {
    pub fn new(home: impl Into<String>, local_user: impl Into<String>) -> Self {
        let home = home.into();
        let root_name = join_home(&join_home(&home, ".ssh"), "config");
        Self {
            home,
            local_user: local_user.into(),
            root_name,
        }
    }
}

/// Importa a partir do conteúdo de `~/.ssh/config`; `source` serve os `Include`.
pub fn parse_config(
    content: &str,
    opts: &ImportOptions,
    source: &dyn IncludeSource,
) -> ImportPreview {
    let parsed = parser::parse(content, opts, source);
    build_preview(parsed.blocks, parsed.global_warnings, opts)
}

/// Lê `~/.ssh/config` (1 MiB no máximo por arquivo). Só leitura.
pub fn load_default() -> std::io::Result<ImportPreview> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("HOME")
        .or_else(|| env("USERPROFILE"))
        .ok_or_else(|| std::io::Error::other("diretório do usuário não encontrado"))?;
    let user = env("USER").or_else(|| env("USERNAME")).unwrap_or_default();
    let opts = ImportOptions::new(home, user);
    let content = fsys::read_limited(&opts.root_name)?;
    Ok(parse_config(&content, &opts, &DiskFs))
}

// ---------------------------------------------------------------------------

fn is_concrete(b: &Block) -> bool {
    !b.patterns.is_empty()
        && b.patterns
            .iter()
            .all(|p| !has_wildcard(p) && !p.starts_with('!'))
}

fn block_matches(b: &Block, alias: &str) -> bool {
    let mut positive = false;
    for p in &b.patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if wildcard_match(neg, alias) {
                return false;
            }
        } else if wildcard_match(p, alias) {
            positive = true;
        }
    }
    positive
}

struct Hop {
    user: Option<String>,
    host: String,
    port: Option<u16>,
}

fn parse_hop(tok: &str) -> Option<Hop> {
    let t = tok.strip_prefix("ssh://").unwrap_or(tok);
    let (user, hp) = match t.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() => (Some(u.to_string()), h),
        _ => (None, t),
    };
    let (host, port) = if let Some(rest) = hp.strip_prefix('[') {
        let (h, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p.parse::<u16>().ok().filter(|p| *p != 0)?),
            None if after.is_empty() => None,
            None => return None,
        };
        (h.to_string(), port)
    } else if let Some((h, p)) = hp.rsplit_once(':') {
        (
            h.to_string(),
            Some(p.parse::<u16>().ok().filter(|p| *p != 0)?),
        )
    } else {
        (hp.to_string(), None)
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    Some(Hop { user, host, port })
}

struct Job {
    alias: String,
    user: Option<String>,
    port: Option<u16>,
    note: Option<String>,
}

struct Ctx<'a> {
    blocks: &'a [Block],
    opts: &'a ImportOptions,
}

impl Ctx<'_> {
    fn first<'s>(&'s self, alias: &'s str, key: &'s str) -> Option<&'s Directive> {
        self.all(alias, key).next()
    }
    fn all<'s>(&'s self, alias: &'s str, key: &'s str) -> impl Iterator<Item = &'s Directive> {
        self.blocks
            .iter()
            .filter(move |b| block_matches(b, alias))
            .flat_map(|b| b.directives.iter())
            .filter(move |d| d.key == key)
    }
}

fn w(d: &Directive, msg: String) -> Warning {
    Warning {
        line: d.line,
        file: d.file.clone(),
        msg,
    }
}

fn build_preview(
    blocks: Vec<Block>,
    mut global_warnings: Vec<Warning>,
    opts: &ImportOptions,
) -> ImportPreview {
    let ctx = Ctx {
        blocks: &blocks,
        opts,
    };
    // Avisos de blocos que não geram perfis (global, curingas) vão para o global.
    let mut queue: VecDeque<Job> = VecDeque::new();
    let mut known: HashSet<String> = HashSet::new();
    let mut alias_warnings: HashMap<String, Vec<Warning>> = HashMap::new();
    for b in &blocks {
        if is_concrete(b) {
            for p in &b.patterns {
                if known.insert(p.clone()) {
                    queue.push_back(Job {
                        alias: p.clone(),
                        user: None,
                        port: None,
                        note: None,
                    });
                    alias_warnings.insert(p.clone(), b.warnings.clone());
                } else {
                    global_warnings.push(Warning {
                        line: 0,
                        file: opts.root_name.clone(),
                        msg: format!("Host \"{p}\" duplicado: usado apenas o primeiro bloco."),
                    });
                }
            }
        } else {
            global_warnings.extend(b.warnings.iter().cloned());
        }
    }

    let mut candidates = Vec::new();
    let mut synth: HashMap<String, String> = HashMap::new(); // token -> alias
    while let Some(job) = queue.pop_front() {
        let mut warnings = alias_warnings.remove(&job.alias).unwrap_or_default();
        if let Some(n) = &job.note {
            warnings.push(Warning {
                line: 0,
                file: opts.root_name.clone(),
                msg: n.clone(),
            });
        }
        let (mut cand, hop) = build_candidate(&ctx, &job, warnings);
        if let Some((tok, d)) = hop {
            match parse_hop(&tok) {
                None => cand
                    .warnings
                    .push(w(&d, format!("ProxyJump \"{tok}\" inválido: ignorado."))),
                Some(h) => {
                    let plain = h.user.is_none() && h.port.is_none();
                    let target = if plain && known.contains(&h.host) {
                        h.host.clone()
                    } else if let Some(a) = synth.get(&tok) {
                        a.clone()
                    } else {
                        let name = if known.contains(&h.host) {
                            tok.clone()
                        } else {
                            h.host.clone()
                        };
                        known.insert(name.clone());
                        synth.insert(tok.clone(), name.clone());
                        queue.push_back(Job {
                            alias: name.clone(),
                            user: h.user,
                            port: h.port,
                            note: Some(format!(
                                "Criado a partir do ProxyJump do perfil \"{}\".",
                                cand.alias
                            )),
                        });
                        name
                    };
                    if target == cand.alias {
                        cand.warnings.push(w(
                            &d,
                            "ProxyJump aponta para o próprio host: ignorado.".into(),
                        ));
                    } else {
                        cand.jump_alias = Some(target);
                    }
                }
            }
        }
        candidates.push(cand);
    }
    ImportPreview {
        candidates,
        global_warnings,
    }
}

/// Retorna o candidato e, se houver, o salto escolhido (token + diretiva).
fn build_candidate(
    ctx: &Ctx,
    job: &Job,
    mut warnings: Vec<Warning>,
) -> (Candidate, Option<(String, Directive)>) {
    let alias = job.alias.as_str();
    let host = ctx
        .first(alias, "hostname")
        .map(|d| d.args[0].replace("%h", alias))
        .unwrap_or_else(|| alias.to_string());
    let user = job
        .user
        .clone()
        .or_else(|| ctx.first(alias, "user").map(|d| d.args[0].clone()))
        .unwrap_or_else(|| ctx.opts.local_user.clone());

    let mut profile = Profile::new(alias, host.clone(), user.clone());

    profile.port = match job.port {
        Some(p) => p,
        None => match ctx.first(alias, "port") {
            Some(d) => match d.args[0].parse::<u16>().ok().filter(|p| *p != 0) {
                Some(p) => p,
                None => {
                    warnings.push(w(
                        d,
                        format!("Port \"{}\" inválida: usada a 22.", d.args[0]),
                    ));
                    22
                }
            },
            None => 22,
        },
    };

    let ids: Vec<&Directive> = ctx.all(alias, "identityfile").collect();
    if let Some(d) = ids.first() {
        let t = Tokens {
            home: &ctx.opts.home,
            local_user: &ctx.opts.local_user,
            host: &host,
            remote_user: &user,
        };
        let path = expand_path(&d.args[0], &t);
        profile.auth = AuthMethod::KeyFile {
            path: PathBuf::from(path),
            passphrase: None,
        };
        if ids.len() > 1 {
            warnings.push(w(
                ids[1],
                format!(
                    "{} IdentityFile encontrados: apenas o primeiro foi importado.",
                    ids.len()
                ),
            ));
        }
    }

    if let Some(d) = ctx.first(alias, "serveraliveinterval") {
        match d.args[0].parse::<u32>() {
            Ok(0) => profile.keepalive_secs = None,
            Ok(n) => profile.keepalive_secs = Some(n),
            Err(_) => warnings.push(w(
                d,
                format!("ServerAliveInterval \"{}\" inválido: ignorado.", d.args[0]),
            )),
        }
    }
    if let Some(d) = ctx.first(alias, "connecttimeout") {
        match d.args[0].parse::<u32>() {
            Ok(n) if n > 0 => profile.connect_timeout_secs = n,
            _ => warnings.push(w(
                d,
                format!("ConnectTimeout \"{}\" inválido: ignorado.", d.args[0]),
            )),
        }
    }
    if let Some(d) = ctx.first(alias, "forwardagent")
        && matches!(d.args[0].to_ascii_lowercase().as_str(), "yes" | "true")
    {
        warnings.push(w(
            d,
            "ForwardAgent importado como desligado: desabilitado por segurança, ative manualmente."
                .into(),
        ));
    }

    let mut hop = None;
    if let Some(d) = ctx.first(alias, "proxyjump") {
        let toks: Vec<String> = d
            .args
            .iter()
            .flat_map(|a| a.split(',').map(str::to_string))
            .filter(|s| !s.is_empty())
            .collect();
        if !(toks.len() == 1 && toks[0].eq_ignore_ascii_case("none")) && !toks.is_empty() {
            if toks.len() > 1 {
                warnings.push(w(
                    d,
                    format!(
                        "ProxyJump com {} saltos: apenas o último (\"{}\") foi importado; configure os anteriores como jump do próprio salto.",
                        toks.len(),
                        toks[toks.len() - 1]
                    ),
                ));
            }
            hop = Some((toks[toks.len() - 1].clone(), d.clone()));
        }
    }

    if let Err(e) = profile.validate() {
        warnings.push(Warning {
            line: 0,
            file: ctx.opts.root_name.clone(),
            msg: format!("Perfil \"{alias}\" inválido: {e}"),
        });
    }

    (
        Candidate {
            alias: alias.to_string(),
            profile,
            jump_alias: None,
            warnings,
        },
        hop,
    )
}

/// Liga `jump_alias` aos `ProfileId` dos candidatos importados juntos.
/// Alias inexistente ou ciclo → `jump = None` e aviso no candidato.
pub fn resolve_jumps(cands: &mut [Candidate]) {
    let ids: HashMap<String, ProfileId> = cands
        .iter()
        .rev()
        .map(|c| (c.alias.clone(), c.profile.id))
        .collect();
    let mut next: HashMap<ProfileId, ProfileId> = HashMap::new();
    for c in cands.iter_mut() {
        c.profile.jump = None;
        if let Some(a) = c.jump_alias.clone() {
            match ids.get(&a) {
                Some(id) if *id != c.profile.id => {
                    c.profile.jump = Some(*id);
                    next.insert(c.profile.id, *id);
                }
                Some(_) => c.warnings.push(Warning {
                    line: 0,
                    file: String::new(),
                    msg: "O salto aponta para o próprio perfil: ignorado.".into(),
                }),
                None => c.warnings.push(Warning {
                    line: 0,
                    file: String::new(),
                    msg: format!("Salto \"{a}\" não encontrado entre os perfis importados."),
                }),
            }
        }
    }
    for c in cands.iter_mut() {
        let start = c.profile.id;
        let mut cur = start;
        let mut cyclic = false;
        for _ in 0..=next.len() {
            match next.get(&cur) {
                Some(n) if *n == start => {
                    cyclic = true;
                    break;
                }
                Some(n) => cur = *n,
                None => break,
            }
        }
        if cyclic {
            c.profile.jump = None;
            c.warnings.push(Warning {
                line: 0,
                file: String::new(),
                msg: "Ciclo de saltos (ProxyJump) detectado: salto ignorado.".into(),
            });
        }
    }
}
