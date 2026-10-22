//! Textos de interface, centralizados, em inglês, pt-BR e espanhol (ver `myssh_i18n`).
//!
//! Textos fixos são funções em MAIÚSCULAS (`i18n::SAVE()`), herdeiras das antigas constantes:
//! o valor depende do idioma ativo, escolhido com `tr!(pt_br, en, es)`.

#![allow(non_snake_case)]

use crate::shortcuts::Action;
use myssh_i18n::tr;

mod about;
mod aws;
mod files;
mod lang;
mod m2b;
mod sync;
pub use about::*;
pub use aws::*;
pub use files::*;
pub use lang::*;
pub use m2b::*;
pub use sync::*;

pub fn TITLE() -> &'static str {
    "mySSH"
}
pub fn STORE_FALLBACK() -> &'static str {
    tr!(
        "Não foi possível abrir o banco de dados local: perfis e configurações não serão salvos nesta execução.",
        "Could not open the local database: profiles and settings will not be saved in this run.",
        "No se pudo abrir la base de datos local: los perfiles y la configuración no se guardarán en esta ejecución."
    )
}

// Barra superior / geral
pub fn NEW_PROFILE() -> &'static str {
    tr!("Novo perfil", "New profile", "Nuevo perfil")
}
pub fn QUICK_CONNECT() -> &'static str {
    tr!("Conexão rápida", "Quick connect", "Conexión rápida")
}
pub fn SETTINGS() -> &'static str {
    tr!("Configurações", "Settings", "Configuración")
}
pub fn CANCEL() -> &'static str {
    tr!("Cancelar", "Cancel", "Cancelar")
}
pub fn SAVE() -> &'static str {
    tr!("Salvar", "Save", "Guardar")
}
pub fn SAVING() -> &'static str {
    tr!("Salvando…", "Saving…", "Guardando…")
}
pub fn CLOSE() -> &'static str {
    tr!("Fechar", "Close", "Cerrar")
}
pub fn CONNECT() -> &'static str {
    tr!("Conectar", "Connect", "Conectar")
}
pub fn OPEN() -> &'static str {
    tr!("Abrir", "Open", "Abrir")
}
pub fn EDIT() -> &'static str {
    tr!("Editar", "Edit", "Editar")
}
pub fn DUPLICATE() -> &'static str {
    tr!("Duplicar", "Duplicate", "Duplicar")
}
pub fn DELETE() -> &'static str {
    tr!("Excluir", "Delete", "Eliminar")
}
pub fn FAVORITE() -> &'static str {
    tr!("Favoritar", "Favorite", "Marcar como favorito")
}
pub fn UNFAVORITE() -> &'static str {
    tr!(
        "Remover dos favoritos",
        "Remove from favorites",
        "Quitar de favoritos"
    )
}
pub fn ALL() -> &'static str {
    tr!("Todos", "All", "Todos")
}
pub fn ALL_F() -> &'static str {
    tr!("Todas", "All", "Todas")
}
pub fn BROWSE() -> &'static str {
    tr!("Escolher…", "Browse…", "Elegir…")
}
pub fn PICK_KEY_TITLE() -> &'static str {
    tr!(
        "Escolher chave privada",
        "Choose private key",
        "Elegir clave privada"
    )
}

// Ambientes
pub fn ENV_NONE() -> &'static str {
    tr!("Sem ambiente", "No environment", "Sin entorno")
}
pub fn ENV_PROD() -> &'static str {
    tr!("PRODUÇÃO", "PRODUCTION", "PRODUCCIÓN")
}
pub fn ENV_STAGING() -> &'static str {
    tr!("HOMOLOGAÇÃO", "STAGING", "PREPRODUCCIÓN")
}
pub fn ENV_DEV() -> &'static str {
    "DEV"
}

// Sidebar
pub fn SEARCH_HINT() -> &'static str {
    tr!(
        "Buscar (nome, host, usuário, tag)",
        "Search (name, host, user, tag)",
        "Buscar (nombre, host, usuario, etiqueta)"
    )
}
pub fn FILTER_FAVORITES() -> &'static str {
    tr!("Somente favoritos", "Favorites only", "Solo favoritos")
}
pub fn FILTER_CLEAR() -> &'static str {
    tr!("Limpar filtros", "Clear filters", "Limpiar filtros")
}
pub fn FILTER_ENV() -> &'static str {
    tr!("Ambiente:", "Environment:", "Entorno:")
}
pub fn FILTER_FOLDER() -> &'static str {
    tr!("Pasta:", "Folder:", "Carpeta:")
}
pub fn NO_FOLDER() -> &'static str {
    tr!("Sem pasta", "No folder", "Sin carpeta")
}
pub fn NO_PROFILES() -> &'static str {
    tr!(
        "Nenhum perfil ainda. Use “Novo perfil” ou “Conexão rápida”.",
        "No profiles yet. Use “New profile” or “Quick connect”.",
        "Aún no hay perfiles. Usa “Nuevo perfil” o “Conexión rápida”."
    )
}
pub fn NO_MATCH() -> &'static str {
    tr!(
        "Nenhum perfil corresponde aos filtros.",
        "No profiles match the filters.",
        "Ningún perfil coincide con los filtros."
    )
}

