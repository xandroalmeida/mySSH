//! Login no AWS IAM Identity Center (SSO) pelo próprio app, sem aws CLI.
//!
//! Fluxo OAuth "device authorization" (`sso-oidc`): registra o cliente, pede um código ao usuário
//! (confirmado no navegador) e troca por um token de acesso + refresh token. Com o token,
//! `sso:GetRoleCredentials` devolve credenciais temporárias da role escolhida.
//!
//! Registro do cliente e tokens ficam **só no cofre do SO** (via [`Vault`]), por par
//! (start URL, região do SSO), compartilhados entre os perfis desse par. Os valores passam do
//! limite de uma entrada do cofre do Windows, então são gravados em pedaços.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aws_config::BehaviorVersion;
use aws_sdk_ssm::config::Credentials;
use myssh_core::SecretRef;
use myssh_secrets::{SecretError, Vault};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::{AwsError, sdk_detail, text};

const CLIENT_NAME: &str = "mySSH";
const SCOPE: &str = "sso:account:access";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Pedaço máximo por entrada do cofre (o limite do Windows é 1280 unidades UTF-16; JSON ASCII).
const PIECE: usize = 1000;
/// Antecedência para considerar o token vencido.
const SKEW_SECS: i64 = 300;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize)]
struct Registration {
    client_id: String,
    client_secret: String,
    expires_at: i64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.client_secret.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct Token {
    access_token: String,
    expires_at: i64,
    refresh_token: Option<String>,
}

impl Drop for Token {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(r) = &mut self.refresh_token {
            r.zeroize();
        }
    }
}

/// Nome base das entradas no cofre para (start URL, região do SSO). Nomes internos, fixos.
fn base(start_url: &str, sso_region: &str) -> String {
    let h = Sha256::digest(format!("{start_url}\n{sso_region}").as_bytes());
    let hex: String = h[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("aws-sso-{hex}")
}

fn map_secret(e: SecretError) -> AwsError {
    match e {
        SecretError::Locked => AwsError::VaultLocked,
        other => AwsError::Failed(other.to_string()),
    }
}

/// Grava `value` em pedaços: `name` guarda a quantidade, `name.i` cada pedaço.
fn put_large(vault: &Vault, name: &str, value: &str) -> Result<(), AwsError> {
    let old = count(vault, name)?;
    let pieces: Vec<&str> = value
        .as_bytes()
        .chunks(PIECE)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();
    for (i, p) in pieces.iter().enumerate() {
        vault
            .put(&piece_ref(name, i), SecretString::from(p.to_string()))
            .map_err(map_secret)?;
    }
    vault
        .put(
            &SecretRef(name.to_string()),
            SecretString::from(pieces.len().to_string()),
        )
        .map_err(map_secret)?;
    for i in pieces.len()..old.unwrap_or(0) {
        let _ = vault.delete(&piece_ref(name, i));
    }
    Ok(())
}

fn piece_ref(name: &str, i: usize) -> SecretRef {
    SecretRef(format!("{name}.{i}"))
}

fn count(vault: &Vault, name: &str) -> Result<Option<usize>, AwsError> {
    match vault.get(&SecretRef(name.to_string())) {
        Ok(n) => Ok(n.expose_secret().parse().ok()),
        Err(SecretError::NotFound) => Ok(None),
        Err(e) => Err(map_secret(e)),
    }
}

fn get_large(vault: &Vault, name: &str) -> Result<Option<Zeroizing<String>>, AwsError> {
    let Some(n) = count(vault, name)? else {
        return Ok(None);
    };
    let mut out = Zeroizing::new(String::new());
    for i in 0..n {
        match vault.get(&piece_ref(name, i)) {
            Ok(p) => out.push_str(p.expose_secret()),
            Err(SecretError::NotFound) => return Ok(None),
            Err(e) => return Err(map_secret(e)),
        }
    }
    Ok(Some(out))
}

fn delete_large(vault: &Vault, name: &str) {
    if let Ok(Some(n)) = count(vault, name) {
        for i in 0..n {
            let _ = vault.delete(&piece_ref(name, i));
        }
    }
    let _ = vault.delete(&SecretRef(name.to_string()));
}

fn load<T: DeserializeOwned>(vault: &Vault, name: &str) -> Result<Option<T>, AwsError> {
    Ok(get_large(vault, name)?.and_then(|s| serde_json::from_str(&s).ok()))
}

fn save<T: Serialize>(vault: &Vault, name: &str, v: &T) -> Result<(), AwsError> {
    let json =
        Zeroizing::new(serde_json::to_string(v).map_err(|e| AwsError::Failed(e.to_string()))?);
    put_large(vault, name, &json)
}

async fn oidc(sso_region: &str) -> aws_sdk_ssooidc::Client {
    let cfg = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(sso_region.to_string()))
        .no_credentials()
        .load()
        .await;
    aws_sdk_ssooidc::Client::new(&cfg)
}

