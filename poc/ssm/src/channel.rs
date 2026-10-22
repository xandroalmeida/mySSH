//! Data channel do SSM sobre websocket, exposto como um stream local (`tokio::io::duplex`).
//!
//! Uma task dona do websocket faz: ack de toda `output_stream_data`, entrega em ordem (buffer de
//! fora de ordem), numeração das `input_stream_data`, retransmissão do que não recebeu ack e o
//! handshake com o agente. O stream só é devolvido depois do `HandshakeComplete`.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use crate::message::{
    ACKNOWLEDGE, Acknowledge, CHANNEL_CLOSED, ChannelClosed, ClientMessage, HandshakeRequest,
    HandshakeResponse, INPUT_STREAM, OUTPUT_STREAM, OpenDataChannel, PAUSE_PUBLICATION,
    ProcessedClientAction, START_PUBLICATION, action_status, flag, payload_type,
};

const CLIENT_VERSION: &str = "1.2.0.0";
/// Tamanho máximo de payload por `input_stream_data` (o mesmo do plugin).
const CHUNK: usize = 1024;
/// Máximo de mensagens enviadas sem ack antes de parar de ler o lado local.
const WINDOW: usize = 128;
const RESEND_AFTER: Duration = Duration::from_millis(1500);
const PING_EVERY: Duration = Duration::from_secs(240);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// O lado do usuário: bytes escritos aqui chegam à porta remota e vice-versa.
pub type SsmStream = DuplexStream;

type WsTx = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Resultado do `StartSession`. O token autentica o data channel: nunca é logado.
pub struct SsmSession {
    pub session_id: String,
    stream_url: String,
    token: SecretString,
}

impl std::fmt::Debug for SsmSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsmSession")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

/// `StartSession` com o documento `AWS-StartSSHSession` (sessão do tipo Port para `port`).
pub async fn start_session(
    client: &aws_sdk_ssm::Client,
    target: &str,
    port: u16,
) -> Result<SsmSession> {
    let out = client
        .start_session()
        .target(target)
        .document_name("AWS-StartSSHSession")
        .parameters("portNumber", vec![port.to_string()])
        .send()
        .await
        .map_err(|e| {
            anyhow!(
                "StartSession: {}",
                aws_sdk_ssm::error::DisplayErrorContext(&e)
            )
        })?;
    Ok(SsmSession {
        session_id: out.session_id.context("StartSession sem SessionId")?,
        stream_url: out.stream_url.context("StartSession sem StreamUrl")?,
        token: SecretString::from(out.token_value.context("StartSession sem TokenValue")?),
    })
}

/// Conecta o websocket, faz o handshake com o agente e devolve o stream pronto para uso.
/// A task devolvida termina quando o canal fecha (o resultado diz por quê).
pub async fn open(s: SsmSession) -> Result<(SsmStream, JoinHandle<Result<()>>)> {
    let (mut ws, _) = tokio_tungstenite::connect_async(s.stream_url.as_str())
        .await
        .context("conexão websocket com ssmmessages")?;
    let open = serde_json::to_string(&OpenDataChannel {
        message_schema_version: "1.0",
        request_id: Uuid::new_v4().to_string(),
        token_value: s.token.expose_secret(),
        client_id: Uuid::new_v4().to_string(),
        client_version: CLIENT_VERSION,
    })?;
    ws.send(Message::Text(open.into()))
        .await
        .context("envio do OpenDataChannel")?;

    let (user, ours) = tokio::io::duplex(256 * 1024);
    let (ready_tx, ready_rx) = oneshot::channel();
    let task = tokio::spawn(run(ws, ours, ready_tx));
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, ready_rx).await {
        Ok(Ok(Ok(()))) => Ok((user, task)),
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_)) => Err(task
            .await
            .map_err(|e| anyhow!("task do data channel: {e}"))
            .and_then(|r| r)
            .err()
            .unwrap_or_else(|| anyhow!("data channel encerrado antes do handshake"))),
        Err(_) => {
            task.abort();
            bail!("timeout esperando o handshake do agente SSM")
        }
    }
}

