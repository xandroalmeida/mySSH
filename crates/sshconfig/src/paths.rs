//! Utilitários de caminho e de padrões (`*`, `?`) sem dependências externas.

/// Casamento de padrão com `*` e `?` (sensível a maiúsculas, como no ssh).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || (p[pi] != '*' && p[pi] == t[ti])) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn has_wildcard(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}

fn sep_for(home: &str) -> char {
    if home.contains('\\') && !home.contains('/') {
        '\\'
    } else {
        '/'
    }
}

/// Junta `home` e `rest` usando o separador do próprio `home`.
pub fn join_home(home: &str, rest: &str) -> String {
    let sep = sep_for(home);
    let h = home.trim_end_matches(['/', '\\']);
    let r = rest.trim_start_matches(['/', '\\']);
    if r.is_empty() {
        h.to_string()
    } else {
        format!("{h}{sep}{r}")
    }
}

fn is_absolute(p: &str) -> bool {
    if p.starts_with('/') || p.starts_with('\\') {
        return true;
    }
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

/// Contexto para expansão de tokens.
pub struct Tokens<'a> {
    pub home: &'a str,
    pub local_user: &'a str,
    pub host: &'a str,
    pub remote_user: &'a str,
}

/// Expande `%d %u %h %r %%`, `%USERPROFILE%`/`%HOME%` e `~`. Caminhos relativos
/// são resolvidos a partir do diretório do usuário. Suporta separadores `/` e `\`.
pub fn expand_path(raw: &str, t: &Tokens) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('%') => {
                out.push('%');
                i += 2;
            }
            Some('d') => {
                out.push_str(t.home);
                i += 2;
            }
            Some('u') => {
                out.push_str(t.local_user);
                i += 2;
            }
            Some('h') => {
                out.push_str(t.host);
                i += 2;
            }
            Some('r') => {
                out.push_str(t.remote_user);
                i += 2;
            }
            _ => {
                let rest: String = chars[i..].iter().collect();
                let up = rest.to_ascii_uppercase();
                if up.starts_with("%USERPROFILE%") {
                    out.push_str(t.home);
                    i += "%USERPROFILE%".len();
                } else if up.starts_with("%HOME%") {
                    out.push_str(t.home);
                    i += "%HOME%".len();
                } else {
                    out.push('%');
                    i += 1;
                }
            }
        }
    }
    let expanded = if out == "~" {
        t.home.to_string()
    } else if let Some(rest) = out.strip_prefix("~/").or_else(|| out.strip_prefix("~\\")) {
        join_home(t.home, rest)
    } else {
        out
    };
    if is_absolute(&expanded) {
        expanded
    } else {
        join_home(t.home, &expanded)
    }
}
