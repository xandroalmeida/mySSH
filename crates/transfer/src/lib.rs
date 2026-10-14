//! Transferência SFTP do mySSH (sem UI): navegador remoto, fila de upload/download,
//! retomada validada e `FileTree` remoto para a sincronização. Contrato: `docs/contratos/transfer.md`.
//!
//! # Garantias
//! - Escrita em temporário `.<nome>.myssh-part-<rand>` no mesmo diretório; publicação por rename
//!   só depois de conferir o tamanho final. Local: rename atômico. Remoto: `SSH_FXP_RENAME`
//!   (não sobrescreve); com destino existente o arquivo antigo vai para um backup, o novo é
//!   renomeado e o backup removido — **não atômico** (a biblioteca SFTP não expõe
//!   `posix-rename@openssh.com`).
//! - Cancelamento cooperativo remove o temporário. Perda de conexão preserva temporário + sidecar
//!   (retomada); qualquer outro erro os remove.
//! - Retomada só com sidecar válido (mesmo nome, tamanho e mtime da origem), temporário existente
//!   com tamanho plausível e o trecho final (64 KiB) idêntico à origem. Só para arquivos >= 4 MiB.
//! - Symlinks nunca são seguidos (pulados e reportados via `FileError`, contados em `skipped`);
//!   nomes remotos são validados contra traversal e contra regras do SO local; nomes que colidem
//!   por caixa/normalização no destino viram erro explícito, nunca sobrescrita silenciosa.
//! - Permissões só com `preserve_perms`, mascaradas a `0o777`.
//! - Nomes com controle C0/C1/DEL, bidi ou invisíveis são recusados no destino local (qualquer SO)
//!   e ocultados na listagem do navegador (ver `RemoteBrowser::list_with_report`).
//! - Download nunca grava mais que o tamanho declarado no `lstat`: bytes extras → "origem
//!   alterada" e o temporário é removido. A varredura tem limites de profundidade (64) e de
//!   entradas (1.000.000), ajustáveis em `TransferManager::set_scan_limits`.
//! - Temporários locais nascem `0600` (unix); ao publicar, o modo final é o da origem
//!   (`preserve_perms`) ou o padrão do usuário (`0666 & !umask`). Sidecar `.myssh-resume` é
//!   escrito via `create_new` + rename e nunca lido/escrito através de symlink.
//!
//! # Desvios/acréscimos ao contrato
//! - `Cancelled` é o evento final de um cancelamento (não é seguido de `Finished`).
//! - Itens pulados na varredura (symlink, tipo não suportado) geram `FileError` informativo e
//!   contam em `skipped` (não em `failed`); idem avisos como "não foi possível preservar a data".
//! - `RemoteOrLocalMeta`, `ConflictResponder` e `RemoteBrowser::stat` foram definidos aqui.
//! - `TransferManager::{set_local_case_insensitive, stats}`, `RemoteTree` e `LocalOs` são extras.

mod browser;
mod engine;
mod error;
mod manager;
pub mod names;
pub mod rate;
mod scan;
mod side;
pub mod sidecar;
mod tree;

pub use browser::{Listing, RemoteBrowser};
pub use error::TransferError;
pub use manager::{TransferManager, TransferStats};
pub use names::LocalOs;
pub use tree::RemoteTree;

use std::path::PathBuf;
use std::time::SystemTime;
use tokio::sync::oneshot;

/// Reexportado de `myssh-ssh`: chamado após cada evento enfileirado (acorda a UI).
pub use myssh_ssh::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub perms: Option<u32>,
}

impl RemoteEntry {
    /// Nome sanitizado para exibir na UI (caracteres de controle/bidi/invisíveis viram U+FFFD,
    /// tamanho limitado). `RemoteBrowser::list` já filtra nomes inseguros; isto é a segunda
    /// barreira para entradas montadas por outros caminhos.
    pub fn display_name(&self) -> String {
        names::display_name(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overwrite {
    /// Pergunta por arquivo (`TransferEvent::Conflict`).
    #[default]
    Ask,
    Overwrite,
    Skip,
    /// "nome (1).ext".
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictDecision {
    Overwrite,
    Skip,
    Rename,
    OverwriteAll,
    SkipAll,
    /// Cancela a transferência inteira.
    Cancel,
}

#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub direction: Direction,
    /// Upload: origem (arquivo ou diretório). Download: diretório de destino (pai).
    pub local: PathBuf,
    /// Upload: diretório de destino (pai). Download: origem (arquivo ou diretório).
    pub remote: String,
    pub overwrite: Overwrite,
    pub preserve_mtime: bool,
    pub preserve_perms: bool,
}

impl TransferRequest {
    /// Padrões do contrato: `Ask`, `preserve_mtime = true`, `preserve_perms = false`.
    pub fn new(direction: Direction, local: impl Into<PathBuf>, remote: impl Into<String>) -> Self {
        TransferRequest {
            direction,
            local: local.into(),
            remote: remote.into(),
            overwrite: Overwrite::Ask,
            preserve_mtime: true,
            preserve_perms: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TransferId(pub u64);

/// Metadados do item que já existe no destino.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteOrLocalMeta {
    pub kind: Kind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

/// Canal de resposta a um conflito. Soltar sem responder cancela a transferência.
#[derive(Debug)]
pub struct ConflictResponder(Option<oneshot::Sender<ConflictDecision>>);

impl ConflictResponder {
    pub(crate) fn new(tx: oneshot::Sender<ConflictDecision>) -> Self {
        Self(Some(tx))
    }

    /// Cria um responder e o receptor da resposta (para UIs/testes que simulam o motor).
    pub fn channel() -> (Self, oneshot::Receiver<ConflictDecision>) {
        let (tx, rx) = oneshot::channel();
        (Self::new(tx), rx)
    }

    pub fn answer(mut self, d: ConflictDecision) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(d);
        }
    }
}

#[derive(Debug)]
pub enum TransferEvent {
    Queued {
        id: TransferId,
        label: String,
    },
    /// Contagem antes de começar (repetido durante a varredura; o último valor é o total).
    Scanning {
        id: TransferId,
        files: u64,
        bytes: u64,
    },
    Progress {
        id: TransferId,
        current: String,
        bytes_done: u64,
        bytes_total: u64,
        files_done: u64,
        files_total: u64,
        rate_bps: u64,
    },
    Conflict {
        id: TransferId,
        path: String,
        existing: RemoteOrLocalMeta,
        responder: ConflictResponder,
    },
    FileError {
        id: TransferId,
        path: String,
        message: String,
    },
    Finished {
        id: TransferId,
        ok: u64,
        failed: u64,
        skipped: u64,
    },
    Cancelled {
        id: TransferId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responder_channel_entrega_a_decisao_e_soltar_fecha_o_canal() {
        let (r, mut rx) = ConflictResponder::channel();
        r.answer(ConflictDecision::Skip);
        assert_eq!(rx.try_recv().unwrap(), ConflictDecision::Skip);
        let (r, mut rx) = ConflictResponder::channel();
        drop(r);
        assert!(matches!(
            rx.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        ));
    }
}
