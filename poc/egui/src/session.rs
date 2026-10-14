//! Sessão SSH (russh) rodando num runtime tokio, desacoplada da UI por canais limitados.
//!
//! Nada aqui conhece egui: a UI recebe `SessionEvent`s e envia `SessionCmd`s.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg, PublicKey};
use russh::{client, ChannelMsg, Disconnect};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::known_hosts::{fingerprint, KnownHosts, Verdict};

/// Capacidade dos canais (limitados: backpressure em vez de memória ilimitada).
pub const EVENT_CHANNEL_CAP: usize = 32;
pub const CMD_CHANNEL_CAP: usize = 256;
const HOST_KEY_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);
/// Rede (15s) + tempo máximo que o usuário tem para decidir sobre a host key.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15 + HOST_KEY_PROMPT_TIMEOUT.as_secs());

pub enum Auth {
    Password(SecretString),
    Key {
        path: PathBuf,
        passphrase: Option<SecretString>,
    },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::Password(_) => f.write_str("Auth::Password([REDACTED])"),
            Auth::Key { path, passphrase } => f
                .debug_struct("Auth::Key")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| "[REDACTED]"))
                .finish(),
        }
    }
}

#[derive(Debug)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    pub cols: u16,
    pub rows: u16,
    pub pix_w: u16,
    pub pix_h: u16,
    /// Arquivo de hosts confiáveis. `None` só é permitido junto de `pinned_fingerprint`.
    pub known_hosts: Option<PathBuf>,
    /// Fingerprint esperado (`SHA256:...`) pré-aprovado: aceita SOMENTE se bater, sem
    /// consultar/gravar o arquivo e sem perguntar. Diferente => rejeita.
    pub pinned_fingerprint: Option<String>,
}

pub struct HostKeyPrompt {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    reply: Option<oneshot::Sender<bool>>,
}

impl HostKeyPrompt {
    pub fn answer(&mut self, accept: bool) {
        if let Some(tx) = self.reply.take() {
            let _ = tx.send(accept);
        }
    }
}

pub enum SessionEvent {
    HostKeyPrompt(HostKeyPrompt),
    Connected,
    Data(Vec<u8>),
    Closed(Option<String>),
}

#[derive(Debug)]
pub enum SessionCmd {
    Input(Vec<u8>),
    Resize {
        cols: u16,
        rows: u16,
        pix_w: u16,
        pix_h: u16,
    },
    Close,
}

pub struct SessionHandle {
    pub cmd_tx: mpsc::Sender<SessionCmd>,
    pub evt_rx: mpsc::Receiver<SessionEvent>,
    task: JoinHandle<()>,
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.try_send(SessionCmd::Close);
        // Dá uma chance ao Close; se a tarefa estiver presa (ex.: aguardando fingerprint), aborta.
        self.task.abort();
    }
}

pub type Notify = Arc<dyn Fn() + Send + Sync>;

struct Handler {
    host: String,
    port: u16,
    known: Option<KnownHosts>,
    pinned: Option<String>,
    evt_tx: mpsc::Sender<SessionEvent>,
    notify: Notify,
    reject_reason: Arc<Mutex<Option<String>>>,
}

impl Handler {
    fn reject(&self, reason: String) -> Result<bool, russh::Error> {
        if let Ok(mut g) = self.reject_reason.lock() {
            *g = Some(reason);
        }
        Ok(false)
    }
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return self.reject("certificados de host não suportados neste PoC".into());
        };
        let fp = fingerprint(key);

        // 1. Fingerprint pré-aprovado (testes/demo): aceita só se bater exatamente.
        if let Some(expected) = &self.pinned {
            return if *expected == fp {
                Ok(true)
            } else {
                self.reject(format!(
                    "fingerprint do servidor ({fp}) difere do esperado ({expected})"
                ))
            };
        }

        // 2. Arquivo de hosts confiáveis.
        let Some(known) = &self.known else {
            return self.reject("sem arquivo de hosts confiáveis nem fingerprint esperado".into());
        };
        match known.check(&self.host, self.port, key) {
            Err(e) => self.reject(e),
            Ok(Verdict::Trusted) => Ok(true),
            Ok(Verdict::Mismatch { saved_fingerprint }) => self.reject(format!(
                "ALERTA: a chave do host mudou! Salva: {saved_fingerprint}; apresentada: {fp}. \
                 Conexão bloqueada (possível ataque man-in-the-middle)."
            )),
            Ok(Verdict::Unknown) => {
                let (tx, rx) = oneshot::channel();
                let prompt = HostKeyPrompt {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: key.algorithm().as_str().to_string(),
                    fingerprint: fp,
                    reply: Some(tx),
                };
                if self
                    .evt_tx
                    .send(SessionEvent::HostKeyPrompt(prompt))
                    .await
                    .is_err()
                {
                    return self.reject("interface encerrada".into());
                }
                (self.notify)();
                match tokio::time::timeout(HOST_KEY_PROMPT_TIMEOUT, rx).await {
                    Ok(Ok(true)) => match known.add(&self.host, self.port, key) {
                        Ok(()) => Ok(true),
                        Err(e) => self.reject(format!("não foi possível salvar o host: {e}")),
                    },
                    Ok(Ok(false)) => self.reject("host rejeitado pelo usuário".into()),
                    _ => self.reject("sem resposta sobre a chave do host (timeout)".into()),
                }
            }
        }
    }
}

