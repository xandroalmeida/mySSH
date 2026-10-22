//! POC: túnel TCP para uma instância via AWS SSM Session Manager, em Rust puro.
//!
//! `StartSession` (SDK) devolve a URL do websocket do data channel e um token; o resto é o
//! protocolo do data channel (o mesmo que o `session-manager-plugin` fala): mensagens binárias
//! com número de sequência, ack, retransmissão e um handshake inicial com o agente.
//! O resultado é um stream `AsyncRead + AsyncWrite` que entra direto em `russh::client::connect_stream`.

pub mod channel;
pub mod message;

pub use channel::{SsmSession, SsmStream, open, start_session};
