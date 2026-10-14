//! Arquivo próprio de hosts confiáveis (formato simples, uma entrada por linha):
//! `[host]:porta algoritmo chave-base64`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use russh::keys::{HashAlg, PublicKey};

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Chave idêntica à salva.
    Trusted,
    /// Host sem entrada: precisa de confirmação do usuário.
    Unknown,
    /// Host conhecido com chave DIFERENTE da salva (possível MITM).
    Mismatch { saved_fingerprint: String },
}

pub struct KnownHosts {
    path: PathBuf,
}

pub fn host_id(host: &str, port: u16) -> String {
    format!("[{}]:{}", host.to_ascii_lowercase(), port)
}

/// Fingerprint no formato do OpenSSH (`SHA256:...`, base64 sem padding).
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// `algoritmo base64` (sem comentário).
fn key_line(key: &PublicKey) -> Result<String, String> {
    let s = key.to_openssh().map_err(|e| e.to_string())?;
    let mut it = s.split_whitespace();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => Ok(format!("{a} {b}")),
        _ => Err("chave pública inválida".into()),
    }
}

impl KnownHosts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Caminho padrão no diretório de dados do app (ou `MYSSH_POC_DATA_DIR`).
    pub fn default_path() -> PathBuf {
        if let Some(d) = std::env::var_os("MYSSH_POC_DATA_DIR") {
            return Path::new(&d).join("known_hosts");
        }
        directories::ProjectDirs::from("br", "mySSH", "mySSH-poc")
            .map(|d| d.data_dir().join("known_hosts"))
            .unwrap_or_else(|| PathBuf::from("known_hosts"))
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> Result<Verdict, String> {
        let id = host_id(host, port);
        let wanted = key_line(key)?;
        let content = match fs::read_to_string(&self.path) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Verdict::Unknown),
            Err(e) => return Err(format!("não foi possível ler known_hosts: {e}")),
        };
        let mut seen_other: Option<String> = None;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((lid, rest)) = line.split_once(' ') else {
                continue;
            };
            if lid != id {
                continue;
            }
            if rest.trim() == wanted {
                return Ok(Verdict::Trusted);
            }
            // Mesmo host, chave (ou algoritmo) diferente.
            let saved = PublicKey::from_openssh(rest.trim())
                .map(|k| fingerprint(&k))
                .unwrap_or_else(|_| "?".into());
            seen_other = Some(saved);
        }
        Ok(match seen_other {
            Some(saved_fingerprint) => Verdict::Mismatch { saved_fingerprint },
            None => Verdict::Unknown,
        })
    }

    pub fn add(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{} {}", host_id(host, port), key_line(key)?).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKwq2Hgymay7rPv/rivXVOg6uvyh77ygJKMrdrKKhItj";
    const K2: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPW25y3ON94jXXZAOpHkPFecBeSewe+/bxH/Sg0pGhuc";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("poc-egui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d.join("known_hosts")
    }

    #[test]
    fn unknown_then_trusted_then_mismatch() {
        let kh = KnownHosts::new(tmp("kh"));
        let k1 = PublicKey::from_openssh(K1).unwrap();
        let k2 = PublicKey::from_openssh(K2).unwrap();
        assert_eq!(kh.check("h", 22, &k1).unwrap(), Verdict::Unknown);
        kh.add("h", 22, &k1).unwrap();
        assert_eq!(kh.check("h", 22, &k1).unwrap(), Verdict::Trusted);
        assert!(matches!(
            kh.check("h", 22, &k2).unwrap(),
            Verdict::Mismatch { .. }
        ));
        // outra porta é outro host
        assert_eq!(kh.check("h", 2222, &k2).unwrap(), Verdict::Unknown);
    }

    #[test]
    fn fingerprint_format() {
        let k1 = PublicKey::from_openssh(K1).unwrap();
        let fp = fingerprint(&k1);
        assert!(fp.starts_with("SHA256:") && !fp.ends_with('='), "{fp}");
    }
}
