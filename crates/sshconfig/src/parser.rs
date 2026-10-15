//! Leitura do subconjunto de sintaxe: blocos, diretivas, Include, Match.

use crate::fsys::IncludeSource;
use crate::paths::{Tokens, expand_path, join_home};
use crate::{ImportOptions, Warning};

pub const MAX_INCLUDE_DEPTH: usize = 8;
/// Orçamento global por importação: arquivos incluídos lidos (contando repetições).
pub const MAX_INCLUDE_FILES: usize = 256;
/// Orçamento global por importação: bytes lidos de arquivos incluídos (contando repetições).
pub const MAX_INCLUDE_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// Máximo de caracteres do nome de diretiva exibido nos avisos.
const MAX_DIRECTIVE_NAME_CHARS: usize = 64;

const COMMAND_KEYS: &[&str] = &[
    "proxycommand",
    "localcommand",
    "permitlocalcommand",
    "knownhostscommand",
];
const SUPPORTED_KEYS: &[&str] = &[
    "hostname",
    "port",
    "user",
    "identityfile",
    "proxyjump",
    "serveraliveinterval",
    "connecttimeout",
    "forwardagent",
];

#[derive(Debug, Clone)]
pub struct Directive {
    pub line: usize,
    pub file: String,
    pub key: String,
    pub args: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Block {
    pub patterns: Vec<String>,
    pub directives: Vec<Directive>,
    pub warnings: Vec<Warning>,
}

pub struct Parsed {
    pub blocks: Vec<Block>,
    pub global_warnings: Vec<Warning>,
}

type SplitResult = Result<Option<(String, Vec<String>)>, ()>;

/// Divide uma linha em (chave, argumentos). `Err` = aspas não fechadas.
pub fn split_line(line: &str) -> SplitResult {
    let s = line.trim_start();
    if s.is_empty() || s.starts_with('#') {
        return Ok(None);
    }
    let key_end = s
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(s.len());
    let key = s[..key_end].to_ascii_lowercase();
    let mut rest = s[key_end..].trim_start();
    if let Some(r) = rest.strip_prefix('=') {
        rest = r.trim_start();
    }
    let mut args = Vec::new();
    let mut chars = rest.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.peek() {
            None | Some('#') => break,
            _ => {}
        }
        let mut tok = String::new();
        let mut in_q = false;
        while let Some(&c) = chars.peek() {
            if in_q {
                chars.next();
                if c == '"' {
                    in_q = false;
                } else if c == '\\' && chars.peek() == Some(&'"') {
                    tok.push('"');
                    chars.next();
                } else {
                    tok.push(c);
                }
            } else if c == '"' {
                in_q = true;
                chars.next();
            } else if c.is_whitespace() {
                break;
            } else {
                tok.push(c);
                chars.next();
            }
        }
        if in_q {
            return Err(());
        }
        args.push(tok);
    }
    Ok(Some((key, args)))
}

struct Parser<'a> {
    opts: &'a ImportOptions,
    fs: &'a dyn IncludeSource,
    blocks: Vec<Block>,
    global_warnings: Vec<Warning>,
    files_read: usize,
    bytes_read: usize,
    budget_warned: bool,
}

pub fn parse(content: &str, opts: &ImportOptions, fs: &dyn IncludeSource) -> Parsed {
    let mut p = Parser {
        opts,
        fs,
        blocks: vec![Block {
            patterns: vec!["*".into()],
            ..Block::default()
        }],
        global_warnings: Vec::new(),
        files_read: 0,
        bytes_read: 0,
        budget_warned: false,
    };
    let mut stack = vec![opts.root_name.clone()];
    p.parse_content(content, &opts.root_name, 0, &mut stack, Some(0));
    Parsed {
        blocks: p.blocks,
        global_warnings: p.global_warnings,
    }
}