/// Login em andamento: mostre [`Self::verification_uri`] e [`Self::user_code`] ao usuário e
/// chame [`complete_login`].
pub struct PendingLogin {
    pub verification_uri: String,
    pub user_code: String,
    start_url: String,
    sso_region: String,
    reg: Registration,
    device_code: SecretString,
    interval: Duration,
    deadline: Instant,
}

impl std::fmt::Debug for PendingLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingLogin")
            .field("user_code", &self.user_code)
            .finish_non_exhaustive()
    }
}

/// Registra o cliente (se preciso) e inicia a autorização do dispositivo.
pub async fn begin_login(
    vault: &Vault,
    start_url: &str,
    sso_region: &str,
) -> Result<PendingLogin, AwsError> {
    let b = base(start_url, sso_region);
    let client = oidc(sso_region).await;
    let reg_name = format!("{b}-client");
    let reg = match load::<Registration>(vault, &reg_name)? {
        Some(r) if r.expires_at > now() + 3600 => r,
        _ => {
            let out = client
                .register_client()
                .client_name(CLIENT_NAME)
                .client_type("public")
                .scopes(SCOPE)
                .send()
                .await
                .map_err(|e| {
                    AwsError::Failed(text::aws_error("RegisterClient", &sdk_detail(&e)))
                })?;
            let reg = Registration {
                client_id: out.client_id.unwrap_or_default(),
                client_secret: out.client_secret.unwrap_or_default(),
                expires_at: out.client_secret_expires_at,
            };
            save(vault, &reg_name, &reg)?;
            reg
        }
    };
    let out = client
        .start_device_authorization()
        .client_id(&reg.client_id)
        .client_secret(&reg.client_secret)
        .start_url(start_url)
        .send()
        .await
        .map_err(|e| {
            AwsError::Failed(text::aws_error("StartDeviceAuthorization", &sdk_detail(&e)))
        })?;
    let uri = out
        .verification_uri_complete
        .or(out.verification_uri)
        .filter(|u| {
            u.starts_with("https://") && !u.chars().any(|c| c.is_whitespace() || c.is_control())
        })
        .ok_or_else(|| AwsError::Failed(text::SSO_BAD_URL().into()))?;
    Ok(PendingLogin {
        verification_uri: uri,
        user_code: myssh_ssh::sanitize_text(&out.user_code.unwrap_or_default(), 32, false),
        start_url: start_url.into(),
        sso_region: sso_region.into(),
        reg,
        device_code: SecretString::from(out.device_code.unwrap_or_default()),
        interval: Duration::from_secs(out.interval.clamp(1, 30) as u64),
        deadline: Instant::now() + Duration::from_secs(out.expires_in.clamp(60, 1800) as u64),
    })
}