// Abas
pub fn CLOSE_TAB() -> &'static str {
    tr!("Fechar aba", "Close tab", "Cerrar pestaña")
}
pub fn NO_TABS() -> &'static str {
    tr!(
        "Nenhuma aba aberta. Selecione um perfil e use “Abrir”.",
        "No tabs open. Select a profile and use “Open”.",
        "No hay pestañas abiertas. Selecciona un perfil y usa “Abrir”."
    )
}
pub fn WELCOME_TITLE() -> &'static str {
    tr!("Início", "Home", "Inicio")
}
pub fn WELCOME_HEADING() -> &'static str {
    tr!(
        "Bem-vindo ao mySSH",
        "Welcome to mySSH",
        "Bienvenido a mySSH"
    )
}
pub fn WELCOME_BODY() -> &'static str {
    tr!(
        "Crie um perfil, ou use a conexão rápida, para abrir uma sessão.",
        "Create a profile, or use quick connect, to open a session.",
        "Crea un perfil, o usa la conexión rápida, para abrir una sesión."
    )
}
pub fn QUICK_NOT_SAVED() -> &'static str {
    tr!(
        "Conexão rápida: este perfil não foi salvo.",
        "Quick connect: this profile was not saved.",
        "Conexión rápida: este perfil no se guardó."
    )
}
pub fn AUTH_LABEL() -> &'static str {
    tr!("Autenticação", "Authentication", "Autenticación")
}
pub fn AUTH_SUMMARY_PASSWORD_SAVED() -> &'static str {
    tr!(
        "senha (salva no cofre)",
        "password (saved in the vault)",
        "contraseña (guardada en la bóveda)"
    )
}
pub fn AUTH_SUMMARY_PASSWORD_ASK() -> &'static str {
    tr!(
        "senha (perguntar ao conectar)",
        "password (ask on connect)",
        "contraseña (preguntar al conectar)"
    )
}
pub fn AUTH_SUMMARY_KEY() -> &'static str {
    tr!("chave privada", "private key", "clave privada")
}
pub fn AUTH_SUMMARY_AGENT() -> &'static str {
    tr!("agente SSH", "SSH agent", "agente SSH")
}
pub fn AUTH_SUMMARY_KI() -> &'static str {
    "keyboard-interactive"
}

pub fn CLOSE_TAB_TITLE() -> &'static str {
    tr!(
        "Fechar aba com sessão ativa?",
        "Close tab with an active session?",
        "¿Cerrar la pestaña con una sesión activa?"
    )
}
pub fn CLOSE_TAB_CONFIRM() -> &'static str {
    tr!("Fechar aba", "Close tab", "Cerrar pestaña")
}
pub fn QUIT_TITLE() -> &'static str {
    tr!("Sair do mySSH?", "Quit mySSH?", "¿Salir de mySSH?")
}
pub fn QUIT_BODY() -> &'static str {
    tr!(
        "Há abas com sessão ativa. Sair encerra todas elas.",
        "There are tabs with an active session. Quitting ends all of them.",
        "Hay pestañas con una sesión activa. Salir las cierra todas."
    )
}
pub fn QUIT_CONFIRM() -> &'static str {
    tr!("Sair", "Quit", "Salir")
}

pub fn close_tab_body(title: &str) -> String {
    tr!(
        format!("A sessão em “{title}” está ativa. Fechar a aba encerra a sessão."),
        format!("The session in “{title}” is active. Closing the tab ends the session."),
        format!("La sesión en “{title}” está activa. Cerrar la pestaña termina la sesión.")
    )
}

pub fn shortcut_name(a: Action) -> &'static str {
    match a {
        Action::NewTab => tr!(
            "nova aba do perfil selecionado",
            "new tab for the selected profile",
            "nueva pestaña del perfil seleccionado"
        ),
        Action::CloseTab => tr!("fechar aba", "close tab", "cerrar pestaña"),
        Action::NewProfile => tr!("novo perfil", "new profile", "nuevo perfil"),
        Action::FocusSearch => tr!("buscar perfis", "search profiles", "buscar perfiles"),
        Action::SplitSide => tr!(
            "dividir painel lado a lado (mesmo perfil)",
            "split pane side by side (same profile)",
            "dividir panel lado a lado (mismo perfil)"
        ),
        Action::SplitStack => tr!(
            "dividir painel empilhado (mesmo perfil)",
            "split pane stacked (same profile)",
            "dividir panel apilado (mismo perfil)"
        ),
        Action::ClosePane => tr!("fechar painel", "close pane", "cerrar panel"),
        Action::NextPane => tr!("próximo painel", "next pane", "panel siguiente"),
        Action::ToggleFiles => tr!(
            "painel de arquivos (SFTP)",
            "files pane (SFTP)",
            "panel de archivos (SFTP)"
        ),
    }
}

