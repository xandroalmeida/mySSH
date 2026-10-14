//! Erros do armazenamento. Mensagens em pt-BR, sem host/usuário/caminhos.

use myssh_core::{ProfileError, ProfileId};
use rusqlite::ffi;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Não foi possível abrir o banco de dados local.")]
    Open,
    #[error("Falha ao acessar o banco de dados local.")]
    Database,
    #[error("O banco de dados local foi criado por uma versão mais nova do aplicativo.")]
    DatabaseTooNew,
    #[error("Os dados locais estão corrompidos ou em formato inesperado.")]
    Corrupt,
    #[error("Item não encontrado.")]
    NotFound,
    #[error("Já existe um item com esse identificador.")]
    AlreadyExists,
    #[error("Referência inválida: pasta, jump host ou credencial inexistente.")]
    InvalidReference,
    #[error("{0}")]
    InvalidProfile(#[from] ProfileError),
    #[error("A cadeia de jump hosts forma um ciclo.")]
    JumpCycle,
    #[error("A pasta não pode ser colocada dentro de si mesma ou de uma subpasta.")]
    FolderCycle,
    #[error("O nome da pasta é obrigatório.")]
    EmptyFolderName,
    #[error("O rótulo da credencial é obrigatório.")]
    EmptyCredentialLabel,
    #[error("A credencial está em uso por {} perfil(is) e não pode ser removida.", .0.len())]
    CredentialInUse(Vec<ProfileId>),
    #[error("Arquivo de configuração inválido: {0}")]
    InvalidConfig(String),
    #[error("Versão de esquema da configuração não suportada ({0}).")]
    UnsupportedSchema(u32),
    #[error(
        "Já existe uma chave diferente do mesmo algoritmo para este endpoint; use a substituição explícita."
    )]
    HostKeyConflict,
    #[error("Estado interno do armazenamento indisponível.")]
    Poisoned,
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        if let Some(err) = e.sqlite_error() {
            match err.extended_code {
                ffi::SQLITE_CONSTRAINT_FOREIGNKEY => return StoreError::InvalidReference,
                ffi::SQLITE_CONSTRAINT_PRIMARYKEY | ffi::SQLITE_CONSTRAINT_UNIQUE => {
                    return StoreError::AlreadyExists;
                }
                _ => {}
            }
        }
        StoreError::Database
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(_: serde_json::Error) -> Self {
        StoreError::Corrupt
    }
}

impl From<StoreError> for myssh_core::trust::TrustStoreError {
    fn from(e: StoreError) -> Self {
        myssh_core::trust::TrustStoreError::Storage(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;
