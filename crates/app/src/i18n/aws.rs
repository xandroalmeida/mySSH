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
pub fn AUTH_EIC() -> &'static str {
    "EC2 Instance Connect"
}
pub fn AUTH_EIC_HELP() -> &'static str {
    tr!(
        "A cada conexão o app gera uma chave temporária em memória e a publica na instância por 60 s pela API da AWS. Nada é gravado no servidor. Requer ec2-instance-connect na instância (Ubuntu e Amazon Linux já trazem) e a permissão ec2-instance-connect:SendSSHPublicKey.",
        "On each connection the app generates a temporary in-memory key and publishes it to the instance for 60 s through the AWS API. Nothing is written on the server. Requires ec2-instance-connect on the instance (Ubuntu and Amazon Linux include it) and the ec2-instance-connect:SendSSHPublicKey permission.",
        "En cada conexión la app genera una clave temporal en memoria y la publica en la instancia por 60 s mediante la API de AWS. No se escribe nada en el servidor. Requiere ec2-instance-connect en la instancia (Ubuntu y Amazon Linux lo incluyen) y el permiso ec2-instance-connect:SendSSHPublicKey."
    )
}
pub fn AUTH_SUMMARY_EIC() -> &'static str {
    tr!(
        "EC2 Instance Connect (chave temporária)",
        "EC2 Instance Connect (temporary key)",
        "EC2 Instance Connect (clave temporal)"
    )
}

// ------------------------------------------------------------ ajuda (AWS SSM)

pub fn AWS_HELP_BUTTON() -> &'static str {
    tr!("Como configurar?", "How to set up?", "¿Cómo configurar?")
}
pub fn AWS_HELP_TITLE() -> &'static str {
    tr!(
        "Conexão via AWS SSM Session Manager",
        "Connecting through AWS SSM Session Manager",
        "Conexión mediante AWS SSM Session Manager"
    )
}
pub fn AWS_HELP_COPY_POLICY() -> &'static str {
    tr!("Copiar política", "Copy policy", "Copiar política")
}
pub fn AWS_HELP_COPIED() -> &'static str {
    tr!("Copiada.", "Copied.", "Copiada.")
}

