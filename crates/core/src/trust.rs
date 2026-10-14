//! Política de confiança de host keys, compartilhada por terminal, jump host e SFTP.

use serde::{Deserialize, Serialize};

/// Endpoint identificado por host + porta (como o usuário digitou, normalizado em minúsculas).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HostEndpoint {
    pub host: String,
    pub port: u16,
}

impl HostEndpoint {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.trim().to_ascii_lowercase(),
            port,
        }
    }
}

/// Chave pública de servidor em forma serializável.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostKey {
    /// Ex.: "ssh-ed25519".
    pub algorithm: String,
    /// Blob da chave no formato wire SSH, base64 (como em known_hosts).
    pub key_base64: String,
    /// "SHA256:..." no formato do OpenSSH.
    pub fingerprint_sha256: String,
}

/// Resultado da verificação contra o armazenamento de confiança.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustVerdict {
    Trusted,
    /// Nunca visto: pedir decisão ao usuário.
    Unknown,
    /// Já existe chave do mesmo algoritmo diferente: bloquear.
    Changed {
        previous: HostKey,
    },
    /// Host tem chave(s) confiável(eis) de outro algoritmo: tratar como alteração (bloquear, revisar).
    OtherAlgorithmKnown {
        known: Vec<HostKey>,
    },
    /// Chave revogada explicitamente pelo usuário.
    Revoked,
}

/// Armazenamento de host keys confiáveis. Implementado no crate de persistência.
pub trait HostTrustStore: Send + Sync {
    fn verify(
        &self,
        endpoint: &HostEndpoint,
        key: &HostKey,
    ) -> Result<TrustVerdict, TrustStoreError>;
    /// Persistir aceitação explícita do usuário.
    fn trust(&self, endpoint: &HostEndpoint, key: &HostKey) -> Result<(), TrustStoreError>;
    /// Substituir chave após revisão explícita (fluxo de "chave mudou").
    fn replace(&self, endpoint: &HostEndpoint, key: &HostKey) -> Result<(), TrustStoreError>;
    fn forget(&self, endpoint: &HostEndpoint) -> Result<(), TrustStoreError>;
    fn list(&self) -> Result<Vec<(HostEndpoint, HostKey)>, TrustStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TrustStoreError {
    #[error("Falha ao acessar o armazenamento de hosts confiáveis: {0}")]
    Storage(String),
}
