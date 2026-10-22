//! Mensagens de erro em pt-BR, sanitizadas: nunca incluem senha, passphrase, hostname ou
//! texto bruto de bibliotecas que possa carregar dados sensíveis.

#![allow(non_snake_case)]

use std::io;

use myssh_i18n::tr;

pub(crate) fn KEYGEN() -> &'static str {
    tr!(
        "Falha ao gerar a chave temporária.",
        "Failed to generate the temporary key.",
        "No se pudo generar la clave temporal."
    )
}
pub(crate) fn TIMEOUT() -> &'static str {
    tr!(
        "Tempo esgotado ao conectar.",
        "Connection timed out.",
        "Se agotó el tiempo de conexión."
    )
}
pub(crate) fn CONNECTION_LOST() -> &'static str {
    tr!(
        "A conexão com o servidor foi perdida.",
        "The connection to the server was lost.",
        "Se perdió la conexión con el servidor."
    )
}
pub fn AUTH_REJECTED() -> &'static str {
    tr!(
        "Autenticação recusada pelo servidor.",
        "Authentication rejected by the server.",
        "Autenticación rechazada por el servidor."
    )
}
pub(crate) fn HOST_REJECTED() -> &'static str {
    tr!(
        "Conexão cancelada: host não confiável.",
        "Connection cancelled: untrusted host.",
        "Conexión cancelada: host no confiable."
    )
}
pub(crate) fn HOST_CHANGED() -> &'static str {
    tr!(
        "A chave do servidor mudou ou não é a confiável: conexão bloqueada (possível ataque).",
        "The server key changed or is not the trusted one: connection blocked (possible attack).",
        "La clave del servidor cambió o no es la de confianza: conexión bloqueada (posible ataque)."
    )
}
pub(crate) fn HOST_PROMPT_TIMEOUT() -> &'static str {
    tr!(
        "Sem resposta sobre a chave do servidor: conexão cancelada.",
        "No answer about the server key: connection cancelled.",
        "Sin respuesta sobre la clave del servidor: conexión cancelada."
    )
}
pub(crate) fn HOST_STORE() -> &'static str {
    tr!(
        "Falha ao consultar ou salvar os hosts confiáveis.",
        "Failed to read or save the trusted hosts.",
        "Error al consultar o guardar los hosts de confianza."
    )
}
pub(crate) fn HOST_CERT() -> &'static str {
    tr!(
        "Certificados de host ainda não são suportados.",
        "Host certificates are not supported yet.",
        "Los certificados de host aún no son compatibles."
    )
}
pub(crate) fn UI_GONE() -> &'static str {
    tr!(
        "Interface encerrada.",
        "Interface closed.",
        "Interfaz cerrada."
    )
}
#[allow(dead_code)]
pub(crate) fn UNSUPPORTED_AUTH() -> &'static str {
    tr!(
        "Este método de autenticação ainda não é suportado.",
        "This authentication method is not supported yet.",
        "Este método de autenticación aún no es compatible."
    )
}
pub(crate) fn JUMP_LOOP() -> &'static str {
    tr!(
        "A cadeia de jump hosts passa duas vezes pelo mesmo servidor.",
        "The jump host chain goes through the same server twice.",
        "La cadena de jump hosts pasa dos veces por el mismo servidor."
    )
}
pub(crate) fn JUMP_OPEN() -> &'static str {
    tr!(
        "O salto recusou encaminhar a conexão até o próximo servidor.",
        "The hop refused to forward the connection to the next server.",
        "El salto rechazó reenviar la conexión al siguiente servidor."
    )
}
/// Prompt de keyboard-interactive cancelado (diálogo fechado ou sem resposta a tempo).
pub fn AUTH_CANCELLED() -> &'static str {
    tr!(
        "Autenticação cancelada.",
        "Authentication cancelled.",
        "Autenticación cancelada."
    )
}
pub(crate) fn AUTH_PROMPT_TIMEOUT() -> &'static str {
    tr!(
        "Sem resposta ao pedido de autenticação: cancelado.",
        "No answer to the authentication prompt: cancelled.",
        "Sin respuesta a la solicitud de autenticación: cancelado."
    )
}
pub(crate) fn KI_PROTOCOL() -> &'static str {
    tr!(
        "O servidor enviou um pedido de autenticação inválido.",
        "The server sent an invalid authentication request.",
        "El servidor envió una solicitud de autenticación no válida."
    )
}
pub(crate) fn CONN_CLOSED() -> &'static str {
    tr!(
        "A conexão já foi encerrada.",
        "The connection has already been closed.",
        "La conexión ya fue cerrada."
    )
}
pub(crate) fn SFTP_FAILED() -> &'static str {
    tr!(
        "Não foi possível iniciar o subsistema SFTP.",
        "Could not start the SFTP subsystem.",
        "No se pudo iniciar el subsistema SFTP."
    )
}
pub(crate) fn NO_AGENT() -> &'static str {
    tr!(
        "Agente SSH indisponível (SSH_AUTH_SOCK não definido ou inacessível).",
        "SSH agent unavailable (SSH_AUTH_SOCK not set or unreachable).",
        "Agente SSH no disponible (SSH_AUTH_SOCK no definido o inaccesible)."
    )
}
pub(crate) fn AGENT_NO_KEYS() -> &'static str {
    tr!(
        "O agente SSH não tem chaves aceitas por este servidor.",
        "The SSH agent has no keys accepted by this server.",
        "El agente SSH no tiene claves aceptadas por este servidor."
    )
}
pub(crate) fn KEY_READ() -> &'static str {
    tr!(
        "Não foi possível ler o arquivo da chave privada.",
        "Could not read the private key file.",
        "No se pudo leer el archivo de la clave privada."
    )
}
pub(crate) fn KEY_NOT_FILE() -> &'static str {
    tr!(
        "O caminho da chave privada não é um arquivo comum (pasta, pipe ou dispositivo).",
        "The private key path is not a regular file (folder, pipe or device).",
        "La ruta de la clave privada no es un archivo normal (carpeta, pipe o dispositivo)."
    )
}
pub(crate) fn KEY_TOO_BIG() -> &'static str {
    tr!(
        "O arquivo da chave privada é grande demais para ser uma chave.",
        "The private key file is too large to be a key.",
        "El archivo de la clave privada es demasiado grande para ser una clave."
    )
}
pub fn KEY_DECODE() -> &'static str {
    tr!(
        "Chave privada inválida ou passphrase incorreta.",
        "Invalid private key or incorrect passphrase.",
        "Clave privada no válida o passphrase incorrecta."
    )
}

