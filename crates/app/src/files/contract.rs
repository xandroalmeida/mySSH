//! Tipos do contrato `docs/contratos/transfer.md`, definidos localmente enquanto o crate
//! `myssh-transfer` não existe. Na integração este módulo vira `pub use myssh_transfer::*;`
//! (mesmos nomes e formas). Itens marcados "EXTENSÃO" não estão no contrato e precisam ser
//! pedidos ao Opus.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub perms: Option<u32>,
}

/// Erro das operações remotas (mensagem em pt-BR, sem segredos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferError(pub String);

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransferError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Overwrite {
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictDecision {
    Overwrite,
    Skip,
    Rename,
    OverwriteAll,
    SkipAll,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    pub direction: Direction,
    /// Arquivo ou diretório local (origem no upload, destino-pai no download).
    pub local: PathBuf,
    /// Idem no remoto.
    pub remote: String,
    pub overwrite: Overwrite,
    pub preserve_mtime: bool,
    pub preserve_perms: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransferId(pub u64);

/// Metadados do item. O contrato cita `RemoteOrLocalMeta` sem definir os campos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOrLocalMeta {
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

/// Canal de resposta de um conflito. Descartá-lo sem responder equivale a `Cancel`.
/// (O contrato cita `ConflictResponder` sem definir a forma.)
#[derive(Debug, Clone)]
pub struct ConflictResponder(pub Sender<ConflictDecision>);

impl ConflictResponder {
    pub fn respond(&self, d: ConflictDecision) {
        let _ = self.0.send(d);
    }
}

#[derive(Debug, Clone)]
pub enum TransferEvent {
    Queued {
        id: TransferId,
        label: String,
    },
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
        /// Item que já existe no destino.
        existing: RemoteOrLocalMeta,
        /// EXTENSÃO: metadados do item que chegaria (para o diálogo comparar os dois).
        incoming: Option<RemoteOrLocalMeta>,
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