/// Espera o usuário confirmar no navegador e guarda o token no cofre.
pub async fn complete_login(vault: &Vault, mut login: PendingLogin) -> Result<(), AwsError> {
    let client = oidc(&login.sso_region).await;
    loop {
        tokio::time::sleep(login.interval).await;
        if Instant::now() > login.deadline {
            return Err(AwsError::Failed(text::SSO_LOGIN_EXPIRED().into()));
        }
        let res = client
            .create_token()
            .client_id(&login.reg.client_id)
            .client_secret(&login.reg.client_secret)
            .grant_type(DEVICE_GRANT)
            .device_code(login.device_code.expose_secret())
            .send()
            .await;
        match res {
            Ok(out) => {
                let token = Token {
                    access_token: out.access_token.unwrap_or_default(),
                    expires_at: now() + i64::from(out.expires_in),
                    refresh_token: out.refresh_token,
                };
                let b = base(&login.start_url, &login.sso_region);
                return save(vault, &format!("{b}-token"), &token);
            }
            Err(e) => {
                let se = e.as_service_error();
                if se.is_some_and(|s| s.is_authorization_pending_exception()) {
                    continue;
                }
                if se.is_some_and(|s| s.is_slow_down_exception()) {
                    login.interval += Duration::from_secs(5);
                    continue;
                }
                if se.is_some_and(|s| s.is_expired_token_exception()) {
                    return Err(AwsError::Failed(text::SSO_LOGIN_EXPIRED().into()));
                }
                if se.is_some_and(|s| s.is_access_denied_exception()) {
                    return Err(AwsError::Failed(text::SSO_LOGIN_DENIED().into()));
                }
                if se.is_some_and(|s| s.is_invalid_client_exception()) {
                    let b = base(&login.start_url, &login.sso_region);
                    delete_large(vault, &format!("{b}-client"));
                }
                return Err(AwsError::Failed(text::aws_error(
                    "CreateToken",
                    &sdk_detail(&e),
                )));
            }
        }
    }
}

/// Token de acesso válido, renovando pelo refresh token se preciso. `LoginRequired` quando não há
/// token utilizável.
async fn access_token(
    vault: &Vault,
    start_url: &str,
    sso_region: &str,
) -> Result<SecretString, AwsError> {
    let b = base(start_url, sso_region);
    let token_name = format!("{b}-token");
    let Some(token) = load::<Token>(vault, &token_name)? else {
        return Err(AwsError::LoginRequired);
    };
    if token.expires_at > now() + SKEW_SECS {
        return Ok(SecretString::from(token.access_token.clone()));
    }
    let (Some(refresh), Some(reg)) = (
        token.refresh_token.as_deref(),
        load::<Registration>(vault, &format!("{b}-client"))?,
    ) else {
        return Err(AwsError::LoginRequired);
    };
    if reg.expires_at <= now() {
        return Err(AwsError::LoginRequired);
    }
    let out = oidc(sso_region)
        .await
        .create_token()
        .client_id(&reg.client_id)
        .client_secret(&reg.client_secret)
        .grant_type("refresh_token")
        .refresh_token(refresh)
        .send()
        .await
        .map_err(|_| AwsError::LoginRequired)?;
    let fresh = Token {
        access_token: out.access_token.unwrap_or_default(),
        expires_at: now() + i64::from(out.expires_in),
        refresh_token: out.refresh_token.or_else(|| token.refresh_token.clone()),
    };
    save(vault, &token_name, &fresh)?;
    Ok(SecretString::from(fresh.access_token.clone()))
}

