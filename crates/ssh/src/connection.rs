//! Conexão SSH autenticada e reutilizável: TCP (ou `direct-tcpip` por jump hosts), handshake, host key,
//! autenticação (senha, chave, agente, keyboard-interactive/MFA) e abertura de canais (shell, SFTP).

use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustVerdict};
use russh::client::{self, AuthResult, KeyboardInteractiveAuthResponse};
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PrivateKeyWithHashAlg, decode_secret_key};
use russh::{ChannelMsg, Disconnect, MethodKind};
use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use crate::error;
use crate::session::{
    AuthResponder, HostKeyDecision, HostKeyResponder, Notify, PtySize, SessionEvent,
};
use crate::trust::host_key_from_public;

/// Tempo que o usuário tem para decidir sobre uma host key desconhecida ou responder a um prompt
/// de autenticação (não conta no `connect_timeout`).
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);
/// Tempo máximo para o desligamento educado ao encerrar.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Keepalives sem resposta antes de considerar a conexão perdida.
const KEEPALIVE_MAX: usize = 3;
/// Rodadas máximas de keyboard-interactive (evita laço infinito com servidor malicioso).
const KI_MAX_ROUNDS: usize = 8;
/// Limites para texto vindo do servidor que vai para a UI.
const KI_MAX_PROMPTS: usize = 16;
const KI_MAX_TEXT: usize = 1000;

// ------------------------------------------------------------------ tipos públicos

pub enum AuthMaterial {
    Password(SecretString),
    KeyFile {
        path: PathBuf,
        passphrase: Option<SecretString>,
    },
    /// Agente SSH local, sem forwarding: `SSH_AUTH_SOCK` (Unix) ou, no Windows, o named pipe do
    /// OpenSSH (`\\.\pipe\openssh-ssh-agent`) e depois o Pageant (Windows: **não testado**).
    Agent,
    /// keyboard-interactive (MFA): cada pedido do servidor vira um [`SessionEvent::AuthPrompt`].
    /// As respostas nunca são persistidas nem registradas.
    KeyboardInteractive,
}

impl AuthMaterial {
    /// Descarta a credencial (os `SecretString` se zeram ao cair) e deixa `Agent`, que não carrega
    /// segredo. Usado logo depois de a autenticação concluir.
    pub(crate) fn scrub(&mut self) {
        *self = AuthMaterial::Agent;
    }

    /// `true` se ainda guarda segredo em memória (senha ou passphrase).
    #[cfg(test)]
    pub(crate) fn holds_secret(&self) -> bool {
        matches!(
            self,
            AuthMaterial::Password(_)
                | AuthMaterial::KeyFile {
                    passphrase: Some(_),
                    ..
                }
        )
    }
}

impl std::fmt::Debug for AuthMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMaterial::Password(_) => f.write_str("Password([REDACTED])"),
            AuthMaterial::KeyFile { path, passphrase } => f
                .debug_struct("KeyFile")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| "[REDACTED]"))
                .finish(),
            AuthMaterial::Agent => f.write_str("Agent"),
            AuthMaterial::KeyboardInteractive => f.write_str("KeyboardInteractive"),
        }
    }
}

/// Um salto (bastion) da cadeia de jump hosts. Cada salto valida a **própria** host key contra o
/// `HostTrustStore` (com o endpoint como o usuário configurou) e autentica com as próprias credenciais.
#[derive(Debug)]
pub struct Hop {
    pub endpoint: HostEndpoint,
    pub username: String,
    pub auth: AuthMaterial,
    /// Orçamento para TCP/`direct-tcpip` + handshake + autenticação deste salto.
    pub connect_timeout: Duration,
}

/// O que [`Connection::connect`] usa de um [`crate::ConnectRequest`]: `endpoint`, `username`, `auth`,
/// `connect_timeout`, `keepalive` e `jumps`. (`pty`/`term` só valem em [`Connection::open_shell`].)
pub(crate) struct Target<'a> {
    pub endpoint: &'a HostEndpoint,
    pub username: &'a str,
    pub auth: &'a AuthMaterial,
    pub connect_timeout: Duration,
    pub keepalive: Option<Duration>,
    pub jumps: &'a [Hop],
}

/// Destino dos eventos de uma conexão/sessão (canal limitado: backpressure).
#[derive(Clone)]
pub struct EventSink {
    tx: mpsc::Sender<SessionEvent>,
    notify: Notify,
}

impl EventSink {
    pub fn new(tx: mpsc::Sender<SessionEvent>, notify: Notify) -> Self {
        Self { tx, notify }
    }

    /// `false` se a UI foi embora. Com o canal cheio, espera (backpressure).
    pub(crate) async fn send(&self, ev: SessionEvent) -> bool {
        let ok = self.tx.send(ev).await.is_ok();
        (self.notify)();
        ok
    }

    pub(crate) async fn state(&self, s: SessionState) -> bool {
        self.send(SessionEvent::State(s)).await
    }
}

// ------------------------------------------------------------------ configuração