/// Seções da ajuda: (título, corpo). Linhas começando com "• " são itens de lista.
pub fn aws_help_sections() -> [(&'static str, &'static str); 7] {
    [
        (
            tr!("O que é", "What it is", "Qué es"),
            tr!(
                "O SSH passa por dentro de um túnel do AWS Systems Manager. A instância não precisa de IP público nem de porta 22 aberta no security group, e você não precisa instalar o AWS CLI nem o session-manager-plugin. A host key do servidor continua sendo verificada, como em qualquer conexão SSH.",
                "SSH runs inside an AWS Systems Manager tunnel. The instance needs no public IP and no port 22 open in its security group, and you don't need to install the AWS CLI or the session-manager-plugin. The server's host key is still verified, as in any SSH connection.",
                "SSH pasa por dentro de un túnel de AWS Systems Manager. La instancia no necesita IP pública ni el puerto 22 abierto en el security group, y no hace falta instalar AWS CLI ni session-manager-plugin. La host key del servidor se sigue verificando, como en cualquier conexión SSH."
            ),
        ),
        (
            tr!("Na instância", "On the instance", "En la instancia"),
            tr!(
                "• SSM Agent rodando e registrado: a instância aparece como \"Online\" em Systems Manager > Fleet Manager. Ele já vem no Amazon Linux e no Ubuntu da AWS; a instância precisa de um perfil IAM com a política AmazonSSMManagedInstanceCore.\n• sshd ativo na porta do perfil (normalmente 22; basta escutar localmente).\n• Para EC2 Instance Connect: o pacote ec2-instance-connect (já vem no Amazon Linux 2023 e no Ubuntu).",
                "• SSM Agent running and registered: the instance shows as \"Online\" in Systems Manager > Fleet Manager. It ships with Amazon Linux and AWS's Ubuntu images; the instance needs an IAM instance profile with the AmazonSSMManagedInstanceCore policy.\n• sshd running on the profile's port (usually 22; listening locally is enough).\n• For EC2 Instance Connect: the ec2-instance-connect package (included in Amazon Linux 2023 and Ubuntu).",
                "• SSM Agent en ejecución y registrado: la instancia aparece como \"Online\" en Systems Manager > Fleet Manager. Viene en Amazon Linux y en las imágenes de Ubuntu de AWS; la instancia necesita un perfil IAM con la política AmazonSSMManagedInstanceCore.\n• sshd activo en el puerto del perfil (normalmente 22; basta con escuchar localmente).\n• Para EC2 Instance Connect: el paquete ec2-instance-connect (incluido en Amazon Linux 2023 y Ubuntu)."
            ),
        ),
        (
            tr!("Campos do perfil", "Profile fields", "Campos del perfil"),
            tr!(
                "• ID da instância: i-… (EC2 > Instâncias).\n• Usuário: o do sistema da instância (ubuntu no Ubuntu, ec2-user no Amazon Linux).\n• Região AWS: onde a instância está (ex.: us-east-1). Com perfil AWS, pode ficar vazia se o perfil já define a região.\n• Autenticação: recomendamos EC2 Instance Connect — a cada conexão o app gera uma chave temporária e a publica por 60 s, sem gravar nada no servidor. Chave, senha e agente também funcionam.",
                "• Instance ID: i-… (EC2 > Instances).\n• User: the instance's OS user (ubuntu on Ubuntu, ec2-user on Amazon Linux).\n• AWS region: where the instance is (e.g. us-east-1). With an AWS profile it can be empty if the profile already sets the region.\n• Authentication: we recommend EC2 Instance Connect — on each connection the app generates a temporary key and publishes it for 60 s, writing nothing on the server. Key, password and agent also work.",
                "• ID de instancia: i-… (EC2 > Instancias).\n• Usuario: el del sistema de la instancia (ubuntu en Ubuntu, ec2-user en Amazon Linux).\n• Región de AWS: donde está la instancia (p. ej., us-east-1). Con perfil de AWS puede quedar vacía si el perfil ya define la región.\n• Autenticación: recomendamos EC2 Instance Connect: en cada conexión la app genera una clave temporal y la publica por 60 s, sin escribir nada en el servidor. Clave, contraseña y agente también funcionan."
            ),
        ),
        (
            tr!("Credenciais AWS", "AWS credentials", "Credenciales de AWS"),
            tr!(
                "• Perfil AWS (~/.aws): usa o que já estiver configurado no computador, como o AWS CLI faria (chaves de acesso, aws login, SSO do CLI, credential_process). Em branco = perfil padrão / variáveis de ambiente.\n• IAM Identity Center (SSO): login pelo navegador, feito pelo próprio app.\n  – URL de início: a do portal de acesso AWS (ex.: https://d-1234567890.awsapps.com/start), em IAM Identity Center > Configurações.\n  – Região do SSO: onde o Identity Center foi configurado (pode ser diferente da região da instância).\n  – ID da conta: 12 dígitos da conta da instância.\n  – Role: o nome do permission set (ex.: AdministratorAccess).\n  Os tokens ficam só no cofre do sistema; quando vencem, o app pede o login de novo.",
                "• AWS profile (~/.aws): uses whatever is already configured on the computer, as the AWS CLI would (access keys, aws login, CLI SSO, credential_process). Blank = default profile / environment variables.\n• IAM Identity Center (SSO): browser sign-in, done by the app itself.\n  – Start URL: your AWS access portal (e.g. https://d-1234567890.awsapps.com/start), in IAM Identity Center > Settings.\n  – SSO region: where Identity Center was set up (may differ from the instance's region).\n  – Account ID: the 12 digits of the instance's account.\n  – Role: the permission set name (e.g. AdministratorAccess).\n  Tokens are kept only in the system vault; when they expire, the app asks you to sign in again.",
                "• Perfil de AWS (~/.aws): usa lo que ya esté configurado en el equipo, como haría AWS CLI (claves de acceso, aws login, SSO de CLI, credential_process). En blanco = perfil predeterminado / variables de entorno.\n• IAM Identity Center (SSO): inicio de sesión en el navegador, hecho por la propia app.\n  – URL de inicio: la del portal de acceso de AWS (p. ej., https://d-1234567890.awsapps.com/start), en IAM Identity Center > Configuración.\n  – Región de SSO: donde se configuró Identity Center (puede ser distinta de la región de la instancia).\n  – ID de cuenta: los 12 dígitos de la cuenta de la instancia.\n  – Rol: el nombre del permission set (p. ej., AdministratorAccess).\n  Los tokens se guardan solo en la bóveda del sistema; cuando caducan, la app pide iniciar sesión de nuevo."
            ),
        ),
        (
            tr!("Permissões IAM", "IAM permissions", "Permisos IAM"),
            tr!(
                "Quem conecta precisa de ssm:StartSession na instância e no documento AWS-StartSSHSession e, com EC2 Instance Connect, de ec2-instance-connect:SendSSHPublicKey. Exemplo de política (restrinja os recursos ao que fizer sentido):",
                "Whoever connects needs ssm:StartSession on the instance and on the AWS-StartSSHSession document and, with EC2 Instance Connect, ec2-instance-connect:SendSSHPublicKey. Example policy (narrow the resources as appropriate):",
                "Quien se conecta necesita ssm:StartSession en la instancia y en el documento AWS-StartSSHSession y, con EC2 Instance Connect, ec2-instance-connect:SendSSHPublicKey. Política de ejemplo (restrinja los recursos según convenga):"
            ),
        ),
        (
            tr!("Primeira conexão", "First connection", "Primera conexión"),
            tr!(
                "O app mostra a host key do servidor como em qualquer host novo (o endereço aparece como ID-da-instância:porta). Compare o fingerprint por um canal confiável, por exemplo o console da instância (EC2 > Ações > Monitorar > Obter log do sistema) ou ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub numa sessão já confiável.",
                "The app shows the server's host key as for any new host (the address appears as instance-ID:port). Compare the fingerprint through a trusted channel, e.g. the instance console (EC2 > Actions > Monitor > Get system log) or ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub in an already trusted session.",
                "La app muestra la host key del servidor como con cualquier host nuevo (la dirección aparece como ID-de-instancia:puerto). Compare el fingerprint por un canal de confianza, por ejemplo la consola de la instancia (EC2 > Acciones > Monitorear > Obtener registro del sistema) o ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub en una sesión ya confiable."
            ),
        ),
        (
            tr!("Problemas comuns", "Common problems", "Problemas comunes"),
            tr!(
                "• AccessDeniedException: faltam as permissões acima, ou a role/perfil está errado.\n• TargetNotConnected: o SSM Agent está offline, a instância está parada ou a região está errada.\n• \"não conseguiu conectar na porta SSH\": sshd parado ou porta errada no perfil.\n• \"exige KMSEncryption\": as preferências do Session Manager usam criptografia KMS, ainda não suportada.\n• Autenticação recusada com EC2 Instance Connect: usuário errado ou pacote ec2-instance-connect ausente.\n• \"Região AWS não definida\": preencha a região no perfil.",
                "• AccessDeniedException: the permissions above are missing, or the role/profile is wrong.\n• TargetNotConnected: the SSM Agent is offline, the instance is stopped or the region is wrong.\n• \"could not connect to the instance's SSH port\": sshd is stopped or the profile's port is wrong.\n• \"requires KMSEncryption\": the Session Manager preferences use KMS encryption, not supported yet.\n• Authentication rejected with EC2 Instance Connect: wrong user or the ec2-instance-connect package is missing.\n• \"AWS region not set\": fill in the region in the profile.",
                "• AccessDeniedException: faltan los permisos anteriores, o el rol/perfil es incorrecto.\n• TargetNotConnected: el SSM Agent está desconectado, la instancia está detenida o la región es incorrecta.\n• \"no pudo conectarse al puerto SSH\": sshd detenido o puerto incorrecto en el perfil.\n• \"exige KMSEncryption\": las preferencias de Session Manager usan cifrado KMS, aún no compatible.\n• Autenticación rechazada con EC2 Instance Connect: usuario incorrecto o falta el paquete ec2-instance-connect.\n• \"Región de AWS no definida\": complete la región en el perfil."
            ),
        ),
    ]
}

/// Política IAM de exemplo (JSON, igual em todos os idiomas).
pub const AWS_HELP_POLICY: &str = r#"{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "ssm:StartSession",
      "Resource": [
        "arn:aws:ec2:*:*:instance/*",
        "arn:aws:ssm:*::document/AWS-StartSSHSession"
      ]
    },
    {
      "Effect": "Allow",
      "Action": "ec2-instance-connect:SendSSHPublicKey",
      "Resource": "arn:aws:ec2:*:*:instance/*"
    }
  ]
}"#;
