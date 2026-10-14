//! Sessão SSH: conexão, verificação de host key, autenticação, PTY e laço de E/S.

use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustVerdict};
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PrivateKeyWithHashAlg, decode_secret_key};
use russh::{ChannelMsg, Disconnect, client};
use secrecy::{ExposeSecret, SecretString};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use crate::error;
use crate::trust::host_key_from_public;
use crate::{CMD_CHANNEL_CAP, EVENT_CHANNEL_CAP, MAX_DATA_BATCH};

/// Tempo que o usuário tem para decidir sobre uma host key desconhecida (não conta no
/// `connect_timeout`).
const HOST_KEY_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);
/// Tempo máximo para o desligamento educado ao encerrar.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Keepalives sem resposta antes de considerar a conexão perdida.
const KEEPALIVE_MAX: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
    pub pix_w: u16,
    pub pix_h: u16,
}

pub enum AuthMaterial {
    Password(SecretString),
    KeyFile {
        path: PathBuf,
        passphrase: Option<SecretString>,
    },
    /// Agente SSH local (`SSH_AUTH_SOCK`), sem forwarding. Apenas Unix por enquanto.
    Agent,
    /// Ainda não suportado (M2): a conexão falha com mensagem clara.
    KeyboardInteractive,
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

#[derive(Debug)]
pub struct ConnectRequest {
    pub endpoint: HostEndpoint,
    pub username: String,
    pub auth: AuthMaterial,
    pub pty: PtySize,
    /// Ex.: "xterm-256color".
    pub term: String,
    /// Orçamento para TCP + handshake + autenticação + abertura do shell. O tempo em que a
    /// sessão aguarda a decisão do usuário sobre a host key não é contado.
    pub connect_timeout: Duration,
    pub keepalive: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    Reject,
    AcceptOnce,
    AcceptAndSave,
}

/// Canal de resposta ao prompt de host key. Soltar sem responder equivale a `Reject`.
#[derive(Debug)]
pub struct HostKeyResponder(Option<oneshot::Sender<HostKeyDecision>>);

impl HostKeyResponder {
    pub fn answer(mut self, d: HostKeyDecision) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(d);
        }
    }
}

#[derive(Debug)]
pub enum SessionEvent {
    State(SessionState),
    HostKeyUnknown {
        endpoint: HostEndpoint,
        key: HostKey,
        responder: HostKeyResponder,
    },
    /// Chave diferente da confiável (ou revogada). A conexão já foi abortada.
    HostKeyChanged {
        endpoint: HostEndpoint,
        presented: HostKey,
        known: Vec<HostKey>,
    },
    Data(Vec<u8>),
    /// Fim da sessão. `error` é uma mensagem pt-BR sanitizada.
    Closed {
        error: Option<String>,
    },
}

#[derive(Debug)]
pub enum SessionCommand {
    Input(Vec<u8>),
    Resize(PtySize),
    Disconnect,
}

/// Chamado após cada evento enfileirado (para acordar a UI).
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// Soltar o handle encerra a sessão (o canal de comandos fecha e a tarefa termina).
pub struct SessionHandle {
    pub cmd_tx: mpsc::Sender<SessionCommand>,
    pub evt_rx: mpsc::Receiver<SessionEvent>,
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.try_send(SessionCommand::Disconnect);
    }
}

#[derive(Clone)]
struct Emitter {
    tx: mpsc::Sender<SessionEvent>,
    notify: Notify,
}

impl Emitter {
    /// `false` se a UI foi embora. Com o canal cheio, espera (backpressure).
    async fn send(&self, ev: SessionEvent) -> bool {
        let ok = self.tx.send(ev).await.is_ok();
        (self.notify)();
        ok
    }
    async fn state(&self, s: SessionState) -> bool {
        self.send(SessionEvent::State(s)).await
    }
}

