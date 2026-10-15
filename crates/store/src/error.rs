//! Erros do armazenamento. Mensagens em pt-BR, sem host/usuário/caminhos.

use myssh_core::{ProfileError, ProfileId};
use rusqlite::ffi;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{}", myssh_i18n::tr!("Não foi possível abrir o banco de dados local.", "Could not open the local database.", "No se pudo abrir la base de datos local."))]
    Open,
    #[error("{}", myssh_i18n::tr!("Falha ao acessar o banco de dados local.", "Failed to access the local database.", "Error al acceder a la base de datos local."))]
    Database,
    #[error("{}", myssh_i18n::tr!("O banco de dados local foi criado por uma versão mais nova do aplicativo.", "The local database was created by a newer version of the app.", "La base de datos local fue creada por una versión más reciente de la aplicación."))]
    DatabaseTooNew,
    #[error("{}", myssh_i18n::tr!("Os dados locais estão corrompidos ou em formato inesperado.", "The local data is corrupted or in an unexpected format.", "Los datos locales están dañados o tienen un formato inesperado."))]
    Corrupt,
    #[error("{}", myssh_i18n::tr!("Item não encontrado.", "Item not found.", "Elemento no encontrado."))]
    NotFound,
    #[error("{}", myssh_i18n::tr!("Já existe um item com esse identificador.", "An item with this identifier already exists.", "Ya existe un elemento con ese identificador."))]
    AlreadyExists,
    #[error("{}", myssh_i18n::tr!("Referência inválida: pasta, jump host ou credencial inexistente.", "Invalid reference: folder, jump host or credential does not exist.", "Referencia no válida: la carpeta, el jump host o la credencial no existe."))]
    InvalidReference,
    #[error("{0}")]
    InvalidProfile(#[from] ProfileError),
    #[error("{}", myssh_i18n::tr!("A cadeia de jump hosts forma um ciclo.", "The jump host chain forms a cycle.", "La cadena de jump hosts forma un ciclo."))]
    JumpCycle,
    #[error("{}", myssh_i18n::tr!("A pasta não pode ser colocada dentro de si mesma ou de uma subpasta.", "A folder cannot be placed inside itself or one of its subfolders.", "Una carpeta no puede colocarse dentro de sí misma ni de una subcarpeta."))]
    FolderCycle,
    #[error("{}", myssh_i18n::tr!("O nome da pasta é obrigatório.", "The folder name is required.", "El nombre de la carpeta es obligatorio."))]
    EmptyFolderName,
    #[error("{}", myssh_i18n::tr!("O rótulo da credencial é obrigatório.", "The credential label is required.", "La etiqueta de la credencial es obligatoria."))]
    EmptyCredentialLabel,
    #[error("{}", myssh_i18n::tr!(format!("A credencial está em uso por {} perfil(is) e não pode ser removida.", .0.len()), format!("The credential is used by {} profile(s) and cannot be removed.", .0.len()), format!("La credencial está en uso por {} perfil(es) y no se puede eliminar.", .0.len())))]
    CredentialInUse(Vec<ProfileId>),
    #[error("{}", myssh_i18n::tr!(format!("Arquivo de configuração inválido: {}", .0), format!("Invalid configuration file: {}", .0), format!("Archivo de configuración no válido: {}", .0)))]
    InvalidConfig(String),
    #[error("{}", myssh_i18n::tr!(format!("Versão de esquema da configuração não suportada ({}).", .0), format!("Unsupported configuration schema version ({}).", .0), format!("Versión de esquema de configuración no compatible ({}).", .0)))]
    UnsupportedSchema(u32),
    #[error("{}", myssh_i18n::tr!("Já existe uma chave diferente do mesmo algoritmo para este endpoint; use a substituição explícita.", "A different key of the same algorithm already exists for this endpoint; use explicit replacement.", "Ya existe una clave distinta del mismo algoritmo para este endpoint; usa la sustitución explícita."))]
    HostKeyConflict,
    #[error("{}", myssh_i18n::tr!("Estado interno do armazenamento indisponível.", "Internal storage state unavailable.", "Estado interno del almacenamiento no disponible."))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mensagens_em_ingles_e_espanhol() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(StoreError::NotFound.to_string(), "Item not found.");
            assert!(StoreError::UnsupportedSchema(9).to_string().contains("(9)"));
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(StoreError::NotFound.to_string(), "Elemento no encontrado.");
        });
    }
}
