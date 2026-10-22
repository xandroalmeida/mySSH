//! Lista os perfis AWS configurados (`~/.aws/config` e `~/.aws/credentials`), só os nomes.

use std::path::{Path, PathBuf};

fn home_file(env: &str, rel: &str) -> Option<PathBuf> {
    std::env::var_os(env)
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".aws").join(rel)))
}

/// Nomes de perfil, `default` primeiro, sem repetição. Arquivos ausentes = lista vazia.
/// Leitura pequena de disco: pode rodar fora da thread de UI se preferir.
pub fn list_profiles() -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = home_file("AWS_CONFIG_FILE", "config") {
        out.extend(parse(&read(&p), true));
    }
    if let Some(p) = home_file("AWS_SHARED_CREDENTIALS_FILE", "credentials") {
        out.extend(parse(&read(&p), false));
    }
    out.sort_by(|a, b| (a != "default").cmp(&(b != "default")).then(a.cmp(b)));
    out.dedup();
    out
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// Seções `[default]`, `[profile X]` (config) ou `[X]` (credentials). Ignora `[sso-session X]` etc.
fn parse(text: &str, is_config: bool) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let inner = l.strip_prefix('[')?.strip_suffix(']')?.trim();
            let name = if inner == "default" {
                inner
            } else if is_config {
                inner.strip_prefix("profile ")?.trim()
            } else {
                inner
            };
            (!name.is_empty() && !name.chars().any(|c| c.is_whitespace() || c.is_control()))
                .then(|| name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secoes_de_config_e_credentials() {
        let cfg = "[default]\nregion=us-east-1\n[profile prod]\n[sso-session corp]\n[services x]\n[profile  dev ]\n";
        assert_eq!(parse(cfg, true), vec!["default", "prod", "dev"]);
        let creds = "[default]\naws_access_key_id=x\n[ci]\n# [comentario]\n";
        assert_eq!(parse(creds, false), vec!["default", "ci"]);
    }
}
