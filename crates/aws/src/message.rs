//! Formato binário das mensagens do data channel do SSM (`ClientMessage` do session-manager-plugin).
//!
//! Layout (inteiros big-endian):
//! ```text
//!   0  u32   HeaderLength (= 116)
//!   4  [32]  MessageType (ASCII, completado com espaços)
//!  36  u32   SchemaVersion
//!  40  u64   CreatedDate (epoch ms)
//!  48  i64   SequenceNumber
//!  56  u64   Flags
//!  64  [16]  MessageId (UUID com as metades trocadas: bytes 8..16, depois 0..8)
//!  80  [32]  PayloadDigest (SHA-256 do payload)
//! 112  u32   PayloadType
//! 116  u32   PayloadLength
//! 120  ...   Payload
//! ```

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const HEADER_LENGTH: u32 = 116;
const TYPE_LEN: usize = 32;
const PAYLOAD_OFFSET: usize = HEADER_LENGTH as usize + 4;

pub const INPUT_STREAM: &str = "input_stream_data";
pub const OUTPUT_STREAM: &str = "output_stream_data";
pub const ACKNOWLEDGE: &str = "acknowledge";
pub const CHANNEL_CLOSED: &str = "channel_closed";
pub const START_PUBLICATION: &str = "start_publication";
pub const PAUSE_PUBLICATION: &str = "pause_publication";

pub mod payload_type {
    pub const OUTPUT: u32 = 1;
    pub const ERROR: u32 = 2;
    pub const HANDSHAKE_REQUEST: u32 = 5;
    pub const HANDSHAKE_RESPONSE: u32 = 6;
    pub const HANDSHAKE_COMPLETE: u32 = 7;
    pub const FLAG: u32 = 10;
    pub const STDERR: u32 = 11;
    pub const EXIT_CODE: u32 = 12;
}