/// `true` se já há sessão SSO utilizável (renovando se preciso); `false` = precisa de login.
pub async fn session_ready(
    vault: &Vault,
    start_url: &str,
    sso_region: &str,
) -> Result<bool, AwsError> {
    match access_token(vault, start_url, sso_region).await {
        Ok(_) => Ok(true),
        Err(AwsError::LoginRequired) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Esquece a sessão SSO (tokens) do par (start URL, região). O registro do cliente fica.
pub fn logout(vault: &Vault, start_url: &str, sso_region: &str) {
    delete_large(vault, &format!("{}-token", base(start_url, sso_region)));
}

/// Credenciais temporárias da role `role_name` na conta `account_id`.
pub(crate) async fn role_credentials(
    vault: &Vault,
    start_url: &str,
    sso_region: &str,
    account_id: &str,
    role_name: &str,
) -> Result<Credentials, AwsError> {
    let token = access_token(vault, start_url, sso_region).await?;
    let cfg = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(sso_region.to_string()))
        .no_credentials()
        .load()
        .await;
    let out = aws_sdk_sso::Client::new(&cfg)
        .get_role_credentials()
        .account_id(account_id)
        .role_name(role_name)
        .access_token(token.expose_secret())
        .send()
        .await
        .map_err(|e| {
            if e.as_service_error()
                .is_some_and(|s| s.is_unauthorized_exception())
            {
                logout(vault, start_url, sso_region);
                AwsError::LoginRequired
            } else {
                AwsError::Failed(text::aws_error("GetRoleCredentials", &sdk_detail(&e)))
            }
        })?;
    let rc = out
        .role_credentials
        .ok_or_else(|| AwsError::Failed(text::aws_error("GetRoleCredentials", "-")))?;
    let expiry = UNIX_EPOCH + Duration::from_millis(rc.expiration.max(0) as u64);
    Ok(Credentials::new(
        rc.access_key_id.unwrap_or_default(),
        rc.secret_access_key.unwrap_or_default(),
        rc.session_token,
        Some(expiry),
        "myssh-sso",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_secrets::MemoryStore;
    use std::sync::Arc;

    fn vault() -> Vault {
        Vault::new(Arc::new(MemoryStore::new()))
    }

    #[test]
    fn base_estavel_e_distinta() {
        let a = base("https://a.awsapps.com/start", "us-east-1");
        assert_eq!(a, base("https://a.awsapps.com/start", "us-east-1"));
        assert_ne!(a, base("https://a.awsapps.com/start", "eu-west-1"));
        assert!(a.starts_with("aws-sso-") && a.len() == "aws-sso-".len() + 16);
    }

    #[test]
    fn pedacos_grandes_ida_e_volta() {
        let v = vault();
        let big: String = (0..2500)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        put_large(&v, "x", &big).unwrap();
        assert_eq!(count(&v, "x").unwrap(), Some(3));
        assert_eq!(get_large(&v, "x").unwrap().unwrap().as_str(), big);
        // regravar menor apaga os pedaços sobrando
        put_large(&v, "x", "curto").unwrap();
        assert_eq!(get_large(&v, "x").unwrap().unwrap().as_str(), "curto");
        assert!(matches!(
            v.get(&piece_ref("x", 1)),
            Err(SecretError::NotFound)
        ));
        delete_large(&v, "x");
        assert!(get_large(&v, "x").unwrap().is_none());
    }

    #[test]
    fn cofre_bloqueado_vira_vault_locked() {
        let v = vault();
        v.lock();
        assert!(matches!(get_large(&v, "x"), Err(AwsError::VaultLocked)));
    }

    #[tokio::test]
    async fn token_valido_dispensa_login_e_ausente_pede() {
        let v = vault();
        let (url, reg) = ("https://a.awsapps.com/start", "us-east-1");
        assert!(!session_ready(&v, url, reg).await.unwrap());
        let t = Token {
            access_token: "tok".into(),
            expires_at: now() + 3600,
            refresh_token: None,
        };
        save(&v, &format!("{}-token", base(url, reg)), &t).unwrap();
        assert!(session_ready(&v, url, reg).await.unwrap());
        // vencido e sem refresh token: precisa de login (sem tocar a rede)
        let t = Token {
            access_token: "tok".into(),
            expires_at: now() - 10,
            refresh_token: None,
        };
        save(&v, &format!("{}-token", base(url, reg)), &t).unwrap();
        assert!(!session_ready(&v, url, reg).await.unwrap());
        logout(&v, url, reg);
        assert!(
            get_large(&v, &format!("{}-token", base(url, reg)))
                .unwrap()
                .is_none()
        );
    }
}