// Diálogo de perfil
pub fn NEW_PROFILE_TITLE() -> &'static str {
    tr!("Novo perfil", "New profile", "Nuevo perfil")
}
pub fn EDIT_PROFILE_TITLE() -> &'static str {
    tr!("Editar perfil", "Edit profile", "Editar perfil")
}
pub fn F_NAME() -> &'static str {
    tr!("Nome", "Name", "Nombre")
}
pub fn F_HOST() -> &'static str {
    "Host"
}
pub fn F_PORT() -> &'static str {
    tr!("Porta", "Port", "Puerto")
}
pub fn F_USER() -> &'static str {
    tr!("Usuário", "User", "Usuario")
}
pub fn F_AUTH() -> &'static str {
    tr!("Autenticação", "Authentication", "Autenticación")
}
pub fn AUTH_PASSWORD() -> &'static str {
    tr!("Senha", "Password", "Contraseña")
}
pub fn AUTH_KEY() -> &'static str {
    tr!("Chave", "Key", "Clave")
}
pub fn AUTH_AGENT() -> &'static str {
    tr!("Agente", "Agent", "Agente")
}
pub fn AUTH_KI() -> &'static str {
    "Keyboard-interactive"
}
pub fn PW_ASK() -> &'static str {
    tr!(
        "Perguntar sempre (não salvar)",
        "Always ask (don't save)",
        "Preguntar siempre (no guardar)"
    )
}
pub fn PW_SAVE() -> &'static str {
    tr!(
        "Salvar no cofre do sistema",
        "Save in the system vault",
        "Guardar en la bóveda del sistema"
    )
}
pub fn F_PASSWORD() -> &'static str {
    tr!("Senha", "Password", "Contraseña")
}
pub fn KEEP_SAVED() -> &'static str {
    tr!(
        "em branco = manter a senha salva",
        "blank = keep the saved password",
        "en blanco = mantener la contraseña guardada"
    )
}
pub fn F_KEY_PATH() -> &'static str {
    tr!("Arquivo da chave", "Key file", "Archivo de la clave")
}
pub fn F_PASSPHRASE() -> &'static str {
    "Passphrase"
}
pub fn PP_ASK() -> &'static str {
    tr!(
        "Perguntar quando necessário",
        "Ask when needed",
        "Preguntar cuando sea necesario"
    )
}
pub fn PP_SAVE() -> &'static str {
    tr!(
        "Salvar no cofre do sistema",
        "Save in the system vault",
        "Guardar en la bóveda del sistema"
    )
}
pub fn AGENT_HELP() -> &'static str {
    tr!(
        "Usa as chaves do agente SSH local (SSH_AUTH_SOCK / Pageant).",
        "Uses the keys from the local SSH agent (SSH_AUTH_SOCK / Pageant).",
        "Usa las claves del agente SSH local (SSH_AUTH_SOCK / Pageant)."
    )
}
pub fn KI_HELP() -> &'static str {
    tr!(
        "Respostas (MFA/OTP) são pedidas na conexão e nunca salvas.",
        "Answers (MFA/OTP) are requested on connect and never saved.",
        "Las respuestas (MFA/OTP) se piden al conectar y nunca se guardan."
    )
}
pub fn F_FOLDER() -> &'static str {
    tr!("Pasta", "Folder", "Carpeta")
}
pub fn OR_NEW_FOLDER() -> &'static str {
    tr!("ou nova:", "or new:", "o nueva:")
}
pub fn F_TAGS() -> &'static str {
    tr!("Tags", "Tags", "Etiquetas")
}
pub fn TAGS_HINT() -> &'static str {
    tr!(
        "separadas por vírgula",
        "comma-separated",
        "separadas por comas"
    )
}
pub fn F_ENV() -> &'static str {
    tr!("Ambiente", "Environment", "Entorno")
}
pub fn F_FAVORITE() -> &'static str {
    tr!("Favorito", "Favorite", "Favorito")
}
pub fn F_JUMP() -> &'static str {
    "Jump host"
}
pub fn NO_JUMP() -> &'static str {
    tr!("(nenhum)", "(none)", "(ninguno)")
}
pub fn F_KEEPALIVE() -> &'static str {
    "Keepalive (s)"
}
pub fn KEEPALIVE_OFF() -> &'static str {
    tr!("vazio = desligado", "empty = off", "vacío = desactivado")
}
pub fn F_TIMEOUT() -> &'static str {
    "Timeout (s)"
}

pub fn ERR_PORT() -> &'static str {
    tr!(
        "Porta inválida (1 a 65535).",
        "Invalid port (1 to 65535).",
        "Puerto no válido (1 a 65535)."
    )
}
pub fn ERR_KEEPALIVE() -> &'static str {
    tr!(
        "Keepalive inválido: use um número de segundos ou deixe vazio.",
        "Invalid keepalive: use a number of seconds or leave it empty.",
        "Keepalive no válido: usa un número de segundos o déjalo vacío."
    )
}
pub fn ERR_TIMEOUT() -> &'static str {
    tr!(
        "Timeout inválido: use um número de segundos maior que zero.",
        "Invalid timeout: use a number of seconds greater than zero.",
        "Timeout no válido: usa un número de segundos mayor que cero."
    )
}
pub fn ERR_KEY_PATH() -> &'static str {
    tr!(
        "Informe o arquivo da chave privada.",
        "Enter the private key file.",
        "Indica el archivo de la clave privada."
    )
}
pub fn ERR_SECRET_REQUIRED() -> &'static str {
    tr!(
        "Digite o segredo para salvá-lo no cofre, ou escolha “perguntar”.",
        "Type the secret to save it in the vault, or choose “ask”.",
        "Escribe el secreto para guardarlo en la bóveda, o elige “preguntar”."
    )
}

pub fn profile_saved(name: &str) -> String {
    tr!(
        format!("Perfil “{name}” salvo."),
        format!("Profile “{name}” saved."),
        format!("Perfil “{name}” guardado.")
    )
}
pub fn profile_duplicated(name: &str) -> String {
    tr!(
        format!("Perfil duplicado como “{name}”."),
        format!("Profile duplicated as “{name}”."),
        format!("Perfil duplicado como “{name}”.")
    )
}
pub fn profile_deleted(name: &str) -> String {
    tr!(
        format!("Perfil “{name}” excluído."),
        format!("Profile “{name}” deleted."),
        format!("Perfil “{name}” eliminado.")
    )
}