/// Inicia a sessão no runtime dado. Retorna imediatamente.
pub fn spawn(rt: &tokio::runtime::Handle, params: ConnectParams, notify: Notify) -> SessionHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAP);
    let (evt_tx, evt_rx) = mpsc::channel(EVENT_CHANNEL_CAP);
    let task = rt.spawn(async move {
        let result = run(params, cmd_rx, evt_tx.clone(), notify.clone()).await;
        let _ = evt_tx.send(SessionEvent::Closed(result.err())).await;
        notify();
    });
    SessionHandle {
        cmd_tx,
        evt_rx,
        task,
    }
}

async fn load_key(
    path: &PathBuf,
    passphrase: Option<&SecretString>,
) -> Result<russh::keys::PrivateKey, String> {
    let text = Zeroizing::new(
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("não foi possível ler a chave privada: {e}"))?,
    );
    decode_secret_key(&text, passphrase.map(|p| p.expose_secret()))
        .map_err(|e| format!("chave privada inválida ou passphrase incorreta: {e}"))
}

async fn run(
    params: ConnectParams,
    mut cmd_rx: mpsc::Receiver<SessionCmd>,
    evt_tx: mpsc::Sender<SessionEvent>,
    notify: Notify,
) -> Result<(), String> {
    let reject_reason = Arc::new(Mutex::new(None));
    let handler = Handler {
        host: params.host.clone(),
        port: params.port,
        known: params.known_hosts.clone().map(KnownHosts::new),
        pinned: params.pinned_fingerprint.clone(),
        evt_tx: evt_tx.clone(),
        notify: notify.clone(),
        reject_reason: reject_reason.clone(),
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let connect = client::connect(config, (params.host.as_str(), params.port), handler);
    let mut session = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Err(_) => return Err("tempo esgotado ao conectar".into()),
        Ok(Err(e)) => {
            let reason = reject_reason.lock().ok().and_then(|g| g.clone());
            return Err(reason.unwrap_or_else(|| format!("falha ao conectar: {e}")));
        }
        Ok(Ok(s)) => s,
    };

    // A host key já foi validada em check_server_key ANTES de qualquer autenticação.
    let auth = match &params.auth {
        Auth::Password(pw) => session
            .authenticate_password(params.user.clone(), pw.expose_secret().to_string())
            .await
            .map_err(|e| format!("erro na autenticação: {e}"))?,
        Auth::Key { path, passphrase } => {
            let key = load_key(path, passphrase.as_ref()).await?;
            let hash = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| e.to_string())?
                .flatten();
            session
                .authenticate_publickey(
                    params.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|e| format!("erro na autenticação: {e}"))?
        }
    };
    if !auth.success() {
        return Err("autenticação recusada pelo servidor".into());
    }

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("falha ao abrir canal: {e}"))?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            params.cols as u32,
            params.rows as u32,
            params.pix_w as u32,
            params.pix_h as u32,
            &[],
        )
        .await
        .map_err(|e| format!("falha ao pedir PTY: {e}"))?;
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("falha ao abrir shell: {e}"))?;

    let _ = evt_tx.send(SessionEvent::Connected).await;
    notify();

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(SessionCmd::Input(bytes)) => {
                    channel.data(&bytes[..]).await.map_err(|e| e.to_string())?;
                }
                Some(SessionCmd::Resize { cols, rows, pix_w, pix_h }) => {
                    channel
                        .window_change(cols as u32, rows as u32, pix_w as u32, pix_h as u32)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Some(SessionCmd::Close) | None => break,
            },
            msg = channel.wait() => match msg {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    // send().await com canal limitado: se a UI não drena, paramos de ler
                    // do canal SSH (a janela de recepção do SSH aplica backpressure no servidor).
                    if evt_tx.send(SessionEvent::Data(data.to_vec())).await.is_err() {
                        break;
                    }
                    notify();
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            },
        }
    }
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(())
}

/// Auxiliar para testes/diagnóstico: fingerprint de uma chave pública no formato OpenSSH.
pub fn fingerprint_of_openssh(line: &str) -> Result<String, String> {
    PublicKey::from_openssh(line)
        .map(|k| fingerprint(&k))
        .map_err(|e| e.to_string())
}