pub(crate) fn client_config(keepalive: Option<Duration>) -> Arc<client::Config> {
    let mut preferred = russh::Preferred::DEFAULT;
    // O padrão do russh inclui `ssh-rsa` (SHA-1); não habilitamos.
    preferred.key = Cow::Borrowed(&[
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        },
    ]);
    Arc::new(client::Config {
        preferred,
        keepalive_interval: keepalive,
        keepalive_max: KEEPALIVE_MAX,
        nodelay: true,
        ..Default::default()
    })
}

// ------------------------------------------------------------------ orçamento de tempo

/// Executa `fut` consumindo `budget`; o tempo em que `paused` está ligado não é cobrado.
pub(crate) async fn with_budget<F: Future>(
    fut: F,
    budget: &mut Duration,
    paused: &AtomicBool,
) -> Result<F::Output, ()> {
    tokio::pin!(fut);
    let mut last = Instant::now();
    let mut iv = tokio::time::interval(Duration::from_millis(25));
    loop {
        tokio::select! {
            r = &mut fut => {
                if !paused.load(Ordering::Relaxed) {
                    *budget = budget.saturating_sub(last.elapsed());
                }
                return Ok(r);
            }
            _ = iv.tick() => {
                let now = Instant::now();
                let dt = now - last;
                last = now;
                if !paused.load(Ordering::Relaxed) {
                    *budget = budget.saturating_sub(dt);
                    if budget.is_zero() {
                        return Err(());
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ handler (host key)

pub(crate) struct Handler {
    endpoint: HostEndpoint,
    /// `Some(i)` = i-ésimo jump host (0 = o mais próximo do cliente); `None` = destino final.
    hop: Option<usize>,
    trust: Arc<dyn HostTrustStore>,
    emit: EventSink,
    paused: Arc<AtomicBool>,
    /// Motivo (já sanitizado) pelo qual a host key foi recusada.
    abort: Arc<Mutex<Option<String>>>,
}

impl Handler {
    fn reject(&self, msg: &str) -> Result<bool, russh::Error> {
        if let Ok(mut g) = self.abort.lock() {
            *g = Some(msg.to_string());
        }
        Ok(false)
    }

    async fn verdict(&self, key: &HostKey) -> Result<TrustVerdict, ()> {
        let (trust, ep, key) = (self.trust.clone(), self.endpoint.clone(), key.clone());
        tokio::task::spawn_blocking(move || trust.verify(&ep, &key))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }

    async fn save(&self, key: &HostKey) -> Result<(), ()> {
        let (trust, ep, key) = (self.trust.clone(), self.endpoint.clone(), key.clone());
        tokio::task::spawn_blocking(move || trust.trust(&ep, &key))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return self.reject(error::HOST_CERT());
        };
        let Some(presented) = host_key_from_public(key) else {
            return self.reject(error::HOST_STORE());
        };
        let Ok(verdict) = self.verdict(&presented).await else {
            return self.reject(error::HOST_STORE());
        };
        let known = match verdict {
            TrustVerdict::Trusted => return Ok(true),
            TrustVerdict::Unknown => None,
            TrustVerdict::Changed { previous } => Some(vec![previous]),
            TrustVerdict::OtherAlgorithmKnown { known } => Some(known),
            TrustVerdict::Revoked => Some(Vec::new()),
        };
        if let Some(known) = known {
            let _ = self
                .emit
                .send(SessionEvent::HostKeyChanged {
                    endpoint: self.endpoint.clone(),
                    hop: self.hop,
                    presented,
                    known,
                })
                .await;
            return self.reject(error::HOST_CHANGED());
        }

        // Desconhecido: pergunta ao usuário (relógio do connect_timeout pausado).
        self.paused.store(true, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let sent = self.emit.state(SessionState::AwaitingHostTrust).await
            && self
                .emit
                .send(SessionEvent::HostKeyUnknown {
                    endpoint: self.endpoint.clone(),
                    hop: self.hop,
                    key: presented.clone(),
                    responder: HostKeyResponder::new(tx),
                })
                .await;
        if !sent {
            return self.reject(error::UI_GONE());
        }
        let answer = tokio::time::timeout(PROMPT_TIMEOUT, rx).await;
        self.paused.store(false, Ordering::Relaxed);
        match answer {
            Ok(Ok(HostKeyDecision::AcceptOnce)) => Ok(true),
            Ok(Ok(HostKeyDecision::AcceptAndSave)) => {
                if self.save(&presented).await.is_ok() {
                    Ok(true)
                } else {
                    self.reject(error::HOST_STORE())
                }
            }
            Ok(Ok(HostKeyDecision::Reject)) | Ok(Err(_)) => self.reject(error::HOST_REJECTED()),
            Err(_) => self.reject(error::HOST_PROMPT_TIMEOUT()),
        }
    }
}

type Handle = client::Handle<Handler>;
type Chan = russh::Channel<client::Msg>;

// ------------------------------------------------------------------ chaves e agente

/// Tamanho máximo aceito para um arquivo de chave privada (chaves reais têm poucos KiB).
const MAX_KEY_FILE: u64 = 64 * 1024;

/// Lê o arquivo da chave: só arquivo comum (`metadata` segue symlinks) de até [`MAX_KEY_FILE`].
/// A checagem de `metadata` vem **antes** do `open`, pois abrir um FIFO bloquearia; `/dev/zero`
/// e afins não são `is_file`. Limitação: a troca do arquivo por um FIFO entre a checagem e o
/// `open` (TOCTOU) exigiria atacante local com escrita na pasta da chave; a leitura em si é
/// sempre limitada por `take`.
fn read_key_file(path: &std::path::Path) -> Result<Zeroizing<String>, String> {
    use std::io::Read;
    let meta = std::fs::metadata(path).map_err(|_| error::KEY_READ().to_string())?;
    if !meta.is_file() {
        return Err(error::KEY_NOT_FILE().to_string());
    }
    if meta.len() > MAX_KEY_FILE {
        return Err(error::KEY_TOO_BIG().to_string());
    }
    let file = std::fs::File::open(path).map_err(|_| error::KEY_READ().to_string())?;
    let mut buf = Zeroizing::new(Vec::new());
    file.take(MAX_KEY_FILE + 1)
        .read_to_end(&mut buf)
        .map_err(|_| error::KEY_READ().to_string())?;
    if buf.len() as u64 > MAX_KEY_FILE {
        return Err(error::KEY_TOO_BIG().to_string());
    }
    let text = String::from_utf8(std::mem::take(&mut *buf))
        .map_err(|_| error::KEY_DECODE().to_string())?;
    Ok(Zeroizing::new(text))
}

async fn load_key(
    path: &std::path::Path,
    passphrase: Option<&SecretString>,
) -> Result<russh::keys::PrivateKey, String> {
    let p = path.to_path_buf();
    let text = tokio::task::spawn_blocking(move || read_key_file(&p))
        .await
        .map_err(|_| error::KEY_READ().to_string())??;
    decode_secret_key(&text, passphrase.map(|p| p.expose_secret()))
        .map_err(|_| error::KEY_DECODE().to_string())
}

/// Diz se a chave privada em `path` exige passphrase (leitura síncrona: chamar fora da thread
/// de UI). Erros são mensagens pt-BR sanitizadas.
pub fn key_requires_passphrase(path: &std::path::Path) -> Result<bool, String> {
    let text = read_key_file(path)?;
    match decode_secret_key(&text, None) {
        Ok(_) => Ok(false),
        Err(russh::keys::Error::KeyIsEncrypted) => Ok(true),
        Err(_) => Err(error::KEY_DECODE().to_string()),
    }
}

/// RSA nunca usa SHA-1: se o servidor não anunciar rsa-sha2, tentamos SHA-256.
async fn rsa_hash(session: &Handle, key_alg: &Algorithm) -> Option<HashAlg> {
    if !matches!(key_alg, Algorithm::Rsa { .. }) {
        return None;
    }
    let best = session.best_supported_rsa_hash().await.ok().flatten();
    best.flatten().or(Some(HashAlg::Sha256))
}

type DynAgent = russh::keys::agent::client::AgentClient<
    Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
>;

#[cfg(unix)]
async fn connect_agent() -> Result<DynAgent, String> {
    use russh::keys::agent::client::AgentClient;
    let agent = AgentClient::connect_env()
        .await
        .map_err(|_| error::NO_AGENT().to_string())?;
    Ok(agent.dynamic())
}

/// Windows (NÃO TESTADO): named pipe do OpenSSH for Windows e, se não houver, o Pageant.
#[cfg(windows)]
async fn connect_agent() -> Result<DynAgent, String> {
    use russh::keys::agent::client::AgentClient;
    if let Ok(a) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        return Ok(a.dynamic());
    }
    AgentClient::connect_pageant()
        .await
        .map(|a| a.dynamic())
        .map_err(|_| error::NO_AGENT().to_string())
}

#[cfg(not(any(unix, windows)))]
async fn connect_agent() -> Result<DynAgent, String> {
    Err(error::UNSUPPORTED_AUTH().into())
}

/// Tenta cada identidade do agente. `Success` ou a primeira falha **parcial** (chave aceita, falta
/// outro fator) são devolvidas; sem nenhuma chave aceita, erro.
async fn authenticate_agent(session: &mut Handle, user: &str) -> Result<AuthResult, String> {
    use russh::keys::agent::AgentIdentity;

    let mut agent = connect_agent().await?;
    let ids = agent
        .request_identities()
        .await
        .map_err(|_| error::NO_AGENT().to_string())?;
    for id in ids {
        let AgentIdentity::PublicKey { key, .. } = id else {
            continue;
        };
        let hash = rsa_hash(session, &key.algorithm()).await;
        if let Ok(r) = session
            .authenticate_publickey_with(user.to_string(), key, hash, &mut agent)
            .await
        {
            match &r {
                AuthResult::Success => return Ok(r),
                AuthResult::Failure {
                    partial_success: true,
                    ..
                } => return Ok(r),
                _ => {}
            }
        }
    }
    Err(error::AGENT_NO_KEYS().into())
}

// ------------------------------------------------------------------ keyboard-interactive

struct AuthCtx<'a> {
    emit: &'a EventSink,
    paused: &'a AtomicBool,
    hop: Option<usize>,
}

#[derive(Clone, Copy)]
enum KiMode<'a> {
    /// Todo pedido vira diálogo.
    Interactive,
    /// A senha foi recusada mas o servidor não oferece mais `password`: o prompt "Password:" simples
    /// do keyboard-interactive é respondido (uma vez) com a própria senha do perfil.
    AutoPassword(&'a SecretString),
    /// A senha foi recusada e `password` ainda é aceito: um prompt "Password:" simples só repetiria
    /// a senha errada; encerra sem perguntar de novo.
    SkipPlainPassword,
}

/// Texto vindo do servidor (nome/instruções/prompt): sanitizador único com o título do terminal
/// (controles, bidi, zero-width) e limite de tamanho.
fn clean(s: &str, keep_newlines: bool) -> String {
    crate::sanitize::sanitize_text(s, KI_MAX_TEXT, keep_newlines)
}

/// "Password:", "user@host's password:", "Senha:" (e nada além disso).
fn is_plain_password_prompt(prompts: &[(String, bool)]) -> bool {
    let [(text, echo)] = prompts else {
        return false;
    };
    if *echo {
        return false;
    }
    let t = text.trim().trim_end_matches(':').trim().to_lowercase();
    (t.ends_with("password") || t.ends_with("senha")) && t.split_whitespace().count() <= 4
}

async fn ask_user(
    ctx: &AuthCtx<'_>,
    name: &str,
    instructions: &str,
    prompts: Vec<(String, bool)>,
) -> Result<Vec<SecretString>, String> {
    ctx.paused.store(true, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    let sent = ctx.emit.state(SessionState::AwaitingAuth).await
        && ctx
            .emit
            .send(SessionEvent::AuthPrompt {
                hop: ctx.hop,
                name: name.to_string(),
                instructions: instructions.to_string(),
                prompts,
                responder: AuthResponder::new(tx),
            })
            .await;
    if !sent {
        ctx.paused.store(false, Ordering::Relaxed);
        return Err(error::UI_GONE().into());
    }
    let answer = tokio::time::timeout(PROMPT_TIMEOUT, rx).await;
    ctx.paused.store(false, Ordering::Relaxed);
    match answer {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(_)) => Err(error::AUTH_CANCELLED().into()),
        Err(_) => Err(error::AUTH_PROMPT_TIMEOUT().into()),
    }
}

/// Executa keyboard-interactive. `Ok(true)` = autenticado; `Ok(false)` = servidor recusou.
async fn keyboard_interactive(
    session: &mut Handle,
    user: &str,
    ctx: &AuthCtx<'_>,
    mode: KiMode<'_>,
) -> Result<bool, String> {
    let to_err = |e: russh::Error| error::russh_message(&e);
    let mut resp = session
        .authenticate_keyboard_interactive_start(user.to_string(), None)
        .await
        .map_err(to_err)?;
    let mut auto_used = false;
    for _ in 0..KI_MAX_ROUNDS {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                if prompts.len() > KI_MAX_PROMPTS {
                    return Err(error::KI_PROTOCOL().into());
                }
                let prompts: Vec<(String, bool)> = prompts
                    .into_iter()
                    .map(|p| (clean(&p.prompt, false), p.echo))
                    .collect();
                let answers: Vec<String> = if prompts.is_empty() {
                    Vec::new() // rodada informativa sem perguntas
                } else if is_plain_password_prompt(&prompts)
                    && matches!(mode, KiMode::SkipPlainPassword)
                {
                    return Ok(false);
                } else if let (true, KiMode::AutoPassword(pw), false) =
                    (is_plain_password_prompt(&prompts), mode, auto_used)
                {
                    auto_used = true;
                    vec![pw.expose_secret().to_string()]
                } else {
                    let n = prompts.len();
                    let mut v = ask_user(
                        ctx,
                        &clean(&name, false),
                        &clean(&instructions, true),
                        prompts,
                    )
                    .await?;
                    v.resize_with(n, || SecretString::from(String::new()));
                    // O `String` entregue ao russh é consumido por ele (não zera ao enviar:
                    // limitação da biblioteca). Nossa cópia segura (`v`) cai e zera aqui mesmo.
                    let answers: Vec<String> =
                        v.iter().map(|s| s.expose_secret().to_string()).collect();
                    drop(v);
                    answers
                };
                resp = session
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(to_err)?;
            }
        }
    }
    Err(error::KI_PROTOCOL().into())
}

async fn authenticate(
    session: &mut Handle,
    user: &str,
    auth: &AuthMaterial,
    ctx: &AuthCtx<'_>,
) -> Result<(), String> {
    let to_err = |e: russh::Error| error::russh_message(&e);
    let res = match auth {
        AuthMaterial::Password(pw) => session
            .authenticate_password(user.to_string(), pw.expose_secret().to_string())
            .await
            .map_err(to_err)?,
        AuthMaterial::KeyFile { path, passphrase } => {
            let key = load_key(path, passphrase.as_ref()).await?;
            let hash = rsa_hash(session, &key.algorithm()).await;
            session
                .authenticate_publickey(
                    user.to_string(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(to_err)?
        }
        AuthMaterial::Agent => authenticate_agent(session, user).await?,
        AuthMaterial::KeyboardInteractive => {
            return match keyboard_interactive(session, user, ctx, KiMode::Interactive).await? {
                true => Ok(()),
                false => Err(error::AUTH_REJECTED().into()),
            };
        }
    };
    let AuthResult::Failure {
        remaining_methods,
        partial_success,
    } = res
    else {
        return Ok(());
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive) {
        return Err(error::AUTH_REJECTED().into());
    }
    // O servidor aceitou este fator mas exige outro (ex.: `publickey,keyboard-interactive`): segue
    // para keyboard-interactive. Senha recusada com keyboard-interactive disponível: tenta KI.
    let mode = match auth {
        _ if partial_success => KiMode::Interactive,
        AuthMaterial::Password(_) if remaining_methods.contains(&MethodKind::Password) => {
            KiMode::SkipPlainPassword
        }
        AuthMaterial::Password(pw) => KiMode::AutoPassword(pw),
        // Chave/agente simplesmente não aceitos: não pedir nada ao usuário.
        _ => return Err(error::AUTH_REJECTED().into()),
    };
    match keyboard_interactive(session, user, ctx, mode).await? {
        true => Ok(()),
        false => Err(error::AUTH_REJECTED().into()),
    }
}

// ------------------------------------------------------------------ conexão (com jump hosts)

/// Handshake SSH sobre `stream` (verifica a host key ANTES de autenticar) e autenticação.
#[allow(clippy::too_many_arguments)]
async fn handshake_and_auth<R>(
    stream: R,
    endpoint: &HostEndpoint,
    hop: Option<usize>,
    username: &str,
    auth: &AuthMaterial,
    keepalive: Option<Duration>,
    budget: &mut Duration,
    trust: &Arc<dyn HostTrustStore>,
    emit: &EventSink,
) -> Result<Handle, String>
where
    R: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let timeout = || error::at_hop(hop, error::TIMEOUT().to_string());
    let paused = Arc::new(AtomicBool::new(false));
    let abort = Arc::new(Mutex::new(None));
    let handler = Handler {
        endpoint: endpoint.clone(),
        hop,
        trust: trust.clone(),
        emit: emit.clone(),
        paused: paused.clone(),
        abort: abort.clone(),
    };
    let mut session = match with_budget(
        client::connect_stream(client_config(keepalive), stream, handler),
        budget,
        &paused,
    )
    .await
    .map_err(|_| timeout())?
    {
        Ok(s) => s,
        Err(e) => {
            let reason = abort.lock().ok().and_then(|g| g.clone());
            return Err(error::at_hop(
                hop,
                reason.unwrap_or_else(|| error::russh_message(&e)),
            ));
        }
    };
    if !emit.state(SessionState::AwaitingAuth).await {
        return Err(error::UI_GONE().into());
    }
    let ctx = AuthCtx {
        emit,
        paused: &paused,
        hop,
    };
    with_budget(
        authenticate(&mut session, username, auth, &ctx),
        budget,
        &paused,
    )
    .await
    .map_err(|_| timeout())?
    .map_err(|m| error::at_hop(hop, m))?;
    Ok(session)
}

/// Conexão SSH autenticada, compartilhável (`Arc`), da qual se abrem canais ([`Self::open_shell`],
/// [`Self::open_sftp`]). Com jump hosts, mantém vivas as conexões intermediárias.
///
/// **Ciclo de vida:** a conexão é encerrada quando o **último canal aberto** é solto (a contagem só
/// dispara depois que algum canal foi aberto) ou, explicitamente, por [`Self::close`]. Depois disso
/// não aceita novos canais. Agent forwarding nunca é solicitado.
pub struct Connection {
    // A ordem dos campos importa: o destino cai antes dos saltos que o carregam.
    handle: Handle,
    parents: Vec<Handle>,
    open_channels: AtomicUsize,
    closed: AtomicBool,
    rt: tokio::runtime::Handle,
}

struct ChannelGuard(Arc<Connection>);

impl Drop for ChannelGuard {
    fn drop(&mut self) {
        if self.0.open_channels.fetch_sub(1, Ordering::AcqRel) == 1 {
            let c = self.0.clone();
            self.0.rt.spawn(async move { c.close().await });
        }
    }
}

impl Connection {
    /// Conecta (passando por todos os saltos de `jumps`, em ordem) e autentica em cada um.
    /// Emite `State`, `HostKeyUnknown`/`HostKeyChanged` e `AuthPrompt` em `emit`; os eventos de salto
    /// trazem `hop = Some(índice)`. Erros são mensagens pt-BR sanitizadas (de salto: "Jump host N: ...").
    pub async fn connect(
        req: &crate::ConnectRequest,
        trust: &Arc<dyn HostTrustStore>,
        emit: &EventSink,
    ) -> Result<Arc<Connection>, String> {
        Self::connect_target(
            Target {
                endpoint: &req.endpoint,
                username: &req.username,
                auth: &req.auth,
                connect_timeout: req.connect_timeout,
                keepalive: req.keepalive,
                jumps: &req.jumps,
            },
            trust,
            emit,
        )
        .await
    }

    async fn connect_target(
        t: Target<'_>,
        trust: &Arc<dyn HostTrustStore>,
        emit: &EventSink,
    ) -> Result<Arc<Connection>, String> {
        // Ciclo: o mesmo host:porta duas vezes na cadeia (incluindo o destino).
        let mut seen: Vec<&HostEndpoint> = Vec::new();
        for ep in t
            .jumps
            .iter()
            .map(|h| &h.endpoint)
            .chain(std::iter::once(t.endpoint))
        {
            if seen.contains(&ep) {
                return Err(error::JUMP_LOOP().into());
            }
            seen.push(ep);
        }

        let mut chain: Vec<Handle> = Vec::new();
        let n = t.jumps.len();
        for i in 0..=n {
            let (endpoint, username, auth, total, hop) = if i < n {
                let h = &t.jumps[i];
                (
                    &h.endpoint,
                    h.username.as_str(),
                    &h.auth,
                    h.connect_timeout,
                    Some(i),
                )
            } else {
                (
                    t.endpoint,
                    t.username,
                    t.auth,
                    t.connect_timeout,
                    None::<usize>,
                )
            };
            if i > 0 && !emit.state(SessionState::Connecting).await {
                return Err(error::UI_GONE().into());
            }
            let mut budget = total;
            let nopause = AtomicBool::new(false);
            let timeout = || error::at_hop(hop, error::TIMEOUT().to_string());
            let handle = match chain.last() {
                None => {
                    let stream = with_budget(
                        TcpStream::connect((endpoint.host.as_str(), endpoint.port)),
                        &mut budget,
                        &nopause,
                    )
                    .await
                    .map_err(|_| timeout())?
                    .map_err(|e| error::at_hop(hop, error::io_message(&e)))?;
                    let _ = stream.set_nodelay(true);
                    handshake_and_auth(
                        stream,
                        endpoint,
                        hop,
                        username,
                        auth,
                        t.keepalive,
                        &mut budget,
                        trust,
                        emit,
                    )
                    .await?
                }
                Some(prev) => {
                    // O próximo servidor é alcançado A PARTIR do salto anterior (resolução de nome
                    // incluída); o handshake e a verificação de host key são do próprio servidor.
                    let channel = with_budget(
                        prev.channel_open_direct_tcpip(
                            endpoint.host.clone(),
                            u32::from(endpoint.port),
                            "127.0.0.1",
                            0,
                        ),
                        &mut budget,
                        &nopause,
                    )
                    .await
                    .map_err(|_| timeout())?
                    .map_err(|_| error::at_hop(Some(i - 1), error::JUMP_OPEN().to_string()))?;
                    handshake_and_auth(
                        channel.into_stream(),
                        endpoint,
                        hop,
                        username,
                        auth,
                        t.keepalive,
                        &mut budget,
                        trust,
                        emit,
                    )
                    .await?
                }
            };
            chain.push(handle);
        }
        let handle = chain
            .pop()
            .ok_or_else(|| error::CONN_CLOSED().to_string())?;
        Ok(Arc::new(Connection {
            handle,
            parents: chain,
            open_channels: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            rt: tokio::runtime::Handle::current(),
        }))
    }

    /// `true` se a conexão foi encerrada (explicitamente, pelo último canal ou pela rede).
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.handle.is_closed()
    }

    /// Canais abertos no momento.
    pub fn open_channels(&self) -> usize {
        self.open_channels.load(Ordering::Acquire)
    }

    /// Encerra a conexão (e os saltos). Idempotente.
    pub async fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for h in std::iter::once(&self.handle).chain(self.parents.iter().rev()) {
            let _ = tokio::time::timeout(
                DISCONNECT_TIMEOUT,
                h.disconnect(Disconnect::ByApplication, "", "en"),
            )
            .await;
        }
    }

    async fn open_session_channel(self: &Arc<Self>) -> Result<(Chan, ChannelGuard), String> {
        if self.is_closed() {
            return Err(error::CONN_CLOSED().into());
        }
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| error::russh_message(&e))?;
        self.open_channels.fetch_add(1, Ordering::AcqRel);
        Ok((channel, ChannelGuard(self.clone())))
    }

    /// Abre um canal de sessão com PTY e shell.
    pub async fn open_shell(
        self: &Arc<Self>,
        pty: PtySize,
        term: &str,
    ) -> Result<ShellChannel, String> {
        let (channel, guard) = self.open_session_channel().await?;
        let e = |e: russh::Error| error::russh_message(&e);
        channel
            .request_pty(
                false,
                term,
                pty.cols as u32,
                pty.rows as u32,
                pty.pix_w as u32,
                pty.pix_h as u32,
                &[],
            )
            .await
            .map_err(e)?;
        channel.request_shell(false).await.map_err(e)?;
        Ok(ShellChannel {
            channel,
            _guard: guard,
        })
    }

    /// Abre o subsistema SFTP num canal novo. O resultado derefere para
    /// [`russh_sftp::client::SftpSession`].
    pub async fn open_sftp(self: &Arc<Self>) -> Result<Sftp, String> {
        let (channel, guard) = self.open_session_channel().await?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| error::russh_message(&e))?;
        let session = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|_| error::SFTP_FAILED().to_string())?;
        Ok(Sftp {
            session,
            _guard: guard,
        })
    }
}