pub fn DELETE_TITLE() -> &'static str {
    tr!("Excluir perfil?", "Delete profile?", "¿Eliminar perfil?")
}
pub fn delete_body(name: &str) -> String {
    tr!(
        format!(
            "O perfil “{name}” será excluído. A senha/passphrase salva é removida do cofre se nenhum outro perfil a usa."
        ),
        format!(
            "The profile “{name}” will be deleted. The saved password/passphrase is removed from the vault if no other profile uses it."
        ),
        format!(
            "El perfil “{name}” se eliminará. La contraseña/passphrase guardada se quita de la bóveda si ningún otro perfil la usa."
        )
    )
}

// Cofre
pub fn KEYRING_UNAVAILABLE() -> &'static str {
    tr!(
        "Cofre do sistema indisponível: não é possível salvar senhas com segurança.",
        "System vault unavailable: passwords cannot be saved securely.",
        "Bóveda del sistema no disponible: no se pueden guardar contraseñas de forma segura."
    )
}
pub fn KEYRING_LOCKED() -> &'static str {
    tr!(
        "cofre do sistema bloqueado ou acesso negado",
        "system vault locked or access denied",
        "bóveda del sistema bloqueada o acceso denegado"
    )
}
pub fn USE_SESSION_ONLY() -> &'static str {
    tr!(
        "Usar só nesta sessão",
        "Use for this session only",
        "Usar solo en esta sesión"
    )
}
pub fn SESSION_ONLY_HELP() -> &'static str {
    tr!(
        "Você pode guardar o segredo só em memória: ele some ao fechar o app e nunca é gravado em disco.",
        "You can keep the secret in memory only: it disappears when you close the app and is never written to disk.",
        "Puedes guardar el secreto solo en memoria: desaparece al cerrar la app y nunca se escribe en disco."
    )
}
pub fn SESSION_VAULT_ON() -> &'static str {
    tr!(
        "Cofre em memória ativado: segredos valem apenas nesta sessão.",
        "In-memory vault enabled: secrets are valid only for this session.",
        "Bóveda en memoria activada: los secretos solo valen en esta sesión."
    )
}
pub fn VAULT_LOCKED_BADGE() -> &'static str {
    tr!(
        "🔒 Cofre bloqueado",
        "🔒 Vault locked",
        "🔒 Bóveda bloqueada"
    )
}
pub fn VAULT_SESSION_BADGE() -> &'static str {
    tr!(
        "Cofre: só nesta sessão",
        "Vault: this session only",
        "Bóveda: solo esta sesión"
    )
}
pub fn UNLOCK_VAULT() -> &'static str {
    tr!("Desbloquear cofre", "Unlock vault", "Desbloquear bóveda")
}
pub fn VAULT_UNLOCKED() -> &'static str {
    tr!(
        "Cofre desbloqueado.",
        "Vault unlocked.",
        "Bóveda desbloqueada."
    )
}
pub fn VAULT_LOCKED_NOTICE() -> &'static str {
    tr!("Cofre bloqueado.", "Vault locked.", "Bóveda bloqueada.")
}
pub fn ERR_VAULT_UNAVAILABLE() -> &'static str {
    tr!(
        "O cofre do sistema está indisponível.",
        "The system vault is unavailable.",
        "La bóveda del sistema no está disponible."
    )
}
pub fn ERR_VAULT_LOCKED() -> &'static str {
    tr!(
        "O cofre está bloqueado. Desbloqueie para salvar o segredo.",
        "The vault is locked. Unlock it to save the secret.",
        "La bóveda está bloqueada. Desbloquéala para guardar el secreto."
    )
}
pub fn ERR_SECRET_NOT_FOUND() -> &'static str {
    tr!(
        "Segredo não encontrado no cofre.",
        "Secret not found in the vault.",
        "Secreto no encontrado en la bóveda."
    )
}
pub fn ERR_SECRET_TOO_LONG() -> &'static str {
    tr!(
        "Segredo grande demais para o cofre do sistema.",
        "Secret too large for the system vault.",
        "Secreto demasiado grande para la bóveda del sistema."
    )
}
pub fn ERR_VAULT_BACKEND() -> &'static str {
    tr!(
        "Falha no cofre do sistema.",
        "System vault failure.",
        "Fallo en la bóveda del sistema."
    )
}