/// Inicia a sessão no runtime dado e retorna imediatamente.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    req: ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    notify: Notify,
) -> SessionHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAP);
    let (evt_tx, evt_rx) = mpsc::channel(EVENT_CHANNEL_CAP);
    let emit = Emitter { tx: evt_tx, notify };
    rt.spawn(run(req, trust, cmd_rx, emit));
    SessionHandle { cmd_tx, evt_rx }
}

fn client_config(keepalive: Option<Duration>) -> Arc<client::Config> {
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
async fn with_budget<F: Future>(
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

// ------------------------------------------------------------------ handler

struct Handler {
    endpoint: HostEndpoint,
    trust: Arc<dyn HostTrustStore>,
    emit: Emitter,
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
            return self.reject(error::HOST_CERT);
        };
        let Some(presented) = host_key_from_public(key) else {
            return self.reject(error::HOST_STORE);
        };
        let Ok(verdict) = self.verdict(&presented).await else {
            return self.reject(error::HOST_STORE);
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
                    presented,
                    known,
                })
                .await;
            return self.reject(error::HOST_CHANGED);
        }

        // Desconhecido: pergunta ao usuário (relógio do connect_timeout pausado).
        self.paused.store(true, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let sent = self.emit.state(SessionState::AwaitingHostTrust).await
            && self
                .emit
                .send(SessionEvent::HostKeyUnknown {
                    endpoint: self.endpoint.clone(),
                    key: presented.clone(),
                    responder: HostKeyResponder(Some(tx)),
                })
                .await;
        if !sent {
            return self.reject(error::UI_GONE);
        }
        let answer = tokio::time::timeout(HOST_KEY_PROMPT_TIMEOUT, rx).await;
        self.paused.store(false, Ordering::Relaxed);
        match answer {
            Ok(Ok(HostKeyDecision::AcceptOnce)) => Ok(true),
            Ok(Ok(HostKeyDecision::AcceptAndSave)) => {
                if self.save(&presented).await.is_ok() {
                    Ok(true)
                } else {
                    self.reject(error::HOST_STORE)
                }
            }
            Ok(Ok(HostKeyDecision::Reject)) | Ok(Err(_)) => self.reject(error::HOST_REJECTED),
            Err(_) => self.reject(error::HOST_PROMPT_TIMEOUT),
        }
    }
}

// ------------------------------------------------------------------ conexão

type Handle = client::Handle<Handler>;
type Chan = russh::Channel<client::Msg>;

async fn load_key(
    path: &PathBuf,
    passphrase: Option<&SecretString>,
) -> Result<russh::keys::PrivateKey, String> {
    let text = Zeroizing::new(
        tokio::fs::read_to_string(path)
            .await
            .map_err(|_| error::KEY_READ.to_string())?,
    );
    decode_secret_key(&text, passphrase.map(|p| p.expose_secret()))
        .map_err(|_| error::KEY_DECODE.to_string())
}