/// Canal com PTY e shell. Soltá-lo fecha o canal (e a conexão, se for o último).
pub struct ShellChannel {
    channel: Chan,
    _guard: ChannelGuard,
}

impl ShellChannel {
    pub async fn data(&self, bytes: &[u8]) -> Result<(), String> {
        self.channel
            .data(bytes)
            .await
            .map_err(|_| error::CONNECTION_LOST().to_string())
    }

    pub async fn window_change(&self, size: PtySize) -> Result<(), String> {
        self.channel
            .window_change(
                size.cols as u32,
                size.rows as u32,
                size.pix_w as u32,
                size.pix_h as u32,
            )
            .await
            .map_err(|_| error::CONNECTION_LOST().to_string())
    }

    /// Próxima mensagem do canal (`None` = canal fechado).
    pub async fn wait(&mut self) -> Option<ChannelMsg> {
        self.channel.wait().await
    }
}

/// Sessão SFTP sobre um canal da [`Connection`] (deref para `SftpSession`).
pub struct Sftp {
    session: russh_sftp::client::SftpSession,
    _guard: ChannelGuard,
}

impl std::ops::Deref for Sftp {
    type Target = russh_sftp::client::SftpSession;
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl std::ops::DerefMut for Sftp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_sem_algoritmos_legados() {
        let c = client_config(Some(Duration::from_secs(10)));
        let keys: Vec<String> = c.preferred.key.iter().map(|a| a.to_string()).collect();
        assert!(
            !keys.iter().any(|k| k == "ssh-rsa" || k.contains("dss")),
            "{keys:?}"
        );
        assert!(keys.iter().any(|k| k == "ssh-ed25519"));
        for c in c.preferred.cipher.iter() {
            assert!(!c.as_ref().contains("cbc"), "{}", c.as_ref());
        }
        for m in c.preferred.mac.iter() {
            assert!(!m.as_ref().contains("sha1"), "{}", m.as_ref());
        }
        assert_eq!(c.keepalive_interval, Some(Duration::from_secs(10)));
    }