impl Parser<'_> {
    fn warn(&mut self, cur: Option<usize>, line: usize, file: &str, msg: String) {
        let w = Warning {
            line,
            file: file.to_string(),
            msg,
        };
        match cur {
            Some(i) if i != 0 => self.blocks[i].warnings.push(w),
            _ => self.global_warnings.push(w),
        }
    }

    fn parse_content(
        &mut self,
        content: &str,
        file: &str,
        depth: usize,
        stack: &mut Vec<String>,
        start: Option<usize>,
    ) {
        let mut cur = start;
        for (idx, raw) in content.lines().enumerate() {
            let line = idx + 1;
            let raw = if idx == 0 {
                raw.trim_start_matches('\u{feff}')
            } else {
                raw
            };
            let (key, args) = match split_line(raw) {
                Ok(Some(x)) => x,
                Ok(None) => continue,
                Err(()) => {
                    self.warn(
                        cur,
                        line,
                        file,
                        myssh_i18n::tr!(
                            "Aspas não fechadas: linha ignorada.",
                            "Unclosed quotes: line ignored.",
                            "Comillas sin cerrar: línea ignorada."
                        )
                        .into(),
                    );
                    continue;
                }
            };
            match key.as_str() {
                "host" => {
                    if args.is_empty() {
                        self.warn(
                            cur,
                            line,
                            file,
                            myssh_i18n::tr!(
                                "Host sem padrão: bloco ignorado.",
                                "Host without a pattern: block ignored.",
                                "Host sin patrón: bloque ignorado."
                            )
                            .into(),
                        );
                        cur = None;
                        continue;
                    }
                    self.blocks.push(Block {
                        patterns: args,
                        ..Block::default()
                    });
                    cur = Some(self.blocks.len() - 1);
                }
                "match" => {
                    let has_exec = args.iter().any(|a| a.eq_ignore_ascii_case("exec"));
                    self.global_warnings.push(Warning {
                        line,
                        file: file.to_string(),
                        msg: myssh_i18n::tr!(
                            "Bloco Match ignorado: condições Match não são suportadas.",
                            "Match block ignored: Match conditions are not supported.",
                            "Bloque Match ignorado: las condiciones Match no están soportadas."
                        )
                        .into(),
                    });
                    if has_exec {
                        self.global_warnings.push(Warning {
                            line,
                            file: file.to_string(),
                            msg: myssh_i18n::tr!(
                                "Match exec ignorada: executa comando local.",
                                "Match exec ignored: it runs a local command.",
                                "Match exec ignorado: ejecuta un comando local."
                            )
                            .into(),
                        });
                    }
                    cur = None;
                }
                _ if cur.is_none() => {}
                "include" => self.include(&args, line, file, depth, stack, cur),
                k if COMMAND_KEYS.contains(&k) => {
                    self.warn(
                        cur,
                        line,
                        file,
                        myssh_i18n::tr!(
                            format!("Diretiva {} ignorada: executa comando local.", name_of(raw)),
                            format!(
                                "Directive {} ignored: it runs a local command.",
                                name_of(raw)
                            ),
                            format!(
                                "Directiva {} ignorada: ejecuta un comando local.",
                                name_of(raw)
                            )
                        ),
                    );
                }
                k if SUPPORTED_KEYS.contains(&k) => {
                    if args.is_empty() {
                        self.warn(
                            cur,
                            line,
                            file,
                            myssh_i18n::tr!(
                                format!("Diretiva {} sem valor: ignorada.", name_of(raw)),
                                format!("Directive {} has no value: ignored.", name_of(raw)),
                                format!("Directiva {} sin valor: ignorada.", name_of(raw))
                            ),
                        );
                    } else if let Some(i) = cur {
                        self.blocks[i].directives.push(Directive {
                            line,
                            file: file.to_string(),
                            key,
                            args,
                        });
                    }
                }
                _ => self.warn(
                    cur,
                    line,
                    file,
                    myssh_i18n::tr!(
                        format!("Diretiva {} ignorada (não suportada).", name_of(raw)),
                        format!("Directive {} ignored (not supported).", name_of(raw)),
                        format!("Directiva {} ignorada (no soportada).", name_of(raw))
                    ),
                ),
            }
        }
    }

    fn include(
        &mut self,
        args: &[String],
        line: usize,
        file: &str,
        depth: usize,
        stack: &mut Vec<String>,
        cur: Option<usize>,
    ) {
        if args.is_empty() {
            self.warn(
                cur,
                line,
                file,
                myssh_i18n::tr!(
                    "Include sem caminho: ignorado.",
                    "Include without a path: ignored.",
                    "Include sin ruta: ignorado."
                )
                .into(),
            );
            return;
        }
        if depth + 1 > MAX_INCLUDE_DEPTH {
            self.warn(
                cur,
                line,
                file,
                myssh_i18n::tr!(
                    format!(
                        "Include ignorado: profundidade máxima ({MAX_INCLUDE_DEPTH}) excedida."
                    ),
                    format!("Include ignored: maximum depth ({MAX_INCLUDE_DEPTH}) exceeded."),
                    format!("Include ignorado: profundidad máxima ({MAX_INCLUDE_DEPTH}) excedida.")
                ),
            );
            return;
        }
        let ssh_dir = join_home(&self.opts.home, ".ssh");
        for pat in args {
            let t = Tokens {
                home: &self.opts.home,
                local_user: &self.opts.local_user,
                host: "",
                remote_user: "",
            };
            // Relativo é resolvido a partir de ~/.ssh (não de ~).
            let abs = if is_rel_to_ssh(pat) {
                join_home(&ssh_dir, pat)
            } else {
                expand_path(pat, &t)
            };
            let matches = self.fs.expand(&abs);
            // Padrão sem correspondência não é erro (como no ssh).
            for path in matches {
                if self.files_read >= MAX_INCLUDE_FILES
                    || self.bytes_read >= MAX_INCLUDE_TOTAL_BYTES
                {
                    if !self.budget_warned {
                        self.budget_warned = true;
                        self.warn(
                            cur,
                            line,
                            file,
                            myssh_i18n::tr!(
                                format!(
                                    "Includes restantes ignorados: orçamento da importação esgotado \
                                     ({MAX_INCLUDE_FILES} arquivos ou {} MiB no total).",
                                    MAX_INCLUDE_TOTAL_BYTES / (1024 * 1024)
                                ),
                                format!(
                                    "Remaining Includes ignored: import budget exhausted \
                                     ({MAX_INCLUDE_FILES} files or {} MiB in total).",
                                    MAX_INCLUDE_TOTAL_BYTES / (1024 * 1024)
                                ),
                                format!(
                                    "Includes restantes ignorados: presupuesto de importación agotado \
                                     ({MAX_INCLUDE_FILES} archivos o {} MiB en total).",
                                    MAX_INCLUDE_TOTAL_BYTES / (1024 * 1024)
                                )
                            ),
                        );
                    }
                    return;
                }
                if stack.contains(&path) {
                    self.warn(
                        cur,
                        line,
                        file,
                        myssh_i18n::tr!(
                            format!("Include ignorado: ciclo detectado em {path}."),
                            format!("Include ignored: cycle detected at {path}."),
                            format!("Include ignorado: ciclo detectado en {path}.")
                        ),
                    );
                    continue;
                }
                self.files_read += 1;
                match self.fs.read(&path) {
                    Ok(content) if self.bytes_read + content.len() > MAX_INCLUDE_TOTAL_BYTES => {
                        drop(content);
                        self.bytes_read = MAX_INCLUDE_TOTAL_BYTES; // esgota: os próximos já avisam
                    }
                    Ok(content) => {
                        self.bytes_read += content.len();
                        stack.push(path.clone());
                        self.parse_content(&content, &path, depth + 1, stack, cur);
                        stack.pop();
                    }
                    Err(e) => self.warn(
                        cur,
                        line,
                        file,
                        myssh_i18n::tr!(
                            format!("Não foi possível ler o Include {path}: {}.", io_msg(&e)),
                            format!("Could not read Include {path}: {}.", io_msg(&e)),
                            format!("No se pudo leer el Include {path}: {}.", io_msg(&e))
                        ),
                    ),
                }
            }
        }
    }
}