// Conexão rápida
pub fn QUICK_TITLE() -> &'static str {
    tr!("Conexão rápida", "Quick connect", "Conexión rápida")
}
pub fn QUICK_HELP() -> &'static str {
    tr!(
        "Formato: usuário@host[:porta]",
        "Format: user@host[:port]",
        "Formato: usuario@host[:puerto]"
    )
}
pub fn QUICK_HINT() -> &'static str {
    tr!(
        "ex.: deploy@servidor.exemplo.com:2222",
        "e.g. deploy@server.example.com:2222",
        "p. ej.: deploy@servidor.ejemplo.com:2222"
    )
}
pub fn QUICK_NOTE() -> &'static str {
    tr!(
        "O perfil não é salvo. A senha, se houver, é pedida ao conectar.",
        "The profile is not saved. The password, if any, is requested on connect.",
        "El perfil no se guarda. La contraseña, si hay, se pide al conectar."
    )
}
pub fn QUICK_ERR_EMPTY() -> &'static str {
    tr!(
        "Digite usuário@host[:porta].",
        "Type user@host[:port].",
        "Escribe usuario@host[:puerto]."
    )
}
pub fn QUICK_ERR_USER() -> &'static str {
    tr!(
        "Informe o usuário antes do @ (usuário@host).",
        "Enter the user before the @ (user@host).",
        "Indica el usuario antes de la @ (usuario@host)."
    )
}
pub fn QUICK_ERR_HOST() -> &'static str {
    tr!("Host inválido.", "Invalid host.", "Host no válido.")
}

// Configurações
pub fn SETTINGS_TITLE() -> &'static str {
    tr!("Configurações", "Settings", "Configuración")
}
pub fn S_THEME() -> &'static str {
    tr!("Tema", "Theme", "Tema")
}
pub fn THEME_SYSTEM() -> &'static str {
    tr!("Sistema", "System", "Sistema")
}
pub fn THEME_LIGHT() -> &'static str {
    tr!("Claro", "Light", "Claro")
}
pub fn THEME_DARK() -> &'static str {
    tr!("Escuro", "Dark", "Oscuro")
}
pub fn S_FONT() -> &'static str {
    tr!("Fonte do terminal", "Terminal font", "Fuente del terminal")
}
pub fn FONT_SAMPLE() -> &'static str {
    "user@host:~$ ls -la  áéíõç"
}
pub fn S_PASTE() -> &'static str {
    tr!("Colagem", "Paste", "Pegado")
}
pub fn S_PASTE_HELP() -> &'static str {
    tr!(
        "Confirmar colagem com várias linhas",
        "Confirm pasting multiple lines",
        "Confirmar el pegado de varias líneas"
    )
}
pub fn S_PROD() -> &'static str {
    tr!("Produção", "Production", "Producción")
}
pub fn S_PROD_HELP() -> &'static str {
    tr!(
        "Mostrar faixa e borda vermelhas em abas de produção",
        "Show a red banner and border on production tabs",
        "Mostrar banner y borde rojos en las pestañas de producción"
    )
}
pub fn S_VAULT_TIMEOUT() -> &'static str {
    tr!(
        "Bloquear cofre após",
        "Lock vault after",
        "Bloquear la bóveda tras"
    )
}
pub fn S_VAULT_TIMEOUT_HELP() -> &'static str {
    tr!(
        "de inatividade (0 = nunca)",
        "of inactivity (0 = never)",
        "de inactividad (0 = nunca)"
    )
}
pub fn S_VAULT() -> &'static str {
    tr!("Cofre de segredos", "Secrets vault", "Bóveda de secretos")
}
pub fn S_VAULT_SESSION_ONLY() -> &'static str {
    tr!(
        "Cofre em memória: senhas salvas valem só até fechar o app.",
        "In-memory vault: saved passwords last only until the app is closed.",
        "Bóveda en memoria: las contraseñas guardadas duran solo hasta cerrar la app."
    )
}
pub fn LOCK_NOW() -> &'static str {
    tr!(
        "Bloquear cofre agora",
        "Lock vault now",
        "Bloquear bóveda ahora"
    )
}
pub fn LOCK_NOW_HELP() -> &'static str {
    tr!(
        "Bloquear o cofre não encerra sessões abertas: ele só impede ler senhas e passphrases salvas em novas conexões até ser desbloqueado.",
        "Locking the vault does not end open sessions: it only prevents reading saved passwords and passphrases on new connections until it is unlocked.",
        "Bloquear la bóveda no cierra las sesiones abiertas: solo impide leer contraseñas y passphrases guardadas en nuevas conexiones hasta que se desbloquee."
    )
}

// Aba de terminal: estados
pub fn ST_IDLE() -> &'static str {
    tr!("Não conectado", "Not connected", "No conectado")
}
pub fn ST_PREPARING() -> &'static str {
    tr!("Preparando", "Preparing", "Preparando")
}
pub fn ST_NEEDS_INPUT() -> &'static str {
    tr!(
        "Aguardando credenciais",
        "Waiting for credentials",
        "Esperando credenciales"
    )
}
pub fn ST_CONNECTING() -> &'static str {
    tr!("Conectando", "Connecting", "Conectando")
}
pub fn ST_AWAITING_TRUST() -> &'static str {
    tr!(
        "Aguardando confiança",
        "Waiting for trust",
        "Esperando confianza"
    )
}
pub fn ST_AUTHENTICATING() -> &'static str {
    tr!("Autenticando", "Authenticating", "Autenticando")
}
pub fn ST_CONNECTED() -> &'static str {
    tr!("Conectado", "Connected", "Conectado")
}
pub fn ST_DISCONNECTED() -> &'static str {
    tr!("Desconectado", "Disconnected", "Desconectado")
}
pub fn ST_ERROR() -> &'static str {
    tr!("Erro", "Error", "Error")
}
pub fn ST_BLOCKED() -> &'static str {
    tr!("Bloqueado", "Blocked", "Bloqueado")
}

