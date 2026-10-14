//! Contratos centrais do mySSH. Alterações aqui passam pelo Opus (ver AGENTS.md).
//!
//! Nenhum tipo deste crate carrega segredo: credenciais são sempre referências
//! ([`SecretRef`]) resolvidas em tempo de uso pelo crate de segredos.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod trust;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(ProfileId);
id_type!(CredentialId);
id_type!(SessionId);
id_type!(FolderId);

/// Ambiente declarado do servidor. Usado para sinal visual (cor + texto/ícone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Environment {
    #[default]
    None,
    Development,
    Staging,
    Production,
}

/// Referência opaca a um segredo guardado no armazenamento seguro.
/// O valor do segredo nunca aparece aqui.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef(pub String);

/// Método de autenticação de um perfil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Senha; `credential` = None → perguntar a cada conexão (não persistir).
    Password { credential: Option<CredentialId> },
    /// Chave privada referenciada em disco (não copiada). Passphrase opcional no cofre.
    KeyFile {
        path: std::path::PathBuf,
        passphrase: Option<CredentialId>,
    },
    /// SSH agent local (não implica forwarding).
    Agent,
    /// keyboard-interactive (MFA); respostas nunca persistidas.
    KeyboardInteractive,
}

/// Credencial reutilizável entre perfis. Metadados apenas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: CredentialId,
    pub label: String,
    pub kind: CredentialKind,
    pub secret: SecretRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialKind {
    Password,
    KeyPassphrase,
}

/// Override de tema por perfil. `None` em cada campo herda o global.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ThemeOverride {
    pub palette: Option<String>,
    pub font_family: Option<String>,
    pub font_size_tenths: Option<u16>,
    /// Cor de destaque da aba (RGB).
    pub accent: Option<[u8; 3]>,
}

/// Perfil de conexão ("conta").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub folder: Option<FolderId>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub environment: Environment,
    pub theme: ThemeOverride,
    /// Jump host (outro perfil). Cada salto valida sua própria host key.
    pub jump: Option<ProfileId>,
    pub agent_forwarding: bool,
    pub keepalive_secs: Option<u32>,
    pub connect_timeout_secs: u32,
}

impl Profile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            id: ProfileId::new(),
            name: name.into(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::Password { credential: None },
            folder: None,
            tags: Vec::new(),
            favorite: false,
            environment: Environment::None,
            theme: ThemeOverride::default(),
            jump: None,
            agent_forwarding: false,
            keepalive_secs: Some(30),
            connect_timeout_secs: 15,
        }
    }

    /// Validação de campos; mensagens em pt-BR.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.name.trim().is_empty() {
            return Err(ProfileError::EmptyName);
        }
        if self.host.trim().is_empty()
            || self
                .host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(ProfileError::InvalidHost);
        }
        if self.port == 0 {
            return Err(ProfileError::InvalidPort);
        }
        if self.username.trim().is_empty() || self.username.chars().any(|c| c.is_control()) {
            return Err(ProfileError::InvalidUser);
        }
        if self.jump == Some(self.id) {
            return Err(ProfileError::JumpLoop);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("O nome do perfil é obrigatório.")]
    EmptyName,
    #[error("Host inválido.")]
    InvalidHost,
    #[error("Porta inválida.")]
    InvalidPort,
    #[error("Usuário inválido.")]
    InvalidUser,
    #[error("O perfil não pode usar a si mesmo como jump host.")]
    JumpLoop,
}

/// Estados explícitos de uma sessão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    /// Aguardando decisão do usuário sobre host key desconhecida.
    AwaitingHostTrust,
    AwaitingAuth,
    Connected,
    Reconnecting,
    /// Mensagem já sanitizada (sem segredos), pt-BR.
    Error(String),
}
