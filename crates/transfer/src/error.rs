//! Erros da transferência, com mensagens pt-BR claras e sem segredos.

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::StatusCode;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    #[error("{}", myssh_i18n::tr!("Permissão negada.", "Permission denied.", "Permiso denegado."))]
    PermissionDenied,
    #[error("{}", myssh_i18n::tr!("Sem espaço disponível no destino.", "No space left on the destination.", "Sin espacio disponible en el destino."))]
    NoSpace,
    #[error("{}", myssh_i18n::tr!("Arquivo ou diretório não encontrado.", "File or directory not found.", "Archivo o directorio no encontrado."))]
    NotFound,
    #[error("{}", myssh_i18n::tr!("A conexão com o servidor foi perdida.", "The connection to the server was lost.", "Se perdió la conexión con el servidor."))]
    ConnectionLost,
    #[error("{}", myssh_i18n::tr!(format!("Nome inválido neste sistema: «{}».", .0), format!("Invalid name on this system: «{}».", .0), format!("Nombre no válido en este sistema: «{}».", .0)))]
    InvalidLocalName(String),
    #[error("{}", myssh_i18n::tr!(format!("Nome com caracteres de controle ou invisíveis: «{}».", .0), format!("Name with control or invisible characters: «{}».", .0), format!("Nombre con caracteres de control o invisibles: «{}».", .0)))]
    UnsafeName(String),
    #[error("{}", myssh_i18n::tr!(format!("Varredura interrompida: {}.", .0), format!("Scan stopped: {}.", .0), format!("Exploración interrumpida: {}.", .0)))]
    ScanLimit(String),
    #[error("{}", myssh_i18n::tr!(format!("Caminho fora da raiz da transferência: «{}».", .0), format!("Path outside the transfer root: «{}».", .0), format!("Ruta fuera de la raíz de la transferencia: «{}».", .0)))]
    OutsideRoot(String),
    #[error("{}", myssh_i18n::tr!("origem alterada durante a transferência", "source changed during the transfer", "origen modificado durante la transferencia"))]
    SourceChanged,
    #[error("{}", myssh_i18n::tr!(format!("Tamanho final inesperado (esperado {} bytes, obtido {}); não publicado.", expected, actual), format!("Unexpected final size (expected {} bytes, got {}); not published.", expected, actual), format!("Tamaño final inesperado (esperado {} bytes, obtenido {}); no publicado.", expected, actual)))]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("{}", myssh_i18n::tr!(format!("Conflito de nomes: {} diferem só por maiúsculas/minúsculas ou normalização Unicode e colidiriam neste sistema; nenhum foi baixado.", .0.join(", ")), format!("Name conflict: {} differ only by letter case or Unicode normalization and would collide on this system; none was downloaded.", .0.join(", ")), format!("Conflicto de nombres: {} solo se diferencian por mayúsculas/minúsculas o normalización Unicode y colisionarían en este sistema; no se descargó ninguno.", .0.join(", "))))]
    CaseCollision(Vec<String>),
    #[error("{}", myssh_i18n::tr!(format!("O destino já existe e é {}; não foi sobrescrito.", .0), format!("The destination already exists and is {}; it was not overwritten.", .0), format!("El destino ya existe y es {}; no se sobrescribió.", .0)))]
    DestinationKind(&'static str),
    #[error("{}", myssh_i18n::tr!("Link simbólico ignorado.", "Symbolic link skipped.", "Enlace simbólico omitido."))]
    SymlinkSkipped,
    #[error("{}", myssh_i18n::tr!("Tipo de arquivo não suportado (ignorado).", "Unsupported file type (skipped).", "Tipo de archivo no compatible (omitido)."))]
    UnsupportedKind,
    #[error("{}", myssh_i18n::tr!("Transferência cancelada.", "Transfer cancelled.", "Transferencia cancelada."))]
    Cancelled,
    #[error("{}", myssh_i18n::tr!(format!("Erro de E/S: {}", .0), format!("I/O error: {}", .0), format!("Error de E/S: {}", .0)))]
    Io(String),
    #[error("{}", myssh_i18n::tr!(format!("Erro do servidor SFTP: {}", .0), format!("SFTP server error: {}", .0), format!("Error del servidor SFTP: {}", .0)))]
    Remote(String),
}

impl TransferError {
    pub fn is_connection_lost(&self) -> bool {
        matches!(self, TransferError::ConnectionLost)
    }

    /// Erro de protocolo SFTP.
    pub fn from_sftp(e: &SftpError) -> Self {
        match e {
            SftpError::Status(s) => match s.status_code {
                StatusCode::NoSuchFile => TransferError::NotFound,
                StatusCode::PermissionDenied => TransferError::PermissionDenied,
                StatusCode::NoConnection | StatusCode::ConnectionLost => {
                    TransferError::ConnectionLost
                }
                StatusCode::OpUnsupported => TransferError::Remote(
                    myssh_i18n::tr!(
                        "operação não suportada pelo servidor",
                        "operation not supported by the server",
                        "operación no compatible con el servidor"
                    )
                    .into(),
                ),
                _ => TransferError::Remote(sanitize(&s.error_message)),
            },
            SftpError::IO(_) | SftpError::Timeout => TransferError::ConnectionLost,
            SftpError::UnexpectedBehavior(m)
                if m.contains("RecvError") || m.contains("SendError") =>
            {
                TransferError::ConnectionLost
            }
            SftpError::UnexpectedBehavior(_) | SftpError::UnexpectedPacket => {
                TransferError::Remote(
                    myssh_i18n::tr!(
                        "resposta inesperada do servidor",
                        "unexpected response from the server",
                        "respuesta inesperada del servidor"
                    )
                    .into(),
                )
            }
            SftpError::Limited(_) => TransferError::Remote(
                myssh_i18n::tr!(
                    "limite do servidor excedido",
                    "server limit exceeded",
                    "límite del servidor excedido"
                )
                .into(),
            ),
        }
    }