// Aba de terminal: ações
pub fn TAB_CONNECT() -> &'static str {
    tr!("Conectar", "Connect", "Conectar")
}
pub fn TAB_DISCONNECT() -> &'static str {
    tr!("Desconectar", "Disconnect", "Desconectar")
}
pub fn TAB_RECONNECT() -> &'static str {
    tr!("Reconectar", "Reconnect", "Reconectar")
}
pub fn TAB_RETRY() -> &'static str {
    tr!("Tentar novamente", "Try again", "Reintentar")
}
pub fn TAB_PREPARING() -> &'static str {
    tr!(
        "Preparando a conexão…",
        "Preparing the connection…",
        "Preparando la conexión…"
    )
}
pub fn TAB_WAITING_INPUT() -> &'static str {
    tr!(
        "Aguardando você informar as credenciais…",
        "Waiting for you to enter the credentials…",
        "Esperando que introduzcas las credenciales…"
    )
}
pub fn TAB_NOT_CONNECTED() -> &'static str {
    tr!(
        "Esta aba não está conectada.",
        "This tab is not connected.",
        "Esta pestaña no está conectada."
    )
}
pub fn TAB_SESSION_ENDED() -> &'static str {
    tr!(
        "A sessão foi encerrada.",
        "The session has ended.",
        "La sesión ha terminado."
    )
}
pub fn TAB_CANCELLED() -> &'static str {
    tr!(
        "Conexão cancelada.",
        "Connection cancelled.",
        "Conexión cancelada."
    )
}
pub fn JUMP_LABEL() -> &'static str {
    "Jump host"
}

// Erros / avisos da aba
pub fn ERR_AUTH_PASSWORD() -> &'static str {
    tr!(
        "Autenticação recusada: senha incorreta ou usuário sem acesso a este servidor.",
        "Authentication rejected: wrong password or user without access to this server.",
        "Autenticación rechazada: contraseña incorrecta o usuario sin acceso a este servidor."
    )
}
pub fn ERR_AUTH_KEY() -> &'static str {
    tr!(
        "Autenticação recusada: o servidor não aceitou esta chave.",
        "Authentication rejected: the server did not accept this key.",
        "Autenticación rechazada: el servidor no aceptó esta clave."
    )
}
pub fn ERR_INTERNAL() -> &'static str {
    tr!(
        "Falha interna ao preparar a conexão.",
        "Internal failure while preparing the connection.",
        "Fallo interno al preparar la conexión."
    )
}
pub fn NOTE_CRED_MISSING() -> &'static str {
    tr!(
        "A credencial salva deste perfil não foi encontrada.",
        "This profile's saved credential was not found.",
        "No se encontró la credencial guardada de este perfil."
    )
}
pub fn NOTE_SECRET_MISSING() -> &'static str {
    tr!(
        "A senha salva não está mais no cofre.",
        "The saved password is no longer in the vault.",
        "La contraseña guardada ya no está en la bóveda."
    )
}
pub fn NOTE_VAULT_UNAVAILABLE() -> &'static str {
    tr!(
        "O cofre do sistema está indisponível.",
        "The system vault is unavailable.",
        "La bóveda del sistema no está disponible."
    )
}
pub fn SAVED_TO_VAULT() -> &'static str {
    tr!(
        "Segredo salvo no cofre.",
        "Secret saved in the vault.",
        "Secreto guardado en la bóveda."
    )
}
pub fn save_failed(why: &str) -> String {
    tr!(
        format!("Não foi possível salvar no cofre: {why}"),
        format!("Could not save to the vault: {why}"),
        format!("No se pudo guardar en la bóveda: {why}")
    )
}

// Pedido de senha / passphrase
pub fn ask_password_title(user: &str, host: &str) -> String {
    tr!(
        format!("Senha de {user}@{host}"),
        format!("Password for {user}@{host}"),
        format!("Contraseña de {user}@{host}")
    )
}
pub fn ask_passphrase_title(file: &str) -> String {
    tr!(
        format!("Passphrase da chave “{file}”"),
        format!("Passphrase for key “{file}”"),
        format!("Passphrase de la clave “{file}”")
    )
}
pub fn ASK_PASSWORD_HINT() -> &'static str {
    tr!("Senha", "Password", "Contraseña")
}
pub fn ASK_PASSPHRASE_HINT() -> &'static str {
    "Passphrase"
}
pub fn ASK_SAVE() -> &'static str {
    tr!(
        "Salvar no cofre do sistema",
        "Save in the system vault",
        "Guardar en la bóveda del sistema"
    )
}
pub fn ASK_SAVE_HELP() -> &'static str {
    tr!(
        "Não marque em computadores compartilhados.",
        "Do not check on shared computers.",
        "No lo marques en equipos compartidos."
    )
}

// Cofre bloqueado
pub fn VAULT_LOCKED_TITLE() -> &'static str {
    tr!("Cofre bloqueado", "Vault locked", "Bóveda bloqueada")
}
pub fn VAULT_LOCKED_BODY() -> &'static str {
    tr!(
        "A senha ou passphrase deste perfil está salva no cofre, que está bloqueado. Desbloqueie o cofre ou digite o segredo agora (sem salvar).",
        "This profile's password or passphrase is saved in the vault, which is locked. Unlock the vault or type the secret now (without saving).",
        "La contraseña o passphrase de este perfil está guardada en la bóveda, que está bloqueada. Desbloquea la bóveda o escribe el secreto ahora (sin guardar)."
    )
}
pub fn VAULT_UNLOCKING() -> &'static str {
    tr!("Desbloqueando…", "Unlocking…", "Desbloqueando…")
}
pub fn TYPE_NOW() -> &'static str {
    tr!("Digitar agora", "Type now", "Escribir ahora")
}

