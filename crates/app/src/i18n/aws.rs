//! Textos do transporte AWS SSM e do login IAM Identity Center.

use myssh_i18n::tr;

pub fn F_CONNECTION() -> &'static str {
    tr!("Conexão", "Connection", "Conexión")
}
pub fn CONN_DIRECT() -> &'static str {
    tr!("Direta (TCP)", "Direct (TCP)", "Directa (TCP)")
}
pub fn CONN_SSM() -> &'static str {
    "AWS SSM Session Manager"
}
pub fn F_INSTANCE() -> &'static str {
    tr!("ID da instância", "Instance ID", "ID de instancia")
}
pub fn F_AWS_REGION() -> &'static str {
    tr!("Região AWS", "AWS region", "Región de AWS")
}
pub fn AWS_REGION_FROM_PROFILE() -> &'static str {
    tr!(
        "a do perfil AWS",
        "from the AWS profile",
        "la del perfil de AWS"
    )
}
pub fn F_AWS_CREDS() -> &'static str {
    tr!("Credenciais AWS", "AWS credentials", "Credenciales de AWS")
}
pub fn AWS_CREDS_PROFILE() -> &'static str {
    tr!(
        "Perfil AWS (~/.aws)",
        "AWS profile (~/.aws)",
        "Perfil de AWS (~/.aws)"
    )
}
pub fn AWS_CREDS_SSO() -> &'static str {
    tr!(
        "IAM Identity Center (SSO)",
        "IAM Identity Center (SSO)",
        "IAM Identity Center (SSO)"
    )
}
pub fn F_AWS_PROFILE() -> &'static str {
    tr!("Perfil", "Profile", "Perfil")
}
pub fn AWS_PROFILE_DEFAULT() -> &'static str {
    tr!("(padrão)", "(default)", "(predeterminado)")
}
pub fn AWS_PROFILE_HELP() -> &'static str {
    tr!(
        "Usa as credenciais configuradas em ~/.aws (chaves, aws login, SSO do AWS CLI, credential_process). Vazio = cadeia padrão.",
        "Uses the credentials configured in ~/.aws (keys, aws login, AWS CLI SSO, credential_process). Empty = default chain.",
        "Usa las credenciales configuradas en ~/.aws (claves, aws login, SSO de AWS CLI, credential_process). Vacío = cadena predeterminada."
    )
}
pub fn F_SSO_START_URL() -> &'static str {
    tr!("URL de início", "Start URL", "URL de inicio")
}
pub fn F_SSO_REGION() -> &'static str {
    tr!("Região do SSO", "SSO region", "Región de SSO")
}
pub fn F_SSO_ACCOUNT() -> &'static str {
    tr!("ID da conta", "Account ID", "ID de cuenta")
}
pub fn F_SSO_ROLE() -> &'static str {
    tr!("Role", "Role", "Rol")
}
pub fn SSO_HELP() -> &'static str {
    tr!(
        "Ao conectar, o app pede login pelo navegador. Os tokens ficam só no cofre do sistema.",
        "When connecting, the app asks you to sign in through the browser. Tokens are kept only in the system vault.",
        "Al conectar, la app pide iniciar sesión en el navegador. Los tokens se guardan solo en la bóveda del sistema."
    )
}
pub fn SSM_HELP() -> &'static str {
    tr!(
        "O SSH passa por dentro do túnel SSM (sem porta aberta, sem aws CLI). A instância precisa do SSM Agent e do sshd ativo.",
        "SSH runs inside the SSM tunnel (no open port, no AWS CLI). The instance needs the SSM Agent and a running sshd.",
        "SSH pasa por dentro del túnel SSM (sin puerto abierto, sin AWS CLI). La instancia necesita el SSM Agent y sshd activo."
    )
}
pub fn SSO_LOGIN_TITLE() -> &'static str {
    tr!(
        "Entrar no AWS IAM Identity Center",
        "Sign in to AWS IAM Identity Center",
        "Iniciar sesión en AWS IAM Identity Center"
    )
}
pub fn SSO_LOGIN_STARTING() -> &'static str {
    tr!(
        "Pedindo um código à AWS…",
        "Requesting a code from AWS…",
        "Solicitando un código a AWS…"
    )
}
pub fn SSO_LOGIN_BODY() -> &'static str {
    tr!(
        "Abra a página de login, entre e confirme que o código abaixo é o mesmo mostrado lá:",
        "Open the sign-in page, sign in and confirm that the code below matches the one shown there:",
        "Abra la página de inicio de sesión, entre y confirme que el código de abajo coincide con el que se muestra allí:"
    )
}
pub fn SSO_LOGIN_WAITING() -> &'static str {
    tr!(
        "Aguardando a confirmação no navegador…",
        "Waiting for confirmation in the browser…",
        "Esperando la confirmación en el navegador…"
    )
}
pub fn SSO_OPEN_BROWSER() -> &'static str {
    tr!("Abrir navegador", "Open browser", "Abrir navegador")
}
pub fn SSO_COPY_LINK() -> &'static str {
    tr!("Copiar link", "Copy link", "Copiar enlace")
}
pub fn SSM_LABEL() -> &'static str {
    "AWS SSM"
}