/// Diz se a chave privada em `path` exige passphrase (leitura síncrona: chamar fora da thread
/// de UI). Erros são mensagens pt-BR sanitizadas.
pub fn key_requires_passphrase(path: &std::path::Path) -> Result<bool, String> {
    let text =
        Zeroizing::new(std::fs::read_to_string(path).map_err(|_| error::KEY_READ.to_string())?);
    match decode_secret_key(&text, None) {
        Ok(_) => Ok(false),
        Err(russh::keys::Error::KeyIsEncrypted) => Ok(true),
        Err(_) => Err(error::KEY_DECODE.to_string()),
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

async fn authenticate(session: &mut Handle, req: &ConnectRequest) -> Result<(), String> {
    let ok = match &req.auth {
        AuthMaterial::Password(pw) => session
            .authenticate_password(req.username.clone(), pw.expose_secret().to_string())
            .await
            .map_err(|e| error::russh_message(&e))?
            .success(),
        AuthMaterial::KeyFile { path, passphrase } => {
            let key = load_key(path, passphrase.as_ref()).await?;
            let hash = rsa_hash(session, &key.algorithm()).await;
            session
                .authenticate_publickey(
                    req.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|e| error::russh_message(&e))?
                .success()
        }
        AuthMaterial::Agent => return authenticate_agent(session, req).await,
        AuthMaterial::KeyboardInteractive => return Err(error::UNSUPPORTED_AUTH.into()),
    };
    if ok {
        Ok(())
    } else {
        Err(error::AUTH_REJECTED.into())
    }
}

#[cfg(unix)]
async fn authenticate_agent(session: &mut Handle, req: &ConnectRequest) -> Result<(), String> {
    use russh::keys::agent::AgentIdentity;
    use russh::keys::agent::client::AgentClient;

    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|_| error::NO_AGENT.to_string())?;
    let ids = agent
        .request_identities()
        .await
        .map_err(|_| error::NO_AGENT.to_string())?;
    for id in ids {
        let AgentIdentity::PublicKey { key, .. } = id else {
            continue;
        };
        let hash = rsa_hash(session, &key.algorithm()).await;
        if let Ok(r) = session
            .authenticate_publickey_with(req.username.clone(), key, hash, &mut agent)
            .await
            && r.success()
        {
            return Ok(());
        }
    }
    Err(error::AGENT_NO_KEYS.into())
}

#[cfg(not(unix))]
async fn authenticate_agent(_: &mut Handle, _: &ConnectRequest) -> Result<(), String> {
    Err(error::UNSUPPORTED_AUTH.into())
}

async fn establish(
    req: &ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    emit: &Emitter,
    pty: &Mutex<PtySize>,
) -> Result<(Handle, Chan), String> {
    let mut budget = req.connect_timeout;
    let paused = Arc::new(AtomicBool::new(false));
    let abort = Arc::new(Mutex::new(None));
    let timeout = || error::TIMEOUT.to_string();

    // 1. TCP
    let addr = (req.endpoint.host.as_str(), req.endpoint.port);
    let stream = with_budget(TcpStream::connect(addr), &mut budget, &paused)
        .await
        .map_err(|_| timeout())?
        .map_err(|e| error::io_message(&e))?;

    // 2. Handshake SSH + host key (ANTES de qualquer autenticação).
    let handler = Handler {
        endpoint: req.endpoint.clone(),
        trust,
        emit: emit.clone(),
        paused: paused.clone(),
        abort: abort.clone(),
    };
    let config = client_config(req.keepalive);
    let mut session = match with_budget(
        client::connect_stream(config, stream, handler),
        &mut budget,
        &paused,
    )
    .await
    .map_err(|_| timeout())?
    {
        Ok(s) => s,
        Err(e) => {
            let reason = abort.lock().ok().and_then(|g| g.clone());
            return Err(reason.unwrap_or_else(|| error::russh_message(&e)));
        }
    };

    // 3. Autenticação
    if !emit.state(SessionState::AwaitingAuth).await {
        return Err(error::UI_GONE.into());
    }
    with_budget(authenticate(&mut session, req), &mut budget, &paused)
        .await
        .map_err(|_| timeout())??;

    // 4. Canal + PTY + shell
    let size = *pty.lock().map_err(|_| error::CONNECTION_LOST.to_string())?;
    let open = async {
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| error::russh_message(&e))?;
        channel
            .request_pty(
                false,
                &req.term,
                size.cols as u32,
                size.rows as u32,
                size.pix_w as u32,
                size.pix_h as u32,
                &[],
            )
            .await
            .map_err(|e| error::russh_message(&e))?;
        channel
            .request_shell(false)
            .await
            .map_err(|e| error::russh_message(&e))?;
        Ok::<_, String>(channel)
    };
    let channel = with_budget(open, &mut budget, &paused)
        .await
        .map_err(|_| timeout())??;
    Ok((session, channel))
}