// Host key desconhecida
pub fn HK_TITLE() -> &'static str {
    tr!(
        "Servidor desconhecido",
        "Unknown server",
        "Servidor desconocido"
    )
}
pub fn HK_BODY() -> &'static str {
    tr!(
        "É a primeira vez que você se conecta a este servidor. Confirme a impressão digital abaixo por um canal confiável (com o administrador, no console do provedor ou em documentação oficial) antes de aceitar. Se não for possível confirmar, rejeite.",
        "This is the first time you connect to this server. Confirm the fingerprint below through a trusted channel (with the administrator, in the provider's console or in official documentation) before accepting. If you cannot confirm it, reject.",
        "Es la primera vez que te conectas a este servidor. Confirma la huella digital de abajo por un canal de confianza (con el administrador, en la consola del proveedor o en la documentación oficial) antes de aceptar. Si no puedes confirmarla, rechaza."
    )
}
pub fn HK_HOST() -> &'static str {
    tr!("Servidor", "Server", "Servidor")
}
pub fn HK_ALGO() -> &'static str {
    tr!("Algoritmo", "Algorithm", "Algoritmo")
}
pub fn HK_FINGERPRINT() -> &'static str {
    tr!(
        "Impressão digital (SHA-256)",
        "Fingerprint (SHA-256)",
        "Huella digital (SHA-256)"
    )
}
pub fn HK_REJECT() -> &'static str {
    tr!("Rejeitar", "Reject", "Rechazar")
}
pub fn HK_ACCEPT_ONCE() -> &'static str {
    tr!("Aceitar uma vez", "Accept once", "Aceptar una vez")
}
pub fn HK_ACCEPT_SAVE() -> &'static str {
    tr!("Aceitar e salvar", "Accept and save", "Aceptar y guardar")
}

// Host key alterada
pub fn HKC_TITLE() -> &'static str {
    tr!(
        "ALERTA: a chave do servidor mudou",
        "WARNING: the server's key has changed",
        "ALERTA: la clave del servidor ha cambiado"
    )
}
pub fn HKC_BODY() -> &'static str {
    tr!(
        "A chave apresentada agora é diferente da que você confiou antes. Isso pode indicar um ataque de interceptação (MITM), em que alguém se passa pelo servidor para roubar senhas e dados. Por segurança a conexão foi bloqueada e nada foi enviado. Se o servidor foi reinstalado ou trocou de chave de forma legítima, confirme com o administrador por um canal confiável antes de esquecer a chave antiga.",
        "The key now presented is different from the one you trusted before. This may indicate an interception attack (MITM), in which someone impersonates the server to steal passwords and data. For safety the connection was blocked and nothing was sent. If the server was reinstalled or legitimately changed its key, confirm with the administrator through a trusted channel before forgetting the old key.",
        "La clave que se presenta ahora es distinta de la que aceptaste antes. Esto puede indicar un ataque de interceptación (MITM), en el que alguien suplanta al servidor para robar contraseñas y datos. Por seguridad la conexión fue bloqueada y no se envió nada. Si el servidor fue reinstalado o cambió de clave de forma legítima, confírmalo con el administrador por un canal de confianza antes de olvidar la clave anterior."
    )
}
pub fn HKC_PRESENTED() -> &'static str {
    tr!(
        "Chave apresentada agora",
        "Key presented now",
        "Clave presentada ahora"
    )
}
pub fn HKC_KNOWN() -> &'static str {
    tr!(
        "Chave(s) confiável(is) conhecida(s)",
        "Known trusted key(s)",
        "Clave(s) de confianza conocida(s)"
    )
}
pub fn HKC_REVOKED() -> &'static str {
    tr!(
        "Esta chave foi revogada anteriormente por você.",
        "You previously revoked this key.",
        "Revocaste esta clave anteriormente."
    )
}
pub fn HKC_OPEN_TRUSTED() -> &'static str {
    tr!(
        "Abrir hosts confiáveis",
        "Open trusted hosts",
        "Abrir hosts de confianza"
    )
}
pub fn HKC_HINT() -> &'static str {
    tr!(
        "Depois de esquecer a chave antiga em Configurações, uma nova conexão pedirá confirmação como servidor desconhecido.",
        "After forgetting the old key in Settings, a new connection will ask for confirmation as an unknown server.",
        "Después de olvidar la clave anterior en Configuración, una nueva conexión pedirá confirmación como servidor desconocido."
    )
}

// Colagem
pub fn PASTE_TITLE() -> &'static str {
    tr!(
        "Colar várias linhas?",
        "Paste multiple lines?",
        "¿Pegar varias líneas?"
    )
}
pub fn PASTE_BODY() -> &'static str {
    tr!(
        "O texto abaixo tem várias linhas e cada uma pode ser executada como comando.",
        "The text below has multiple lines and each one may run as a command.",
        "El texto de abajo tiene varias líneas y cada una puede ejecutarse como comando."
    )
}
pub fn PASTE_CONFIRM() -> &'static str {
    tr!("Colar", "Paste", "Pegar")
}
pub fn paste_lines(n: usize) -> String {
    tr!(
        format!("{n} linhas"),
        format!("{n} lines"),
        format!("{n} líneas")
    )
}