enum Flow {
    Continue,
    End,
}

struct State {
    /// Próximo número de sequência das nossas `input_stream_data`.
    out_seq: i64,
    /// Próximo número de sequência esperado das `output_stream_data` do agente.
    expected: i64,
    pending_in: BTreeMap<i64, ClientMessage>,
    unacked: BTreeMap<i64, (Vec<u8>, Instant)>,
    paused: bool,
    established: bool,
    ready: Option<oneshot::Sender<Result<()>>>,
}

async fn run(
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    local: DuplexStream,
    ready: oneshot::Sender<Result<()>>,
) -> Result<()> {
    let (mut tx, mut rx) = ws.split();
    let (mut lr, mut lw) = tokio::io::split(local);
    let mut st = State {
        out_seq: 0,
        expected: 0,
        pending_in: BTreeMap::new(),
        unacked: BTreeMap::new(),
        paused: false,
        established: false,
        ready: Some(ready),
    };
    let mut buf = vec![0u8; CHUNK];
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let mut ping = tokio::time::interval_at(tokio::time::Instant::now() + PING_EVERY, PING_EVERY);

    let result: Result<()> = async {
        loop {
            let can_read = st.established && !st.paused && st.unacked.len() < WINDOW;
            tokio::select! {
                frame = rx.next() => {
                    let Some(frame) = frame else { bail!("websocket fechado pelo servidor") };
                    match frame.context("leitura do websocket")? {
                        Message::Binary(b) => {
                            let m = ClientMessage::decode(&b)?;
                            if let Flow::End = st.handle(m, &mut tx, &mut lw).await? {
                                return Ok(());
                            }
                        }
                        Message::Close(_) => return Ok(()),
                        _ => {}
                    }
                }
                n = lr.read(&mut buf), if can_read => {
                    let n = n.context("leitura do lado local")?;
                    if n == 0 {
                        // Lado local fechou: pede ao agente para encerrar a sessão.
                        let p = flag::TERMINATE_SESSION.to_be_bytes().to_vec();
                        st.send_input(&mut tx, payload_type::FLAG, p).await?;
                        return Ok(());
                    }
                    st.send_input(&mut tx, payload_type::OUTPUT, buf[..n].to_vec()).await?;
                }
                _ = tick.tick() => st.resend(&mut tx).await?,
                _ = ping.tick() => tx.send(Message::Ping(Vec::new().into())).await?,
            }
        }
    }
    .await;

    if let Some(r) = st.ready.take() {
        let e = match &result {
            Err(e) => anyhow!("{e:#}"),
            Ok(()) => anyhow!("data channel encerrado antes do handshake"),
        };
        let _ = r.send(Err(e));
    }
    let _ = lw.shutdown().await;
    let _ = tx.send(Message::Close(None)).await;
    result
}

impl State {
    async fn send_input(&mut self, tx: &mut WsTx, ptype: u32, payload: Vec<u8>) -> Result<()> {
        let m = ClientMessage::new(INPUT_STREAM, self.out_seq, 0, ptype, payload);
        let bytes = m.encode();
        tx.send(Message::Binary(bytes.clone().into()))
            .await
            .context("envio ao websocket")?;
        self.unacked.insert(self.out_seq, (bytes, Instant::now()));
        self.out_seq += 1;
        Ok(())
    }

    async fn resend(&mut self, tx: &mut WsTx) -> Result<()> {
        let now = Instant::now();
        for (bytes, sent) in self.unacked.values_mut() {
            if now.duration_since(*sent) >= RESEND_AFTER {
                tx.feed(Message::Binary(bytes.clone().into())).await?;
                *sent = now;
            }
        }
        // Também despacha pongs que o tungstenite tenha enfileirado.
        tx.flush().await?;
        Ok(())
    }