/// Antes de conectar: só acompanha resize e espera o cancelamento.
async fn wait_cancel(cmd_rx: &mut mpsc::Receiver<SessionCommand>, pty: &Mutex<PtySize>) {
    loop {
        match cmd_rx.recv().await {
            Some(SessionCommand::Resize(s)) => {
                if let Ok(mut g) = pty.lock() {
                    *g = s;
                }
            }
            Some(SessionCommand::Input(_)) => {}
            Some(SessionCommand::Disconnect) | None => return,
        }
    }
}

enum End {
    Clean,
    Failed(String),
}

async fn run(
    req: ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    mut cmd_rx: mpsc::Receiver<SessionCommand>,
    emit: Emitter,
) {
    let end = run_inner(&req, trust, &mut cmd_rx, &emit).await;
    // `req` (com senha/passphrase) é descartado aqui.
    drop(req);
    match end {
        End::Clean => {
            emit.state(SessionState::Disconnected).await;
            emit.send(SessionEvent::Closed { error: None }).await;
        }
        End::Failed(msg) => {
            emit.state(SessionState::Error(msg.clone())).await;
            emit.send(SessionEvent::Closed { error: Some(msg) }).await;
        }
    }
}

async fn run_inner(
    req: &ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    cmd_rx: &mut mpsc::Receiver<SessionCommand>,
    emit: &Emitter,
) -> End {
    if !emit.state(SessionState::Connecting).await {
        return End::Clean;
    }
    let pty = Mutex::new(req.pty);
    let established = tokio::select! {
        r = establish(req, trust, emit, &pty) => r,
        _ = wait_cancel(cmd_rx, &pty) => return End::Clean,
    };
    let (session, mut channel) = match established {
        Ok(x) => x,
        Err(msg) => return End::Failed(msg),
    };
    if !emit.state(SessionState::Connected).await {
        return End::Clean;
    }

    let mut result = End::Clean;
    let mut remote_closed = false;
    'io: loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(SessionCommand::Input(bytes)) => {
                    if channel.data(&bytes[..]).await.is_err() {
                        result = End::Failed(error::CONNECTION_LOST.into());
                        break 'io;
                    }
                }
                Some(SessionCommand::Resize(s)) => {
                    if channel
                        .window_change(s.cols as u32, s.rows as u32, s.pix_w as u32, s.pix_h as u32)
                        .await
                        .is_err()
                    {
                        result = End::Failed(error::CONNECTION_LOST.into());
                        break 'io;
                    }
                }
                Some(SessionCommand::Disconnect) | None => break 'io,
            },
            msg = channel.wait() => {
                let mut batch = Vec::new();
                let mut next = msg;
                // Agrega o que já está disponível num único lote (até MAX_DATA_BATCH).
                loop {
                    match next {
                        Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            batch.extend_from_slice(&data);
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                            remote_closed = true;
                        }
                        None => {
                            if !remote_closed {
                                result = End::Failed(error::CONNECTION_LOST.into());
                            }
                            remote_closed = true;
                        }
                        Some(_) => {}
                    }
                    if remote_closed || batch.len() >= MAX_DATA_BATCH {
                        break;
                    }
                    match tokio::time::timeout(Duration::ZERO, channel.wait()).await {
                        Ok(m) => next = m,
                        Err(_) => break,
                    }
                }
                // Canal limitado: se a UI não drena, esta espera para a leitura do canal SSH
                // (a janela de recepção do SSH aplica backpressure no servidor).
                if !batch.is_empty() && !emit.send(SessionEvent::Data(batch)).await {
                    break 'io;
                }
                if remote_closed {
                    break 'io;
                }
            }
        }
    }
    let _ = tokio::time::timeout(
        DISCONNECT_TIMEOUT,
        session.disconnect(Disconnect::ByApplication, "", "en"),
    )
    .await;
    result
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
}