// Hosts confiáveis (Configurações)
pub fn S_TRUSTED() -> &'static str {
    tr!("Hosts confiáveis", "Trusted hosts", "Hosts de confianza")
}
pub fn TRUSTED_EMPTY() -> &'static str {
    tr!(
        "Nenhum host confiável salvo.",
        "No trusted hosts saved.",
        "No hay hosts de confianza guardados."
    )
}
pub fn TRUSTED_FORGET() -> &'static str {
    tr!("Esquecer", "Forget", "Olvidar")
}
pub fn TRUSTED_HELP() -> &'static str {
    tr!(
        "Chaves aceitas com “Aceitar e salvar”. Esquecer remove a chave: a próxima conexão pedirá confirmação de novo.",
        "Keys accepted with “Accept and save”. Forgetting removes the key: the next connection will ask for confirmation again.",
        "Claves aceptadas con “Aceptar y guardar”. Olvidar elimina la clave: la próxima conexión pedirá confirmación de nuevo."
    )
}
pub fn FORGET_TITLE() -> &'static str {
    tr!(
        "Esquecer este host?",
        "Forget this host?",
        "¿Olvidar este host?"
    )
}
pub fn forget_body(host: &str, port: u16) -> String {
    tr!(
        format!(
            "A chave confiável de {host}:{port} será removida. Na próxima conexão você precisará confirmar a impressão digital novamente."
        ),
        format!(
            "The trusted key for {host}:{port} will be removed. On the next connection you will need to confirm the fingerprint again."
        ),
        format!(
            "La clave de confianza de {host}:{port} se eliminará. En la próxima conexión tendrás que confirmar la huella digital de nuevo."
        )
    )
}
pub fn FORGET_CONFIRM() -> &'static str {
    tr!("Esquecer", "Forget", "Olvidar")
}
pub fn TRUSTED_ERR() -> &'static str {
    tr!(
        "Não foi possível ler ou alterar os hosts confiáveis.",
        "Could not read or change the trusted hosts.",
        "No se pudieron leer ni modificar los hosts de confianza."
    )
}

// M2-A: jump host, keyboard-interactive/MFA, reconexão
pub fn ERR_JUMP_LOOP() -> &'static str {
    tr!(
        "A cadeia de jump hosts tem um ciclo: um perfil aparece duas vezes.",
        "The jump host chain has a cycle: a profile appears twice.",
        "La cadena de jump hosts tiene un ciclo: un perfil aparece dos veces."
    )
}
pub fn ERR_JUMP_MISSING() -> &'static str {
    tr!(
        "O jump host deste perfil não existe mais. Edite o perfil e escolha outro.",
        "This profile's jump host no longer exists. Edit the profile and choose another.",
        "El jump host de este perfil ya no existe. Edita el perfil y elige otro."
    )
}
pub fn KI_TITLE() -> &'static str {
    tr!(
        "Autenticação adicional",
        "Additional authentication",
        "Autenticación adicional"
    )
}
pub fn KI_DEFAULT_INSTRUCTIONS() -> &'static str {
    tr!(
        "O servidor pede informações adicionais para concluir o login.",
        "The server asks for additional information to complete the login.",
        "El servidor pide información adicional para completar el inicio de sesión."
    )
}
pub fn KI_SEND() -> &'static str {
    tr!("Enviar", "Send", "Enviar")
}
pub fn KI_ECHO_HINT() -> &'static str {
    tr!("Resposta", "Answer", "Respuesta")
}
pub fn RECONNECTED_SEPARATOR() -> &'static str {
    tr!("— reconectado —", "— reconnected —", "— reconectado —")
}
pub fn HK_VIA() -> &'static str {
    tr!("Caminho", "Path", "Ruta")
}
pub fn hk_via_jump(n: usize, total: usize, name: &str) -> String {
    tr!(
        format!("Salto {n} de {total}: {name}"),
        format!("Hop {n} of {total}: {name}"),
        format!("Salto {n} de {total}: {name}")
    )
}
pub fn hk_via_target(last_jump: &str) -> String {
    tr!(
        format!("Destino final, via bastion {last_jump}"),
        format!("Final destination, via bastion {last_jump}"),
        format!("Destino final, vía bastion {last_jump}")
    )
}
pub fn ask_password_title_jump(user: &str, host: &str, name: &str) -> String {
    tr!(
        format!("Senha de {user}@{host} (jump host {name})"),
        format!("Password for {user}@{host} (jump host {name})"),
        format!("Contraseña de {user}@{host} (jump host {name})")
    )
}
pub fn ki_title_jump(name: &str) -> String {
    tr!(
        format!("Autenticação adicional no jump host {name}"),
        format!("Additional authentication on jump host {name}"),
        format!("Autenticación adicional en el jump host {name}")
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use myssh_i18n::{Lang, with_lang};

    #[test]
    fn textos_em_ingles_e_espanhol() {
        assert_eq!(SAVE(), "Salvar");
        with_lang(Lang::En, || {
            assert_eq!(SAVE(), "Save");
            assert_eq!(profile_saved("x"), "Profile “x” saved.");
            assert_eq!(ABOUT_TITLE(), "About mySSH");
        });
        with_lang(Lang::Es, || {
            assert_eq!(SAVE(), "Guardar");
            assert_eq!(paste_lines(3), "3 líneas");
        });
    }
}
