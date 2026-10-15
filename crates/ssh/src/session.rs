//! Sessão de terminal sobre uma [`Connection`]: eventos para a UI, comandos da UI e laço de E/S do
//! shell. A conexão (TCP/jump hosts, host key, autenticação) vive em `connection.rs`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use myssh_core::SessionState;
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore};
use russh::ChannelMsg;
use secrecy::SecretString;
use tokio::sync::{mpsc, oneshot};

use crate::connection::{AuthMaterial, Connection, EventSink, Hop};
use crate::error;
use crate::{CMD_CHANNEL_CAP, EVENT_CHANNEL_CAP, MAX_DATA_BATCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
    pub pix_w: u16,
    pub pix_h: u16,
}

#[derive(Debug)]
pub struct ConnectRequest {
    pub endpoint: HostEndpoint,
    pub username: String,
    pub auth: AuthMaterial,
    pub pty: PtySize,
    /// Ex.: "xterm-256color".
    pub term: String,
    /// Orçamento do destino para TCP (ou `direct-tcpip`) + handshake + autenticação. O tempo em que a
    /// sessão aguarda a decisão do usuário (host key, prompts) não é contado.
    pub connect_timeout: Duration,
    pub keepalive: Option<Duration>,
    /// Jump hosts em ordem (o primeiro é alcançado direto; cada um alcança o seguinte; o último
    /// alcança o destino). Vazio = conexão direta. Cada salto tem timeout, host key e credenciais próprios.
    pub jumps: Vec<Hop>,
}

impl ConnectRequest {
    /// Descarta (zera) as credenciais do destino e de todos os saltos. Depois disto o pedido não
    /// serve mais para autenticar.
    pub(crate) fn scrub_credentials(&mut self) {
        self.auth.scrub();
        for h in &mut self.jumps {
            h.auth.scrub();
        }
    }
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
    pub(crate) fn new(tx: oneshot::Sender<HostKeyDecision>) -> Self {
        Self(Some(tx))
    }

    pub fn answer(mut self, d: HostKeyDecision) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(d);
        }
    }
}

/// Canal de resposta a um pedido de keyboard-interactive. Soltar sem responder cancela a autenticação.
/// As respostas (uma por prompt, na ordem) são `SecretString`: nunca persistidas nem registradas.
#[derive(Debug)]
pub struct AuthResponder(Option<oneshot::Sender<Vec<SecretString>>>);

impl AuthResponder {
    pub(crate) fn new(tx: oneshot::Sender<Vec<SecretString>>) -> Self {
        Self(Some(tx))
    }

    pub fn answer(mut self, answers: Vec<SecretString>) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(answers);
        }
    }
}

/// Eventos da sessão. Os campos `hop` indicam **qual servidor da cadeia** gerou o evento:
/// `Some(i)` = i-ésimo jump host (`ConnectRequest::jumps[i]`, base 0); `None` = o destino final
/// (também em conexões diretas, sem jump hosts).
#[derive(Debug)]
pub enum SessionEvent {
    State(SessionState),
    HostKeyUnknown {
        endpoint: HostEndpoint,
        hop: Option<usize>,
        key: HostKey,
        responder: HostKeyResponder,
    },
    /// Chave diferente da confiável (ou revogada). A conexão já foi abortada, antes de qualquer
    /// autenticação ou contato com os servidores seguintes da cadeia.
    HostKeyChanged {
        endpoint: HostEndpoint,
        hop: Option<usize>,
        presented: HostKey,
        known: Vec<HostKey>,
    },
    /// Pedido de keyboard-interactive/MFA: `prompts` = (texto, echo). Responda com
    /// [`AuthResponder::answer`] (um segredo por prompt); soltar o responder cancela.
    /// Textos já sem caracteres de controle e com tamanho limitado.
    AuthPrompt {
        hop: Option<usize>,
        name: String,
        instructions: String,
        prompts: Vec<(String, bool)>,
        responder: AuthResponder,
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
    conn: ConnSlot,
}

/// Conexão da sessão, publicada quando ela fica `Connected` e limpa ao terminar.
type ConnSlot = Arc<Mutex<Option<Arc<Connection>>>>;

impl SessionHandle {
    pub fn new(cmd_tx: mpsc::Sender<SessionCommand>, evt_rx: mpsc::Receiver<SessionEvent>) -> Self {
        Self {
            cmd_tx,
            evt_rx,
            conn: ConnSlot::default(),
        }
    }

