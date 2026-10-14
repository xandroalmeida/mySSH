//! Sessão SSH do mySSH (sem dependência de UI).
//!
//! # Camadas
//! - [`Connection`]: conexão autenticada e compartilhável (`Arc`), com jump hosts, da qual se abrem
//!   canais ([`Connection::open_shell`], [`Connection::open_sftp`]). Encerra quando o último canal
//!   fecha ou por [`Connection::close`].
//! - [`spawn`]: sessão de terminal pronta (conexão + shell) para a UI, que conversa por canais
//!   **limitados** (backpressure): recebe [`SessionEvent`]s em `evt_rx` e envia [`SessionCommand`]s
//!   por `cmd_tx`. A rede roda num runtime tokio fornecido pelo chamador; nada aqui bloqueia a UI.
//!
//! # Jump hosts
//! `ConnectRequest::jumps` lista os saltos em ordem. Cada salto valida a **própria** host key contra
//! o `HostTrustStore` e autentica com as próprias credenciais; os eventos trazem `hop`
//! (`Some(i)` = salto `i`, `None` = destino). Agent forwarding nunca é habilitado.
//!
//! # Algoritmos habilitados (padrões modernos do russh 0.64, menos `ssh-rsa`/SHA-1)
//! - KEX: `mlkem768x25519-sha256`, `curve25519-sha256`, `diffie-hellman-group-exchange-sha256`,
//!   `diffie-hellman-group{14-sha256,15..18-sha512}`.
//! - Chaves de host: `ssh-ed25519`, `ecdsa-sha2-nistp{256,384,521}`, `rsa-sha2-512`, `rsa-sha2-256`.
//!   `ssh-rsa` (SHA-1) e `ssh-dss` **desabilitados** (o padrão do russh inclui `ssh-rsa`; removido
//!   em `connection::client_config`).
//! - Cifras: `chacha20-poly1305`, `aes256-gcm`, `aes{256,192,128}-ctr`. Nenhuma CBC.
//! - MAC: `hmac-sha2-{512,256}-etm`, `hmac-sha2-{512,256}`. Nenhum SHA-1.

mod connection;
mod error;
mod sanitize;
mod session;
mod trust;

pub use connection::{
    AuthMaterial, Connection, EventSink, Hop, Sftp, ShellChannel, key_requires_passphrase,
};
pub use error::{AUTH_CANCELLED, AUTH_REJECTED, KEY_DECODE};
pub use sanitize::sanitize_text;
pub use session::{
    AuthResponder, ConnectRequest, HostKeyDecision, HostKeyResponder, Notify, PtySize,
    SessionCommand, SessionEvent, SessionHandle, spawn,
};
pub use trust::{fingerprint_sha256, host_key_from_public};

/// Capacidade do canal de eventos (cada `Data` agrega até [`MAX_DATA_BATCH`] bytes).
pub const EVENT_CHANNEL_CAP: usize = 32;
/// Capacidade do canal de comandos.
pub const CMD_CHANNEL_CAP: usize = 256;
/// Tamanho máximo de um lote `Data`.
pub const MAX_DATA_BATCH: usize = 64 * 1024;