    #[test]
    fn debug_nao_vaza_segredo() {
        let a = AuthMaterial::Password(SecretString::from("hunter2".to_string()));
        assert!(!format!("{a:?}").contains("hunter2"));
    }

    #[tokio::test]
    async fn orcamento_pausado_nao_conta() {
        let paused = AtomicBool::new(true);
        let mut b = Duration::from_millis(100);
        let r = with_budget(
            tokio::time::sleep(Duration::from_millis(300)),
            &mut b,
            &paused,
        )
        .await;
        assert!(r.is_ok());
        assert_eq!(b, Duration::from_millis(100));
        let paused = AtomicBool::new(false);
        let mut b = Duration::from_millis(100);
        let r = with_budget(tokio::time::sleep(Duration::from_secs(5)), &mut b, &paused).await;
        assert!(r.is_err());
    }

    fn p(t: &str, echo: bool) -> Vec<(String, bool)> {
        vec![(t.to_string(), echo)]
    }

    #[test]
    fn prompt_de_senha_simples() {
        assert!(is_plain_password_prompt(&p("Password: ", false)));
        assert!(is_plain_password_prompt(&p(
            "tester@host's password:",
            false
        )));
        assert!(is_plain_password_prompt(&p("Senha:", false)));
        assert!(!is_plain_password_prompt(&p("Verification code:", false)));
        assert!(!is_plain_password_prompt(&p("Password + OTP:", false)));
        assert!(!is_plain_password_prompt(&p("Password: ", true)));
        let mut two = p("Password:", false);
        two.extend(p("Code:", false));
        assert!(!is_plain_password_prompt(&two));
    }

