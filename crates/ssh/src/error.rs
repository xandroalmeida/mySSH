//! Mensagens de erro em pt-BR, sanitizadas: nunca incluem senha, passphrase, hostname ou
//! texto bruto de bibliotecas que possa carregar dados sensíveis.

use std::io;

pub(crate) const TIMEOUT: &str = "Tempo esgotado ao conectar.";
pub(crate) const CONNECTION_LOST: &str = "A conexão com o servidor foi perdida.";
pub const AUTH_REJECTED: &str = "Autenticação recusada pelo servidor.";
pub(crate) const HOST_REJECTED: &str = "Conexão cancelada: host não confiável.";
pub(crate) const HOST_CHANGED: &str =
    "A chave do servidor mudou ou não é a confiável: conexão bloqueada (possível ataque).";
pub(crate) const HOST_PROMPT_TIMEOUT: &str =
    "Sem resposta sobre a chave do servidor: conexão cancelada.";
pub(crate) const HOST_STORE: &str = "Falha ao consultar ou salvar os hosts confiáveis.";
pub(crate) const HOST_CERT: &str = "Certificados de host ainda não são suportados.";
pub(crate) const UI_GONE: &str = "Interface encerrada.";
pub(crate) const UNSUPPORTED_AUTH: &str = "Este método de autenticação ainda não é suportado.";
pub(crate) const NO_AGENT: &str =
    "Agente SSH indisponível (SSH_AUTH_SOCK não definido ou inacessível).";
pub(crate) const AGENT_NO_KEYS: &str = "O agente SSH não tem chaves aceitas por este servidor.";
pub(crate) const KEY_READ: &str = "Não foi possível ler o arquivo da chave privada.";
pub const KEY_DECODE: &str = "Chave privada inválida ou passphrase incorreta.";

pub(crate) fn io_message(e: &io::Error) -> String {
    use io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused => "Conexão recusada pelo servidor.".into(),
        TimedOut => TIMEOUT.into(),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof => CONNECTION_LOST.into(),
        HostUnreachable | NetworkUnreachable | NetworkDown => {
            "Servidor inacessível pela rede.".into()
        }
        _ => {
            let m = e.to_string().to_ascii_lowercase();
            if m.contains("lookup") || m.contains("nodename") || m.contains("name or service") {
                "Não foi possível resolver o endereço do host.".into()
            } else {
                "Falha de rede ao conectar.".into()
            }
        }
    }
}

pub(crate) fn russh_message(e: &russh::Error) -> String {
    use russh::Error as E;
    match e {
        E::IO(io) => io_message(io),
        E::ConnectionTimeout | E::Elapsed(_) => TIMEOUT.into(),
        E::KeepaliveTimeout | E::InactivityTimeout | E::HUP | E::Disconnect => {
            CONNECTION_LOST.into()
        }
        E::NoCommonAlgo { .. } | E::UnknownAlgo | E::KexInit | E::Kex => {
            "Sem algoritmos de criptografia em comum com o servidor (só são aceitos algoritmos modernos)."
                .into()
        }
        E::ChannelOpenFailure(_) => "O servidor recusou abrir o canal da sessão.".into(),
        E::RequestDenied => "O servidor recusou a solicitação.".into(),
        E::NoAuthMethod | E::UnsupportedAuthMethod => AUTH_REJECTED.into(),
        E::SendError | E::WrongChannel => CONNECTION_LOST.into(),
        _ => "Erro de protocolo SSH.".into(),
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
    fn fallback_nao_vaza_texto_bruto() {
        let e = io::Error::other("senha=hunter2 host=secreto.example");
        let m = io_message(&e);
        assert!(!m.contains("hunter2") && !m.contains("secreto"), "{m}");
    }
}