/// Valores do payload `FLAG` (u32 big-endian).
pub mod flag {
    pub const DISCONNECT_TO_PORT: u32 = 1;
    pub const TERMINATE_SESSION: u32 = 2;
    pub const CONNECT_TO_PORT_ERROR: u32 = 3;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMessage {
    pub message_type: String,
    pub schema_version: u32,
    pub created_date: u64,
    pub sequence_number: i64,
    pub flags: u64,
    pub message_id: Uuid,
    pub payload_type: u32,
    pub payload: Vec<u8>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl ClientMessage {
    pub fn new(
        message_type: &str,
        sequence_number: i64,
        flags: u64,
        payload_type: u32,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            message_type: message_type.to_string(),
            schema_version: 1,
            created_date: now_ms(),
            sequence_number,
            flags,
            message_id: Uuid::new_v4(),
            payload_type,
            payload,
        }
    }

    /// Ack de uma `output_stream_data` recebida (sequência 0, flags 3, como o plugin).
    pub fn ack_for(m: &ClientMessage) -> Self {
        let ack = Acknowledge {
            message_type: m.message_type.clone(),
            message_id: m.message_id.to_string(),
            sequence_number: m.sequence_number,
            is_sequential: true,
        };
        let payload = serde_json::to_vec(&ack).expect("ack serializa");
        Self::new(ACKNOWLEDGE, 0, 3, 0, payload)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(PAYLOAD_OFFSET + self.payload.len());
        b.extend_from_slice(&HEADER_LENGTH.to_be_bytes());
        let mut t = [b' '; TYPE_LEN];
        let n = self.message_type.len().min(TYPE_LEN);
        t[..n].copy_from_slice(&self.message_type.as_bytes()[..n]);
        b.extend_from_slice(&t);
        b.extend_from_slice(&self.schema_version.to_be_bytes());
        b.extend_from_slice(&self.created_date.to_be_bytes());
        b.extend_from_slice(&self.sequence_number.to_be_bytes());
        b.extend_from_slice(&self.flags.to_be_bytes());
        let id = self.message_id.as_bytes();
        b.extend_from_slice(&id[8..16]);
        b.extend_from_slice(&id[0..8]);
        b.extend_from_slice(&Sha256::digest(&self.payload));
        b.extend_from_slice(&self.payload_type.to_be_bytes());
        b.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        b.extend_from_slice(&self.payload);
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        ensure!(
            b.len() >= PAYLOAD_OFFSET,
            "mensagem curta ({} bytes)",
            b.len()
        );
        let u32_at = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
        let u64_at = |o: usize| u64::from_be_bytes(b[o..o + 8].try_into().unwrap());
        let header_length = u32_at(0) as usize;
        ensure!(
            header_length >= HEADER_LENGTH as usize && b.len() >= header_length + 4,
            "HeaderLength inválido ({header_length})"
        );
        let message_type = String::from_utf8_lossy(&b[4..4 + TYPE_LEN])
            .trim_matches(|c| c == ' ' || c == '\0')
            .to_string();
        let mut id = [0u8; 16];
        id[8..16].copy_from_slice(&b[64..72]);
        id[0..8].copy_from_slice(&b[72..80]);
        let payload_len = u32_at(header_length) as usize;
        let start = header_length + 4;
        let Some(payload) = b.get(start..start + payload_len) else {
            bail!(
                "PayloadLength ({payload_len}) além do fim da mensagem ({} bytes)",
                b.len()
            );
        };
        Ok(Self {
            message_type,
            schema_version: u32_at(36),
            created_date: u64_at(40),
            sequence_number: u64_at(48) as i64,
            flags: u64_at(56),
            message_id: Uuid::from_bytes(id),
            payload_type: u32_at(112),
            payload: payload.to_vec(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Acknowledge {
    #[serde(rename = "AcknowledgedMessageType")]
    pub message_type: String,
    #[serde(rename = "AcknowledgedMessageId")]
    pub message_id: String,
    #[serde(rename = "AcknowledgedMessageSequenceNumber")]
    pub sequence_number: i64,
    #[serde(rename = "IsSequentialMessage")]
    pub is_sequential: bool,
}

/// Primeira mensagem (texto) no websocket: autentica o data channel com o token do `StartSession`.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OpenDataChannel<'a> {
    pub message_schema_version: &'a str,
    pub request_id: String,
    pub token_value: &'a str,
    pub client_id: String,
    pub client_version: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HandshakeRequest {
    #[serde(default)]
    pub requested_client_actions: Vec<RequestedClientAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RequestedClientAction {
    pub action_type: String,
}

pub mod action_status {
    pub const SUCCESS: u32 = 1;
    pub const UNSUPPORTED: u32 = 3;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct HandshakeResponse {
    pub client_version: String,
    pub processed_client_actions: Vec<ProcessedClientAction>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProcessedClientAction {
    pub action_type: String,
    pub action_status: u32,
    pub action_result: Option<serde_json::Value>,
    pub error: String,
}

/// Payload de `channel_closed`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChannelClosed {
    #[serde(default)]
    pub output: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let m = ClientMessage::new(
            INPUT_STREAM,
            42,
            0,
            payload_type::OUTPUT,
            b"SSH-2.0-x\r\n".to_vec(),
        );
        let b = m.encode();
        assert_eq!(b.len(), 120 + 11);
        assert_eq!(&b[4..21], b"input_stream_data");
        assert_eq!(b[21], b' ');
        assert_eq!(ClientMessage::decode(&b).unwrap(), m);
    }

    #[test]
    fn uuid_com_metades_trocadas() {
        let mut m = ClientMessage::new(ACKNOWLEDGE, 0, 3, 0, Vec::new());
        m.message_id = Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let b = m.encode();
        assert_eq!(
            &b[64..80],
            &[
                0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0, 0x11, 0x22, 0x33, 0x44, 0x55,
                0x66, 0x77
            ]
        );
    }

    #[test]
    fn digest_e_tamanho() {
        let m = ClientMessage::new(INPUT_STREAM, 0, 0, payload_type::OUTPUT, b"abc".to_vec());
        let b = m.encode();
        assert_eq!(&b[80..112], Sha256::digest(b"abc").as_slice());
        assert_eq!(u32::from_be_bytes(b[116..120].try_into().unwrap()), 3);
    }

    #[test]
    fn ack_json() {
        let m = ClientMessage::new(OUTPUT_STREAM, 7, 0, payload_type::OUTPUT, Vec::new());
        let ack = ClientMessage::ack_for(&m);
        let v: serde_json::Value = serde_json::from_slice(&ack.payload).unwrap();
        assert_eq!(v["AcknowledgedMessageType"], "output_stream_data");
        assert_eq!(v["AcknowledgedMessageSequenceNumber"], 7);
        assert_eq!(v["AcknowledgedMessageId"], m.message_id.to_string());
        assert_eq!(ack.flags, 3);
    }

    #[test]
    fn rejeita_truncada() {
        let mut b = ClientMessage::new(INPUT_STREAM, 0, 0, 1, vec![1; 10]).encode();
        b.truncate(125);
        assert!(ClientMessage::decode(&b).is_err());
        assert!(ClientMessage::decode(&b[..50]).is_err());
    }
}