    /// A conexão SSH viva da sessão (para abrir SFTP em canais próprios); `None` antes de
    /// conectar e depois de encerrar.
    pub fn connection(&self) -> Option<Arc<Connection>> {
        self.conn.lock().ok().and_then(|g| g.clone())
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.try_send(SessionCommand::Disconnect);
    }
}

/// Inicia a sessão (conexão + shell) no runtime dado e retorna imediatamente. Para obter uma
/// conexão reutilizável (vários canais, SFTP) use [`Connection::connect`].
pub fn spawn(
    rt: &tokio::runtime::Handle,
    req: ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    notify: Notify,
) -> SessionHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAP);
    let (evt_tx, evt_rx) = mpsc::channel(EVENT_CHANNEL_CAP);
    let emit = EventSink::new(evt_tx, notify);
    let handle = SessionHandle::new(cmd_tx, evt_rx);
    rt.spawn(run(req, trust, cmd_rx, emit, handle.conn.clone()));
    handle
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
    emit: EventSink,
    slot: ConnSlot,
) {
    let mut req = req;
    let end = run_inner(&mut req, trust, &mut cmd_rx, &emit, &slot).await;
    if let Ok(mut g) = slot.lock() {
        *g = None;
    }
    // Senha/passphrase já foram descartadas logo após a autenticação (`ConnectRequest::scrub`);
    // `req` cai aqui de qualquer forma.
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
    req: &mut ConnectRequest,
    trust: Arc<dyn HostTrustStore>,
    cmd_rx: &mut mpsc::Receiver<SessionCommand>,
    emit: &EventSink,
    slot: &ConnSlot,
) -> End {
    if !emit.state(SessionState::Connecting).await {
        return End::Clean;
    }
    let pty = Mutex::new(req.pty);
    let established = tokio::select! {
        r = async {
            let conn = Connection::connect(req, &trust, emit).await;
            // Autenticação concluída (ou falhou): as credenciais não são mais necessárias e não
            // devem ficar na memória durante toda a sessão.
            req.scrub_credentials();
            let conn = conn?;
            let size = *pty.lock().map_err(|_| error::CONNECTION_LOST().to_string())?;
            let shell = tokio::time::timeout(req.connect_timeout, conn.open_shell(size, &req.term))
                .await
                .map_err(|_| error::TIMEOUT().to_string())?;
            match shell {
                Ok(sh) => Ok((conn, sh)),
                Err(e) => {
                    conn.close().await;
                    Err(e)
                }
            }
        } => r,
        _ = wait_cancel(cmd_rx, &pty) => return End::Clean,
    };
    let (conn, mut channel) = match established {
        Ok(x) => x,
        Err(msg) => return End::Failed(msg),
    };
    if !emit.state(SessionState::Connected).await {
        conn.close().await;
        return End::Clean;
    }
    if let Ok(mut g) = slot.lock() {
        *g = Some(conn.clone());
    }

    let mut result = End::Clean;
    let mut remote_closed = false;
    'io: loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(SessionCommand::Input(bytes)) => {
                    if channel.data(&bytes[..]).await.is_err() {
                        result = End::Failed(error::CONNECTION_LOST().into());
                        break 'io;
                    }
                }
                Some(SessionCommand::Resize(s)) => {
                    if channel.window_change(s).await.is_err() {
                        result = End::Failed(error::CONNECTION_LOST().into());
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
                                result = End::Failed(error::CONNECTION_LOST().into());
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
    drop(channel);
    conn.close().await;
    result
}
