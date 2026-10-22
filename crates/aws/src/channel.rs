//! Data channel do SSM sobre websocket, exposto como um stream local (`tokio::io::duplex`).
//!
//! Uma task dona do websocket faz: ack de toda `output_stream_data`, entrega em ordem (buffer de
//! fora de ordem), numeração das `input_stream_data`, retransmissão do que não recebeu ack e o
//! handshake com o agente. O stream só é devolvido depois do `HandshakeComplete`. Soltar o stream
//! pede ao agente o fim da sessão (`TerminateSession`) e encerra a task.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::message::{
    ACKNOWLEDGE, Acknowledge, CHANNEL_CLOSED, ChannelClosed, ClientMessage, HandshakeRequest,
    HandshakeResponse, INPUT_STREAM, OUTPUT_STREAM, OpenDataChannel, PAUSE_PUBLICATION,
    ProcessedClientAction, START_PUBLICATION, action_status, flag, payload_type,
};
use crate::text;

const CLIENT_VERSION: &str = "1.2.0.0";
/// Tamanho máximo de payload por `input_stream_data` (o mesmo do session-manager-plugin).
const CHUNK: usize = 1024;
/// Máximo de mensagens enviadas sem ack antes de parar de ler o lado local.
const WINDOW: usize = 128;
const RESEND_AFTER: Duration = Duration::from_millis(1500);
const PING_EVERY: Duration = Duration::from_secs(240);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const DUPLEX_BUF: usize = 256 * 1024;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsTx = SplitSink<Ws, Message>;
type Res<T> = Result<T, String>;

/// Conecta o websocket do data channel (`stream_url` do `StartSession`), autentica com o token e
/// faz o handshake com o agente. Devolve o lado do usuário do stream.
pub(crate) async fn open(stream_url: &str, token: &SecretString) -> Res<DuplexStream> {
    let (mut ws, _) = tokio_tungstenite::connect_async(stream_url)
        .await
        .map_err(|_| text::WEBSOCKET().to_string())?;
    let open = Zeroizing::new(
        serde_json::to_string(&OpenDataChannel {
            message_schema_version: "1.0",
            request_id: Uuid::new_v4().to_string(),
            token_value: token.expose_secret(),
            client_id: Uuid::new_v4().to_string(),
            client_version: CLIENT_VERSION,
        })
        .map_err(|_| text::PROTOCOL().to_string())?,
    );
    ws.send(Message::Text(open.as_str().into()))
        .await
        .map_err(|_| text::WEBSOCKET().to_string())?;
    drop(open);

    let (user, ours) = tokio::io::duplex(DUPLEX_BUF);
    let (ready_tx, ready_rx) = oneshot::channel();
    let task = tokio::spawn(run(ws, ours, ready_tx));
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, ready_rx).await {
        Ok(Ok(Ok(()))) => Ok(user),
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_)) => Err(text::CLOSED_BEFORE_HANDSHAKE().to_string()),
        Err(_) => {
            task.abort();
            Err(text::HANDSHAKE_TIMEOUT().to_string())
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
    ready: Option<oneshot::Sender<Res<()>>>,
}

async fn run(ws: Ws, local: DuplexStream, ready: oneshot::Sender<Res<()>>) -> Res<()> {
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
    let ws_err = |_| text::WEBSOCKET().to_string();

    let result: Res<()> = async {
        loop {
            let can_read = st.established && !st.paused && st.unacked.len() < WINDOW;
            tokio::select! {
                frame = rx.next() => {
                    let Some(frame) = frame else { return Err(text::WEBSOCKET().to_string()) };
                    match frame.map_err(ws_err)? {
                        Message::Binary(b) => {
                            let m = ClientMessage::decode(&b)
                                .map_err(|_| text::PROTOCOL().to_string())?;
                            if let Flow::End = st.handle(m, &mut tx, &mut lw).await? {
                                return Ok(());
                            }
                        }
                        Message::Close(_) => return Ok(()),
                        _ => {}
                    }
                }
                n = lr.read(&mut buf), if can_read => {
                    let n = n.unwrap_or(0);
                    if n == 0 {
                        // Lado local fechou: pede ao agente para encerrar a sessão.
                        let p = flag::TERMINATE_SESSION.to_be_bytes().to_vec();
                        st.send_input(&mut tx, payload_type::FLAG, p).await?;
                        return Ok(());
                    }
                    st.send_input(&mut tx, payload_type::OUTPUT, buf[..n].to_vec()).await?;
                }
                _ = tick.tick() => st.resend(&mut tx).await?,
                _ = ping.tick() => tx.send(Message::Ping(Vec::new().into())).await.map_err(ws_err)?,
            }
        }
    }
    .await;

    if let Some(r) = st.ready.take() {
        let e = match &result {
            Err(e) => e.clone(),
            Ok(()) => text::CLOSED_BEFORE_HANDSHAKE().to_string(),
        };
        let _ = r.send(Err(e));
    }
    let _ = lw.shutdown().await;
    let _ = tx.send(Message::Close(None)).await;
    result
}