/// Indica se a mensagem (já traduzida) significa conexão perdida ou encerrada.
pub fn is_connection_lost_message(m: &str) -> bool {
    // Em qualquer idioma: a mensagem pode ter sido criada antes de o usuário trocar o idioma.
    myssh_i18n::Lang::ALL.into_iter().any(|l| {
        myssh_i18n::with_lang(l, || {
            m.contains(CONNECTION_LOST()) || m.contains(CONN_CLOSED())
        })
    })
}

/// Prefixa o erro com o salto de origem (`hop` = índice do jump host; `None` = destino não leva prefixo).
pub(crate) fn at_hop(hop: Option<usize>, msg: String) -> String {
    match hop {
        Some(i) => format!("Jump host {}: {msg}", i + 1), // igual nos três idiomas
        None => msg,
    }
}

pub(crate) fn io_message(e: &io::Error) -> String {
    use io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused => tr!(
            "Conexão recusada pelo servidor.",
            "Connection refused by the server.",
            "Conexión rechazada por el servidor."
        )
        .into(),
        TimedOut => TIMEOUT().into(),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof => {
            CONNECTION_LOST().into()
        }
        HostUnreachable | NetworkUnreachable | NetworkDown => tr!(
            "Servidor inacessível pela rede.",
            "Server unreachable over the network.",
            "Servidor inaccesible por la red."
        )
        .into(),
        _ => {
            let m = e.to_string().to_ascii_lowercase();
            if m.contains("lookup") || m.contains("nodename") || m.contains("name or service") {
                tr!(
                    "Não foi possível resolver o endereço do host.",
                    "Could not resolve the host address.",
                    "No se pudo resolver la dirección del host."
                )
                .into()
            } else {
                tr!(
                    "Falha de rede ao conectar.",
                    "Network error while connecting.",
                    "Error de red al conectar."
                )
                .into()
            }
        }
    }
}

pub(crate) fn russh_message(e: &russh::Error) -> String {
    use russh::Error as E;
    match e {
        E::IO(io) => io_message(io),
        E::ConnectionTimeout | E::Elapsed(_) => TIMEOUT().into(),
        E::KeepaliveTimeout | E::InactivityTimeout | E::HUP | E::Disconnect => {
            CONNECTION_LOST().into()
        }
        E::NoCommonAlgo { .. } | E::UnknownAlgo | E::KexInit | E::Kex => {
            tr!("Sem algoritmos de criptografia em comum com o servidor (só são aceitos algoritmos modernos).","No encryption algorithms in common with the server (only modern algorithms are accepted).","Sin algoritmos de cifrado en común con el servidor (solo se aceptan algoritmos modernos).").into()
        }
        E::ChannelOpenFailure(_) => tr!("O servidor recusou abrir o canal da sessão.","The server refused to open the session channel.","El servidor rechazó abrir el canal de la sesión.").into(),
        E::RequestDenied => tr!("O servidor recusou a solicitação.","The server refused the request.","El servidor rechazó la solicitud.").into(),
        E::NoAuthMethod | E::UnsupportedAuthMethod => AUTH_REJECTED().into(),
        E::SendError | E::WrongChannel => CONNECTION_LOST().into(),
        _ => tr!("Erro de protocolo SSH.","SSH protocol error.","Error de protocolo SSH.").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_refused() {
        let e = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(io_message(&e), "Conexão recusada pelo servidor.");
    }

    #[test]
    fn mensagens_em_ingles_e_espanhol() {
        let e = io::Error::from(io::ErrorKind::ConnectionRefused);
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(io_message(&e), "Connection refused by the server.");
            assert_eq!(AUTH_REJECTED(), "Authentication rejected by the server.");
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(io_message(&e), "Conexión rechazada por el servidor.");
        });
    }

    #[test]
    fn fallback_nao_vaza_texto_bruto() {
        let e = io::Error::other("senha=hunter2 host=secreto.example");
        let m = io_message(&e);
        assert!(!m.contains("hunter2") && !m.contains("secreto"), "{m}");
    }
}
