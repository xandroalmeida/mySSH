//! Erros da transferência, com mensagens pt-BR claras e sem segredos.

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::StatusCode;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    #[error("Permissão negada.")]
    PermissionDenied,
    #[error("Sem espaço disponível no destino.")]
    NoSpace,
    #[error("Arquivo ou diretório não encontrado.")]
    NotFound,
    #[error("A conexão com o servidor foi perdida.")]
    ConnectionLost,
    #[error("Nome inválido neste sistema: «{0}».")]
    InvalidLocalName(String),
    #[error("Nome com caracteres de controle ou invisíveis: «{0}».")]
    UnsafeName(String),
    #[error("Varredura interrompida: {0}.")]
    ScanLimit(String),
    #[error("Caminho fora da raiz da transferência: «{0}».")]
    OutsideRoot(String),
    #[error("origem alterada durante a transferência")]
    SourceChanged,
    #[error(
        "Tamanho final inesperado (esperado {expected} bytes, obtido {actual}); não publicado."
    )]
    SizeMismatch { expected: u64, actual: u64 },
    #[error(
        "Conflito de nomes: {} diferem só por maiúsculas/minúsculas ou normalização Unicode e colidiriam neste sistema; nenhum foi baixado.",
        .0.join(", ")
    )]
    CaseCollision(Vec<String>),
    #[error("O destino já existe e é {0}; não foi sobrescrito.")]
    DestinationKind(&'static str),
    #[error("Link simbólico ignorado.")]
    SymlinkSkipped,
    #[error("Tipo de arquivo não suportado (ignorado).")]
    UnsupportedKind,
    #[error("Transferência cancelada.")]
    Cancelled,
    #[error("Erro de E/S: {0}")]
    Io(String),
    #[error("Erro do servidor SFTP: {0}")]
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
                StatusCode::OpUnsupported => {
                    TransferError::Remote("operação não suportada pelo servidor".into())
                }
                _ => TransferError::Remote(sanitize(&s.error_message)),
            },
            SftpError::IO(_) | SftpError::Timeout => TransferError::ConnectionLost,
            SftpError::UnexpectedBehavior(m)
                if m.contains("RecvError") || m.contains("SendError") =>
            {
                TransferError::ConnectionLost
            }
            SftpError::UnexpectedBehavior(_) | SftpError::UnexpectedPacket => {
                TransferError::Remote("resposta inesperada do servidor".into())
            }
            SftpError::Limited(_) => TransferError::Remote("limite do servidor excedido".into()),
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
                return TransferError::Io("nome de arquivo longo demais".into());
            }
            Some(63) if cfg!(target_os = "macos") => {
                return TransferError::Io("nome de arquivo longo demais".into());
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
            K::AlreadyExists => TransferError::Io("o item já existe".into()),
            K::InvalidInput | K::InvalidData => TransferError::Io("nome ou dado inválido".into()),
            K::ReadOnlyFilesystem => {
                TransferError::Io("sistema de arquivos somente leitura".into())
            }
            _ => TransferError::Io("falha de leitura ou escrita".into()),
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