    /// Erro de E/S local ou do `AsyncRead`/`AsyncWrite` do arquivo SFTP (que embute o erro SFTP
    /// ou só a mensagem do status).
    pub fn from_io(e: &io::Error) -> Self {
        if let Some(inner) = e.get_ref().and_then(|i| i.downcast_ref::<SftpError>()) {
            return Self::from_sftp(inner);
        }
        if e.get_ref().is_some() {
            // Mensagem de status do servidor repassada como texto.
            let m = e.to_string();
            let l = m.to_ascii_lowercase();
            if l.contains("permission denied") {
                return TransferError::PermissionDenied;
            }
            if l.contains("no such file") {
                return TransferError::NotFound;
            }
            if l.contains("connection lost") || l.contains("no connection") {
                return TransferError::ConnectionLost;
            }
            return TransferError::Remote(sanitize(&m));
        }
        use io::ErrorKind as K;
        match e.raw_os_error() {
            Some(28) => return TransferError::NoSpace, // ENOSPC
            Some(122) if cfg!(target_os = "linux") => return TransferError::NoSpace, // EDQUOT
            Some(69) if cfg!(target_os = "macos") => return TransferError::NoSpace, // EDQUOT
            Some(36) if cfg!(target_os = "linux") => {
                return TransferError::Io(
                    myssh_i18n::tr!(
                        "nome de arquivo longo demais",
                        "file name too long",
                        "nombre de archivo demasiado largo"
                    )
                    .into(),
                );
            }
            Some(63) if cfg!(target_os = "macos") => {
                return TransferError::Io(
                    myssh_i18n::tr!(
                        "nome de arquivo longo demais",
                        "file name too long",
                        "nombre de archivo demasiado largo"
                    )
                    .into(),
                );
            }
            _ => {}
        }
        match e.kind() {
            K::PermissionDenied => TransferError::PermissionDenied,
            K::NotFound => TransferError::NotFound,
            K::StorageFull | K::QuotaExceeded => TransferError::NoSpace,
            K::BrokenPipe
            | K::ConnectionReset
            | K::ConnectionAborted
            | K::NotConnected
            | K::UnexpectedEof
            | K::TimedOut => TransferError::ConnectionLost,
            K::AlreadyExists => TransferError::Io(
                myssh_i18n::tr!(
                    "o item já existe",
                    "the item already exists",
                    "el elemento ya existe"
                )
                .into(),
            ),
            K::InvalidInput | K::InvalidData => TransferError::Io(
                myssh_i18n::tr!(
                    "nome ou dado inválido",
                    "invalid name or data",
                    "nombre o dato no válido"
                )
                .into(),
            ),
            K::ReadOnlyFilesystem => TransferError::Io(
                myssh_i18n::tr!(
                    "sistema de arquivos somente leitura",
                    "read-only file system",
                    "sistema de archivos de solo lectura"
                )
                .into(),
            ),
            _ => TransferError::Io(
                myssh_i18n::tr!(
                    "falha de leitura ou escrita",
                    "read or write failure",
                    "error de lectura o escritura"
                )
                .into(),
            ),
        }
    }
}

/// Texto vindo do servidor: sem controles, tamanho limitado.
pub(crate) fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mensagens_pt_br() {
        assert_eq!(
            TransferError::PermissionDenied.to_string(),
            "Permissão negada."
        );
        assert!(TransferError::NoSpace.to_string().contains("espaço"));
        assert_eq!(
            TransferError::SourceChanged.to_string(),
            "origem alterada durante a transferência"
        );
        assert!(
            TransferError::CaseCollision(vec!["A".into(), "a".into()])
                .to_string()
                .contains("A, a")
        );
    }

    #[test]
    fn mensagens_em_ingles_e_espanhol() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(
                TransferError::PermissionDenied.to_string(),
                "Permission denied."
            );
            assert!(
                TransferError::Io("x".into())
                    .to_string()
                    .starts_with("I/O error")
            );
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(
                TransferError::Cancelled.to_string(),
                "Transferencia cancelada."
            );
        });
    }

    #[test]
    fn classifica_io() {
        let e = io::Error::other("Permission denied");
        assert_eq!(TransferError::from_io(&e), TransferError::PermissionDenied);
        let e = io::Error::from(io::ErrorKind::StorageFull);
        assert_eq!(TransferError::from_io(&e), TransferError::NoSpace);
        let e = io::Error::from_raw_os_error(28);
        assert_eq!(TransferError::from_io(&e), TransferError::NoSpace);
        let e = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(TransferError::from_io(&e), TransferError::NotFound);
        let e = io::Error::from(SftpError::Timeout);
        assert_eq!(TransferError::from_io(&e), TransferError::ConnectionLost);
        let e = io::Error::other("Failure");
        assert!(matches!(
            TransferError::from_io(&e),
            TransferError::Remote(_)
        ));
    }
}
