//! Mensagens do crate (pt-BR, inglês, espanhol). Nunca carregam tokens nem credenciais.

#![allow(non_snake_case)]

use myssh_i18n::tr;

pub(crate) fn ssm_failed(detail: &str) -> String {
    tr!(
        format!("Falha no túnel AWS SSM: {detail}"),
        format!("AWS SSM tunnel failed: {detail}"),
        format!("Falló el túnel de AWS SSM: {detail}")
    )
}
pub(crate) fn aws_error(op: &str, detail: &str) -> String {
    tr!(
        format!("Erro da AWS em {op}: {detail}"),
        format!("AWS error in {op}: {detail}"),
        format!("Error de AWS en {op}: {detail}")
    )
}
pub fn NO_REGION() -> &'static str {
    tr!(
        "Região AWS não definida: informe a região no perfil ou configure-a no perfil AWS.",
        "AWS region not set: enter it in the profile or configure it in the AWS profile.",
        "Región de AWS no definida: indíquela en el perfil o configúrela en el perfil de AWS."
    )
}
pub fn SSO_LOGIN_REQUIRED() -> &'static str {
    tr!(
        "É preciso entrar no AWS IAM Identity Center.",
        "You need to sign in to AWS IAM Identity Center.",
        "Debe iniciar sesión en AWS IAM Identity Center."
    )
}
pub fn SSO_LOGIN_EXPIRED() -> &'static str {
    tr!(
        "O código de login expirou antes da confirmação. Tente de novo.",
        "The sign-in code expired before it was confirmed. Try again.",
        "El código de inicio de sesión caducó antes de confirmarse. Inténtelo de nuevo."
    )
}
pub fn SSO_LOGIN_DENIED() -> &'static str {
    tr!(
        "Login recusado no navegador.",
        "Sign-in was denied in the browser.",
        "Inicio de sesión rechazado en el navegador."
    )
}
pub fn SSO_BAD_URL() -> &'static str {
    tr!(
        "A AWS devolveu um endereço de verificação inesperado.",
        "AWS returned an unexpected verification address.",
        "AWS devolvió una dirección de verificación inesperada."
    )
}
pub(crate) fn HANDSHAKE_TIMEOUT() -> &'static str {
    tr!(
        "o agente SSM não respondeu ao handshake",
        "the SSM agent did not answer the handshake",
        "el agente SSM no respondió al handshake"
    )
}
pub(crate) fn CLOSED_BEFORE_HANDSHAKE() -> &'static str {
    tr!(
        "canal encerrado antes do handshake",
        "channel closed before the handshake",
        "canal cerrado antes del handshake"
    )
}
pub(crate) fn PORT_ERROR() -> &'static str {
    tr!(
        "o agente SSM não conseguiu conectar na porta SSH da instância",
        "the SSM agent could not connect to the instance's SSH port",
        "el agente SSM no pudo conectarse al puerto SSH de la instancia"
    )
}
pub(crate) fn unsupported_action(action: &str) -> String {
    tr!(
        format!("a sessão exige '{action}' (ex.: criptografia KMS), ainda não suportado"),
        format!("the session requires '{action}' (e.g. KMS encryption), not supported yet"),
        format!("la sesión exige '{action}' (p. ej., cifrado KMS), aún no compatible")
    )
}
pub(crate) fn channel_closed(detail: &str) -> String {
    tr!(
        format!("canal fechado pelo SSM: {detail}"),
        format!("channel closed by SSM: {detail}"),
        format!("canal cerrado por SSM: {detail}")
    )
}
pub(crate) fn agent_error(detail: &str) -> String {
    tr!(
        format!("erro do agente SSM: {detail}"),
        format!("SSM agent error: {detail}"),
        format!("error del agente SSM: {detail}")
    )
}
pub(crate) fn WEBSOCKET() -> &'static str {
    tr!(
        "falha na conexão com o serviço ssmmessages",
        "failed to connect to the ssmmessages service",
        "falló la conexión con el servicio ssmmessages"
    )
}
pub(crate) fn PROTOCOL() -> &'static str {
    tr!(
        "mensagem inválida do serviço SSM",
        "invalid message from the SSM service",
        "mensaje no válido del servicio SSM"
    )
}
