//! Sidecar de retomada: registra qual temporário pertence a qual origem (tamanho + mtime).
//!
//! O conteúdo vem do disco/servidor de destino e **não é confiável**: o nome do temporário é
//! revalidado (`names::is_part_name_for`) e a retomada exige que o temporário exista, seja
//! prefixo plausível da origem (tamanho <= origem) e que o trecho final bata byte a byte.

use crate::names;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Arquivos menores que isto não usam sidecar (recomeçar é barato).
pub const RESUME_MIN_SIZE: u64 = 4 * 1024 * 1024;
/// Quantos bytes finais do temporário são comparados com a origem antes de retomar.
pub const TAIL_CHECK: u64 = 64 * 1024;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub v: u32,
    /// Nome final do arquivo (sem diretório).
    pub name: String,
    /// Nome do temporário (sem diretório).
    pub temp: String,
    pub src_size: u64,
    pub src_mtime_secs: i64,
    pub src_mtime_nanos: u32,
}

fn split_time(t: Option<SystemTime>) -> (i64, u32) {
    match t {
        Some(t) => match t.duration_since(UNIX_EPOCH) {
            Ok(d) => (d.as_secs() as i64, d.subsec_nanos()),
            Err(e) => (
                -(e.duration().as_secs() as i64),
                e.duration().subsec_nanos(),
            ),
        },
        None => (i64::MIN, 0),
    }
}

impl Sidecar {
    pub fn new(name: &str, temp: &str, src_size: u64, src_mtime: Option<SystemTime>) -> Self {
        let (s, n) = split_time(src_mtime);
        Sidecar {
            v: VERSION,
            name: name.to_string(),
            temp: temp.to_string(),
            src_size,
            src_mtime_secs: s,
            src_mtime_nanos: n,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// `None` se ilegível, de outra versão ou grande demais.
    pub fn parse(bytes: &[u8]) -> Option<Sidecar> {
        if bytes.len() > 4096 {
            return None;
        }
        let s: Sidecar = serde_json::from_slice(bytes).ok()?;
        (s.v == VERSION).then_some(s)
    }

    /// O sidecar descreve esta mesma origem e um temporário legítimo para `final_name`?
    pub fn matches(&self, final_name: &str, src_size: u64, src_mtime: Option<SystemTime>) -> bool {
        let (s, n) = split_time(src_mtime);
        self.name == final_name
            && names::is_part_name_for(final_name, &self.temp)
            && self.src_size == src_size
            && self.src_mtime_secs == s
            && self.src_mtime_nanos == n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t(secs: u64, nanos: u32) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::new(secs, nanos))
    }

    #[test]
    fn ida_e_volta() {
        let s = Sidecar::new("a.bin", ".a.bin.myssh-part-xyz", 100, t(5, 7));
        let p = Sidecar::parse(&s.to_bytes()).unwrap();
        assert_eq!(p, s);
        assert!(p.matches("a.bin", 100, t(5, 7)));
    }

    #[test]
    fn origem_diferente_invalida() {
        let s = Sidecar::new("a.bin", ".a.bin.myssh-part-xyz", 100, t(5, 7));
        assert!(!s.matches("a.bin", 101, t(5, 7)));
        assert!(!s.matches("a.bin", 100, t(6, 7)));
        assert!(!s.matches("a.bin", 100, t(5, 8)));
        assert!(!s.matches("b.bin", 100, t(5, 7)));
    }

    #[test]
    fn temporario_hostil_e_rejeitado() {
        for tmp in [
            "../../etc/passwd",
            "/etc/passwd",
            ".outro.myssh-part-1",
            ".a.bin.myssh-part-../x",
            "",
        ] {
            let s = Sidecar::new("a.bin", tmp, 1, t(1, 0));
            assert!(!s.matches("a.bin", 1, t(1, 0)), "{tmp}");
        }
    }

    #[test]
    fn lixo_e_versao_errada() {
        assert!(Sidecar::parse(b"nada").is_none());
        assert!(Sidecar::parse(&vec![b' '; 5000]).is_none());
        let mut s = Sidecar::new("a", ".a.myssh-part-1", 1, None);
        s.v = 99;
        assert!(Sidecar::parse(&s.to_bytes()).is_none());
    }

    #[test]
    fn mtime_ausente_nao_casa_com_presente() {
        let s = Sidecar::new("a.bin", ".a.bin.myssh-part-xyz", 1, None);
        assert!(s.matches("a.bin", 1, None));
        assert!(!s.matches("a.bin", 1, t(0, 0)));
    }
}
