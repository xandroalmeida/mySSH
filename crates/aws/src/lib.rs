//! AWS para o mySSH: túnel SSM Session Manager em Rust puro (sem aws CLI nem
//! session-manager-plugin) e login IAM Identity Center (SSO) pelo próprio app.
//!
//! [`SsmTunnel`] implementa [`myssh_ssh::Tunnel`]: `StartSession` com o documento
//! `AWS-StartSSHSession` e o protocolo do data channel sobre websocket (ack, ordem,
//! retransmissão, handshake com o agente). O SSH roda por dentro, ponta a ponta: a host key é
//! verificada normalmente. Opcionalmente publica antes uma chave efêmera pelo EC2 Instance
//! Connect (válida por 60 s, nada gravado no servidor).
//!
//! Credenciais: perfil de `~/.aws` (cadeia do `aws-config`, inclusive `aws login`/SSO do CLI) ou
//! SSO do app ([`sso`]), com tokens no cofre do SO. Nenhum token/credencial vai para log, erro ou
//! `Debug`.

mod channel;
mod message;
mod profiles;
pub mod sso;
mod text;

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_ssm::error::{ProvideErrorMetadata, SdkError};
use myssh_core::{AwsCredentials, AwsSsm};
use myssh_secrets::Vault;
use myssh_ssh::{Tunnel, TunnelIo, TunnelOpen};
use secrecy::SecretString;

pub use profiles::list_profiles;
pub use text::{NO_REGION, SSO_LOGIN_REQUIRED};

/// Erros de credencial que a UI trata de forma especial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwsError {
    /// Cofre bloqueado: desbloquear e tentar de novo.
    VaultLocked,
    /// Sessão SSO ausente ou vencida: fazer login ([`sso::begin_login`]).
    LoginRequired,
    /// Mensagem já traduzida e sem segredos.
    Failed(String),
}

impl std::fmt::Display for AwsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VaultLocked => f.write_str(&myssh_secrets::SecretError::Locked.to_string()),
            Self::LoginRequired => f.write_str(text::SSO_LOGIN_REQUIRED()),
            Self::Failed(m) => f.write_str(m),
        }
    }
}

/// Código + mensagem de um erro do SDK, sanitizado e com tamanho limitado.
pub(crate) fn sdk_detail<E, R>(e: &SdkError<E, R>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
    R: std::fmt::Debug,
{
    let raw = match (e.code(), e.message()) {
        (Some(c), Some(m)) => format!("{c}: {m}"),
        (Some(c), None) => c.to_string(),
        _ => aws_sdk_ssm::error::DisplayErrorContext(e).to_string(),
    };
    myssh_ssh::sanitize_text(&raw, 300, false)
}

/// Configuração do SDK com a região e as credenciais do perfil.
async fn sdk_config(ssm: &AwsSsm, vault: &Vault) -> Result<aws_config::SdkConfig, AwsError> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if !ssm.region.is_empty() {
        loader = loader.region(aws_config::Region::new(ssm.region.clone()));
    }
    match &ssm.credentials {
        AwsCredentials::Profile { name } => {
            if let Some(n) = name {
                loader = loader.profile_name(n);
            }
        }
        AwsCredentials::Sso {
            start_url,
            sso_region,
            account_id,
            role_name,
        } => {
            let creds =
                sso::role_credentials(vault, start_url, sso_region, account_id, role_name).await?;
            loader = loader.credentials_provider(creds);
        }
    }
    let cfg = loader.load().await;
    if cfg.region().is_none() {
        return Err(AwsError::Failed(text::NO_REGION().into()));
    }
    Ok(cfg)
}

/// Túnel SSM até `port` da instância `target` (`i-…`/`mi-…`).
pub struct SsmTunnel {
    target: String,
    port: u16,
    ssm: AwsSsm,
    vault: Arc<Vault>,
    /// (usuário do SO, chave pública OpenSSH) a publicar pelo EC2 Instance Connect.
    instance_connect: Option<(String, String)>,
}

impl SsmTunnel {
    pub fn new(target: impl Into<String>, port: u16, ssm: AwsSsm, vault: Arc<Vault>) -> Self {
        Self {
            target: target.into(),
            port,
            ssm,
            vault,
            instance_connect: None,
        }
    }

    /// Antes de cada abertura, publica `public_key` para `os_user` na instância (60 s).
    pub fn with_instance_connect(mut self, os_user: String, public_key: String) -> Self {
        self.instance_connect = Some((os_user, public_key));
        self
    }

    async fn open_stream(&self) -> Result<Box<dyn TunnelIo>, String> {
        let cfg = sdk_config(&self.ssm, &self.vault)
            .await
            .map_err(|e| e.to_string())?;
        if let Some((user, key)) = &self.instance_connect {
            aws_sdk_ec2instanceconnect::Client::new(&cfg)
                .send_ssh_public_key()
                .instance_id(&self.target)
                .instance_os_user(user)
                .ssh_public_key(key)
                .send()
                .await
                .map_err(|e| text::aws_error("SendSSHPublicKey", &sdk_detail(&e)))?;
        }
        let out = aws_sdk_ssm::Client::new(&cfg)
            .start_session()
            .target(&self.target)
            .document_name("AWS-StartSSHSession")
            .parameters("portNumber", vec![self.port.to_string()])
            .send()
            .await
            .map_err(|e| text::aws_error("StartSession", &sdk_detail(&e)))?;
        let (Some(url), Some(token)) = (out.stream_url, out.token_value) else {
            return Err(text::aws_error("StartSession", "-"));
        };
        let token = SecretString::from(token);
        let stream = channel::open(&url, &token)
            .await
            .map_err(|e| text::ssm_failed(&e))?;
        Ok(Box::new(stream))
    }
}

impl Tunnel for SsmTunnel {
    fn open(&self) -> TunnelOpen<'_> {
        Box::pin(self.open_stream())
    }
}