    #[test]
    fn texto_do_servidor_sem_controles() {
        assert_eq!(clean("a\x1b[31mb\u{7}c\nd", false), "a[31mbcd");
        assert_eq!(clean("a\nb", true), "a\nb");
        assert_eq!(clean(&"x".repeat(5000), false).len(), KI_MAX_TEXT);
    }

    #[test]
    fn credenciais_sao_descartadas_apos_autenticar() {
        let mut a = AuthMaterial::Password(SecretString::from("segredo".to_string()));
        assert!(a.holds_secret());
        a.scrub();
        assert!(!a.holds_secret());
        let mut k = AuthMaterial::KeyFile {
            path: "/x".into(),
            passphrase: Some(SecretString::from("frase".to_string())),
        };
        assert!(k.holds_secret());
        k.scrub();
        assert!(!k.holds_secret());
        let mut req = crate::ConnectRequest {
            endpoint: HostEndpoint::new("h", 22),
            username: "u".into(),
            auth: AuthMaterial::Password(SecretString::from("a".to_string())),
            pty: crate::PtySize {
                cols: 80,
                rows: 24,
                pix_w: 0,
                pix_h: 0,
            },
            term: "xterm".into(),
            connect_timeout: Duration::from_secs(1),
            keepalive: None,
            jumps: vec![Hop {
                endpoint: HostEndpoint::new("j", 22),
                username: "u".into(),
                auth: AuthMaterial::Password(SecretString::from("b".to_string())),
                connect_timeout: Duration::from_secs(1),
            }],
        };
        req.scrub_credentials();
        assert!(!req.auth.holds_secret() && !req.jumps[0].auth.holds_secret());
    }