    async fn handle(
        &mut self,
        m: ClientMessage,
        tx: &mut WsTx,
        lw: &mut WriteHalf<DuplexStream>,
    ) -> Result<Flow> {
        match m.message_type.as_str() {
            OUTPUT_STREAM => {
                // Ack sempre, inclusive de duplicata (nosso ack anterior pode ter se perdido).
                tx.send(Message::Binary(ClientMessage::ack_for(&m).encode().into()))
                    .await?;
                if m.sequence_number == self.expected {
                    self.expected += 1;
                    if let Flow::End = self.process(m, tx, lw).await? {
                        return Ok(Flow::End);
                    }
                    while let Some(next) = self.pending_in.remove(&self.expected) {
                        self.expected += 1;
                        if let Flow::End = self.process(next, tx, lw).await? {
                            return Ok(Flow::End);
                        }
                    }
                } else if m.sequence_number > self.expected {
                    self.pending_in.insert(m.sequence_number, m);
                }
            }
            ACKNOWLEDGE => {
                let a: Acknowledge = serde_json::from_slice(&m.payload).context("ack inválido")?;
                self.unacked.remove(&a.sequence_number);
            }
            CHANNEL_CLOSED => {
                let c: ChannelClosed =
                    serde_json::from_slice(&m.payload).unwrap_or(ChannelClosed {
                        output: String::new(),
                    });
                if c.output.is_empty() {
                    return Ok(Flow::End);
                }
                bail!("canal fechado pelo SSM: {}", c.output);
            }
            PAUSE_PUBLICATION => self.paused = true,
            START_PUBLICATION => self.paused = false,
            _ => {}
        }
        Ok(Flow::Continue)
    }

    async fn process(
        &mut self,
        m: ClientMessage,
        tx: &mut WsTx,
        lw: &mut WriteHalf<DuplexStream>,
    ) -> Result<Flow> {
        match m.payload_type {
            payload_type::OUTPUT => {
                if lw.write_all(&m.payload).await.is_err() {
                    return Ok(Flow::End);
                }
            }
            payload_type::HANDSHAKE_REQUEST => {
                let req: HandshakeRequest =
                    serde_json::from_slice(&m.payload).context("HandshakeRequest inválido")?;
                let mut unsupported = None;
                let actions = req
                    .requested_client_actions
                    .iter()
                    .map(|a| {
                        let (status, error) = match a.action_type.as_str() {
                            "SessionType" => (action_status::SUCCESS, String::new()),
                            other => {
                                unsupported.get_or_insert_with(|| other.to_string());
                                (action_status::UNSUPPORTED, format!("{other} not supported"))
                            }
                        };
                        ProcessedClientAction {
                            action_type: a.action_type.clone(),
                            action_status: status,
                            action_result: None,
                            error,
                        }
                    })
                    .collect();
                let resp = HandshakeResponse {
                    client_version: CLIENT_VERSION.into(),
                    processed_client_actions: actions,
                    errors: Vec::new(),
                };
                self.send_input(
                    tx,
                    payload_type::HANDSHAKE_RESPONSE,
                    serde_json::to_vec(&resp)?,
                )
                .await?;
                if let Some(a) = unsupported {
                    // Ex.: KMSEncryption ligado nas preferências do Session Manager.
                    bail!(
                        "o agente pediu '{a}', que a POC não implementa (agente {})",
                        req.agent_version
                    );
                }
            }
            payload_type::HANDSHAKE_COMPLETE => {
                self.established = true;
                if let Some(r) = self.ready.take() {
                    let _ = r.send(Ok(()));
                }
            }
            payload_type::FLAG => {
                let v = m
                    .payload
                    .get(..4)
                    .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
                    .unwrap_or(0);
                match v {
                    flag::CONNECT_TO_PORT_ERROR => {
                        bail!("o agente SSM não conseguiu conectar na porta de destino")
                    }
                    flag::TERMINATE_SESSION | flag::DISCONNECT_TO_PORT => return Ok(Flow::End),
                    _ => {}
                }
            }
            payload_type::ERROR | payload_type::STDERR => {
                bail!(
                    "erro do agente SSM: {}",
                    String::from_utf8_lossy(&m.payload).trim()
                );
            }
            payload_type::EXIT_CODE => return Ok(Flow::End),
            _ => {}
        }
        Ok(Flow::Continue)
    }
}