fn io_msg(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            myssh_i18n::tr!(
                "arquivo não encontrado",
                "file not found",
                "archivo no encontrado"
            )
        }
        std::io::ErrorKind::PermissionDenied => {
            myssh_i18n::tr!("sem permissão", "permission denied", "sin permiso")
        }
        std::io::ErrorKind::InvalidData => myssh_i18n::tr!(
            "arquivo inválido ou maior que 1 MiB",
            "invalid file or larger than 1 MiB",
            "archivo no válido o mayor de 1 MiB"
        ),
        _ => myssh_i18n::tr!("erro de leitura", "read error", "error de lectura"),
    }
}

fn is_rel_to_ssh(p: &str) -> bool {
    let b = p.as_bytes();
    let abs = p.starts_with(['/', '\\', '~', '%'])
        || (b.len() >= 3 && b[1] == b':' && b[0].is_ascii_alphabetic());
    !abs
}

/// Nome da diretiva para avisos: só o primeiro token e só se parecer uma palavra-chave (ASCII
/// alfanumérico), truncado. Nunca a linha bruta nem texto arbitrário: um `Include` de arquivo
/// qualquer (como `/etc/passwd`) não deve vazar conteúdo na UI.
fn name_of(raw: &str) -> String {
    let s = raw.trim_start();
    let end = s
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(s.len());
    let name = &s[..end];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return "(nome inválido)".into();
    }
    if name.len() > MAX_DIRECTIVE_NAME_CHARS {
        format!("{}…", &name[..MAX_DIRECTIVE_NAME_CHARS])
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod i18n_tests {
    use super::*;

    #[test]
    fn io_msg_traduzida() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(io_msg(&e), "file not found")
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(io_msg(&e), "archivo no encontrado")
        });
    }
}