    #[test]
    fn prompt_do_servidor_sem_bidi_nem_zero_width() {
        // "Password:" disfarçado com override RTL e caracteres invisíveis.
        assert_eq!(
            clean("Pass\u{202E}word\u{200B}:\u{FEFF}\u{2066}", false),
            "Password:"
        );
        assert_eq!(clean("a\u{202D}b\nc", true), "ab\nc");
    }

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("myssh-ssh-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn chave_diretorio_e_arquivo_grande_sao_recusados() {
        let d = tmp_dir("key-big");
        let e = key_requires_passphrase(&d).unwrap_err();
        assert_eq!(e, error::KEY_NOT_FILE());
        let big = d.join("big");
        std::fs::write(&big, vec![b'A'; 200 * 1024]).unwrap();
        assert_eq!(
            key_requires_passphrase(&big).unwrap_err(),
            error::KEY_TOO_BIG()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn chave_fifo_e_dev_zero_nao_travam() {
        let d = tmp_dir("key-fifo");
        let fifo = d.join("fifo");
        let st = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(st.success());
        // Antes da correção `read_to_string` bloqueava para sempre no FIFO: roda numa thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let f = fifo.clone();
        std::thread::spawn(move || {
            let _ = tx.send(key_requires_passphrase(&f));
        });
        let r = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("FIFO travou a leitura da chave");
        assert_eq!(r.unwrap_err(), error::KEY_NOT_FILE());
        assert_eq!(
            key_requires_passphrase(std::path::Path::new("/dev/zero")).unwrap_err(),
            error::KEY_NOT_FILE()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn ciclo_de_saltos_e_erro() {
        let ep = HostEndpoint::new("a.example", 22);
        let jumps = vec![Hop {
            endpoint: ep.clone(),
            username: "u".into(),
            auth: AuthMaterial::Agent,
            connect_timeout: Duration::from_secs(1),
        }];
        let (tx, _rx) = mpsc::channel(4);
        let emit = EventSink::new(tx, Arc::new(|| {}));
        struct NoTrust;
        impl HostTrustStore for NoTrust {
            fn verify(
                &self,
                _: &HostEndpoint,
                _: &HostKey,
            ) -> Result<TrustVerdict, myssh_core::trust::TrustStoreError> {
                Ok(TrustVerdict::Unknown)
            }
            fn trust(
                &self,
                _: &HostEndpoint,
                _: &HostKey,
            ) -> Result<(), myssh_core::trust::TrustStoreError> {
                Ok(())
            }
            fn replace(
                &self,
                _: &HostEndpoint,
                _: &HostKey,
            ) -> Result<(), myssh_core::trust::TrustStoreError> {
                Ok(())
            }
            fn forget(&self, _: &HostEndpoint) -> Result<(), myssh_core::trust::TrustStoreError> {
                Ok(())
            }
            fn list(
                &self,
            ) -> Result<Vec<(HostEndpoint, HostKey)>, myssh_core::trust::TrustStoreError>
            {
                Ok(vec![])
            }
        }
        let trust: Arc<dyn HostTrustStore> = Arc::new(NoTrust);
        let auth = AuthMaterial::Agent;
        let r = Connection::connect_target(
            Target {
                endpoint: &ep,
                username: "u",
                auth: &auth,
                connect_timeout: Duration::from_secs(1),
                keepalive: None,
                jumps: &jumps,
            },
            &trust,
            &emit,
        )
        .await;
        assert_eq!(r.err().as_deref(), Some(error::JUMP_LOOP()));
    }
}