impl State {
    async fn send_input(&mut self, tx: &mut WsTx, ptype: u32, payload: Vec<u8>) -> Res<()> {
        let m = ClientMessage::new(INPUT_STREAM, self.out_seq, 0, ptype, payload);
        let bytes = m.encode();
        tx.send(Message::Binary(bytes.clone().into()))
            .await
            .map_err(|_| text::WEBSOCKET().to_string())?;
        self.unacked.insert(self.out_seq, (bytes, Instant::now()));
        self.out_seq += 1;
        Ok(())
    }

    async fn resend(&mut self, tx: &mut WsTx) -> Res<()> {
        let now = Instant::now();
        let ws_err = |_| text::WEBSOCKET().to_string();
        for (bytes, sent) in self.unacked.values_mut() {
            if now.duration_since(*sent) >= RESEND_AFTER {
                tx.feed(Message::Binary(bytes.clone().into()))
                    .await
                    .map_err(ws_err)?;
                *sent = now;
            }
        }
        // Também despacha pongs que o tungstenite tenha enfileirado.
        tx.flush().await.map_err(ws_err)
    }

    async fn handle(
        &mut self,
        m: ClientMessage,
        tx: &mut WsTx,
        lw: &mut WriteHalf<DuplexStream>,
    ) -> Res<Flow> {
        match m.message_type.as_str() {
            OUTPUT_STREAM => {
                // Ack sempre, inclusive de duplicata (nosso ack anterior pode ter se perdido).
                tx.send(Message::Binary(ClientMessage::ack_for(&m).encode().into()))
                    .await
                    .map_err(|_| text::WEBSOCKET().to_string())?;
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
                let a: Acknowledge =
                    serde_json::from_slice(&m.payload).map_err(|_| text::PROTOCOL().to_string())?;
                self.unacked.remove(&a.sequence_number);
            }
            CHANNEL_CLOSED => {
                let c: ChannelClosed =
                    serde_json::from_slice(&m.payload).unwrap_or(ChannelClosed {
                        output: String::new(),
                    });
                if c.output.trim().is_empty() {
                    return Ok(Flow::End);
                }
                return Err(text::channel_closed(&clean(&c.output)));
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
    ) -> Res<Flow> {
        match m.payload_type {
            payload_type::OUTPUT => {
                if lw.write_all(&m.payload).await.is_err() {
                    return Ok(Flow::End);
                }
            }
            payload_type::HANDSHAKE_REQUEST => {
                let req: HandshakeRequest =
                    serde_json::from_slice(&m.payload).map_err(|_| text::PROTOCOL().to_string())?;
                let (resp, unsupported) = handshake_response(&req);
                let body = serde_json::to_vec(&resp).map_err(|_| text::PROTOCOL().to_string())?;
                self.send_input(tx, payload_type::HANDSHAKE_RESPONSE, body)
                    .await?;
                if let Some(a) = unsupported {
                    return Err(text::unsupported_action(&clean(&a)));
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
                    .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);
                match v {
                    flag::CONNECT_TO_PORT_ERROR => return Err(text::PORT_ERROR().to_string()),
                    flag::TERMINATE_SESSION | flag::DISCONNECT_TO_PORT => return Ok(Flow::End),
                    _ => {}
                }
            }
            payload_type::ERROR | payload_type::STDERR => {
                return Err(text::agent_error(&clean(&String::from_utf8_lossy(
                    &m.payload,
                ))));
            }
            payload_type::EXIT_CODE => return Ok(Flow::End),
            _ => {}
        }
        Ok(Flow::Continue)
    }
}

/// Resposta ao `HandshakeRequest`: só `SessionType` é aceito; qualquer outra ação (ex.:
/// `KMSEncryption`) é recusada e devolvida para virar erro.
fn handshake_response(req: &HandshakeRequest) -> (HandshakeResponse, Option<String>) {
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
    (resp, unsupported)
}

/// Texto vindo do serviço/agente: sem controle e com tamanho limitado.
fn clean(s: &str) -> String {
    myssh_ssh::sanitize_text(s.trim(), 300, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_aceita_session_type_e_recusa_kms() {
        let req: HandshakeRequest = serde_json::from_str(
            r#"{"AgentVersion":"3.3","RequestedClientActions":[
                {"ActionType":"SessionType","ActionParameters":{"SessionType":"Port"}},
                {"ActionType":"KMSEncryption","ActionParameters":{"KMSKeyId":"k"}}]}"#,
        )
        .unwrap();
        let (resp, unsupported) = handshake_response(&req);
        assert_eq!(unsupported.as_deref(), Some("KMSEncryption"));
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["ClientVersion"], CLIENT_VERSION);
        assert_eq!(v["ProcessedClientActions"][0]["ActionStatus"], 1);
        assert_eq!(v["ProcessedClientActions"][1]["ActionStatus"], 3);

        let req: HandshakeRequest = serde_json::from_str(
            r#"{"AgentVersion":"3.3","RequestedClientActions":[{"ActionType":"SessionType"}]}"#,
        )
        .unwrap();
        assert!(handshake_response(&req).1.is_none());
    }
}
