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
    /// Chave ed25519 efêmera gerada em memória a cada conexão e publicada por 60 s na instância
    /// pela API EC2 Instance Connect (nada é gravado no servidor). Só com [`Transport::AwsSsm`]
    /// em instância EC2 (`i-…`).
    Ec2InstanceConnect,
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

/// Como o cliente alcança a porta SSH do perfil.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Transport {
    /// TCP direto para `host:port` (ou via jump host).
    #[default]
    Direct,
    /// Túnel AWS SSM Session Manager até a porta `port` da instância `host` (`i-…`/`mi-…`).
    /// Só no início da cadeia: um perfil SSM não tem jump host (mas pode ser o jump de outro).
    AwsSsm(AwsSsm),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwsSsm {
    /// Região da instância. Vazia = a do perfil AWS (só com [`AwsCredentials::Profile`]).
    pub region: String,
    pub credentials: AwsCredentials,
}

/// De onde vêm as credenciais AWS. Nenhum segredo aqui: tokens SSO ficam no cofre do SO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AwsCredentials {
    /// Perfil de `~/.aws/config`/`~/.aws/credentials`; `None` = cadeia padrão (env, `default`, …).
    Profile { name: Option<String> },
    /// IAM Identity Center (SSO) com login pelo próprio app.
    Sso {
        start_url: String,
        sso_region: String,
        account_id: String,
        role_name: String,
    },
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
    #[serde(default)]
    pub transport: Transport,
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
            transport: Transport::Direct,
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
        if self.auth == AuthMethod::Ec2InstanceConnect
            && !(matches!(self.transport, Transport::AwsSsm(_)) && self.host.starts_with("i-"))
        {
            return Err(ProfileError::InstanceConnectNeedsSsm);
        }
        if let Transport::AwsSsm(ssm) = &self.transport {
            if self.jump.is_some() {
                return Err(ProfileError::SsmWithJump);
            }
            let simple = |s: &str| {
                s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            };
            let region_ok = |r: &str| !r.is_empty() && simple(r);
            if !(ssm.region.is_empty() || region_ok(&ssm.region)) {
                return Err(ProfileError::InvalidRegion);
            }
            match &ssm.credentials {
                AwsCredentials::Profile { name } => {
                    if name.as_deref().is_some_and(|n| {
                        n.trim().is_empty()
                            || n.chars().any(|c| c.is_whitespace() || c.is_control())
                    }) {
                        return Err(ProfileError::InvalidAwsProfile);
                    }
                }
                AwsCredentials::Sso {
                    start_url,
                    sso_region,
                    account_id,
                    role_name,
                } => {
                    if ssm.region.is_empty() {
                        return Err(ProfileError::InvalidRegion);
                    }
                    if !start_url.starts_with("https://")
                        || start_url
                            .chars()
                            .any(|c| c.is_whitespace() || c.is_control())
                    {
                        return Err(ProfileError::InvalidSsoUrl);
                    }
                    if !region_ok(sso_region) {
                        return Err(ProfileError::InvalidRegion);
                    }
                    if account_id.len() != 12 || !account_id.chars().all(|c| c.is_ascii_digit()) {
                        return Err(ProfileError::InvalidAwsAccount);
                    }
                    if role_name.is_empty()
                        || role_name
                            .chars()
                            .any(|c| c.is_whitespace() || c.is_control())
                    {
                        return Err(ProfileError::InvalidAwsRole);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("{}", myssh_i18n::tr!("O nome do perfil é obrigatório.", "The profile name is required.", "El nombre del perfil es obligatorio."))]
    EmptyName,
    #[error("{}", myssh_i18n::tr!("Host inválido.", "Invalid host.", "Host no válido."))]
    InvalidHost,
    #[error("{}", myssh_i18n::tr!("Porta inválida.", "Invalid port.", "Puerto no válido."))]
    InvalidPort,
    #[error("{}", myssh_i18n::tr!("Usuário inválido.", "Invalid user.", "Usuario no válido."))]
    InvalidUser,
    #[error("{}", myssh_i18n::tr!("O perfil não pode usar a si mesmo como jump host.", "A profile cannot use itself as a jump host.", "Un perfil no puede usarse a sí mismo como jump host."))]
    JumpLoop,
    #[error("{}", myssh_i18n::tr!("Um perfil AWS SSM não pode usar jump host (ele pode ser o jump host de outro perfil).", "An AWS SSM profile cannot use a jump host (it can be another profile's jump host).", "Un perfil AWS SSM no puede usar jump host (puede ser el jump host de otro perfil)."))]
    SsmWithJump,
    #[error("{}", myssh_i18n::tr!("Região AWS inválida.", "Invalid AWS region.", "Región de AWS no válida."))]
    InvalidRegion,
    #[error("{}", myssh_i18n::tr!("Nome de perfil AWS inválido.", "Invalid AWS profile name.", "Nombre de perfil de AWS no válido."))]
    InvalidAwsProfile,
    #[error("{}", myssh_i18n::tr!("A URL de início do SSO deve começar com https://.", "The SSO start URL must start with https://.", "La URL de inicio de SSO debe comenzar con https://."))]
    InvalidSsoUrl,
    #[error("{}", myssh_i18n::tr!("O ID da conta AWS deve ter 12 dígitos.", "The AWS account ID must have 12 digits.", "El ID de la cuenta de AWS debe tener 12 dígitos."))]
    InvalidAwsAccount,
    #[error("{}", myssh_i18n::tr!("Nome de role inválido.", "Invalid role name.", "Nombre de rol no válido."))]
    InvalidAwsRole,
    #[error("{}", myssh_i18n::tr!("EC2 Instance Connect exige conexão AWS SSM com uma instância EC2 (i-…).", "EC2 Instance Connect requires an AWS SSM connection to an EC2 instance (i-…).", "EC2 Instance Connect requiere conexión AWS SSM con una instancia EC2 (i-…)."))]
    InstanceConnectNeedsSsm,
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

#[cfg(test)]
mod transport_tests {
    use super::*;

    fn ssm(credentials: AwsCredentials, region: &str) -> Profile {
        let mut p = Profile::new("a", "i-0123456789abcdef0", "ubuntu");
        p.transport = Transport::AwsSsm(AwsSsm {
            region: region.into(),
            credentials,
        });
        p
    }

    fn sso() -> AwsCredentials {
        AwsCredentials::Sso {
            start_url: "https://d-123.awsapps.com/start".into(),
            sso_region: "us-east-1".into(),
            account_id: "123456789012".into(),
            role_name: "Admin".into(),
        }
    }

    #[test]
    fn perfil_aws_valido() {
        assert_eq!(
            ssm(AwsCredentials::Profile { name: None }, "").validate(),
            Ok(())
        );
        let named = AwsCredentials::Profile {
            name: Some("prod".into()),
        };
        assert_eq!(ssm(named, "sa-east-1").validate(), Ok(()));
        assert_eq!(ssm(sso(), "us-east-1").validate(), Ok(()));
    }

    #[test]
    fn ssm_rejeita_jump_e_campos_ruins() {
        let mut p = ssm(AwsCredentials::Profile { name: None }, "");
        p.jump = Some(ProfileId::new());
        assert_eq!(p.validate(), Err(ProfileError::SsmWithJump));
        let p = ssm(AwsCredentials::Profile { name: None }, "us east");
        assert_eq!(p.validate(), Err(ProfileError::InvalidRegion));
        let p = ssm(
            AwsCredentials::Profile {
                name: Some(" ".into()),
            },
            "",
        );
        assert_eq!(p.validate(), Err(ProfileError::InvalidAwsProfile));
        // SSO exige a região da instância
        assert_eq!(ssm(sso(), "").validate(), Err(ProfileError::InvalidRegion));
        let mut p = ssm(sso(), "us-east-1");
        if let Transport::AwsSsm(AwsSsm {
            credentials: AwsCredentials::Sso { account_id, .. },
            ..
        }) = &mut p.transport
        {
            *account_id = "1234".into();
        }
        assert_eq!(p.validate(), Err(ProfileError::InvalidAwsAccount));
        let mut p = ssm(sso(), "us-east-1");
        if let Transport::AwsSsm(AwsSsm {
            credentials: AwsCredentials::Sso { start_url, .. },
            ..
        }) = &mut p.transport
        {
            *start_url = "http://x".into();
        }
        assert_eq!(p.validate(), Err(ProfileError::InvalidSsoUrl));
    }

    #[test]
    fn instance_connect_so_com_ssm_em_ec2() {
        let mut p = ssm(AwsCredentials::Profile { name: None }, "");
        p.auth = AuthMethod::Ec2InstanceConnect;
        assert_eq!(p.validate(), Ok(()));
        p.host = "mi-0123456789abcdef0".into();
        assert_eq!(p.validate(), Err(ProfileError::InstanceConnectNeedsSsm));
        let mut d = Profile::new("a", "i-0123456789abcdef0", "u");
        d.auth = AuthMethod::Ec2InstanceConnect;
        assert_eq!(d.validate(), Err(ProfileError::InstanceConnectNeedsSsm));
    }

    #[test]
    fn transport_ausente_no_json_e_direto() {
        let p = Profile::new("a", "h", "u");
        let mut v: serde_json::Value = serde_json::to_value(&p).unwrap();
        v.as_object_mut().unwrap().remove("transport");
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back.transport, Transport::Direct);
    }
}

#[cfg(test)]
mod i18n_tests {
    use super::*;

    #[test]
    fn erros_em_ingles_e_espanhol() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(ProfileError::InvalidPort.to_string(), "Invalid port.");
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(ProfileError::InvalidPort.to_string(), "Puerto no válido.");
        });
    }
}
