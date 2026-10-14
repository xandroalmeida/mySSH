//! Textos de interface (pt-BR), centralizados.

use crate::shortcuts::Action;

pub const TITLE: &str = "mySSH";
pub const STORE_FALLBACK: &str = "Não foi possível abrir o banco de dados local: perfis e configurações não serão salvos nesta execução.";

// Barra superior / geral
pub const NEW_PROFILE: &str = "Novo perfil";
pub const QUICK_CONNECT: &str = "Conexão rápida";
pub const SETTINGS: &str = "Configurações";
pub const CANCEL: &str = "Cancelar";
pub const SAVE: &str = "Salvar";
pub const SAVING: &str = "Salvando…";
pub const CLOSE: &str = "Fechar";
pub const CONNECT: &str = "Conectar";
pub const OPEN: &str = "Abrir";
pub const EDIT: &str = "Editar";
pub const DUPLICATE: &str = "Duplicar";
pub const DELETE: &str = "Excluir";
pub const FAVORITE: &str = "Favoritar";
pub const UNFAVORITE: &str = "Remover dos favoritos";
pub const ALL: &str = "Todos";
pub const ALL_F: &str = "Todas";
pub const BROWSE: &str = "Escolher…";
pub const PICK_KEY_TITLE: &str = "Escolher chave privada";

// Ambientes
pub const ENV_NONE: &str = "Sem ambiente";
pub const ENV_PROD: &str = "PRODUÇÃO";
pub const ENV_STAGING: &str = "HOMOLOGAÇÃO";
pub const ENV_DEV: &str = "DEV";

// Sidebar
pub const SEARCH_HINT: &str = "Buscar (nome, host, usuário, tag)";
pub const FILTER_FAVORITES: &str = "Somente favoritos";
pub const FILTER_CLEAR: &str = "Limpar filtros";
pub const FILTER_ENV: &str = "Ambiente:";
pub const FILTER_FOLDER: &str = "Pasta:";
pub const NO_FOLDER: &str = "Sem pasta";
pub const NO_PROFILES: &str = "Nenhum perfil ainda. Use “Novo perfil” ou “Conexão rápida”.";
pub const NO_MATCH: &str = "Nenhum perfil corresponde aos filtros.";

// Abas
pub const CLOSE_TAB: &str = "Fechar aba";
pub const NO_TABS: &str = "Nenhuma aba aberta. Selecione um perfil e use “Abrir”.";
pub const WELCOME_TITLE: &str = "Início";
pub const WELCOME_HEADING: &str = "Bem-vindo ao mySSH";
pub const WELCOME_BODY: &str = "Crie um perfil, ou use a conexão rápida, para abrir uma sessão.";
pub const TERMINAL_PLACEHOLDER: &str = "O terminal será ligado aqui.";
pub const SIMULATE_BUSY: &str = "Simular sessão ativa (protótipo)";
pub const QUICK_NOT_SAVED: &str = "Conexão rápida: este perfil não foi salvo.";
pub const AUTH_LABEL: &str = "Autenticação";
pub const JUMP_LABEL: &str = "Jump host";
pub const AUTH_SUMMARY_PASSWORD_SAVED: &str = "senha (salva no cofre)";
pub const AUTH_SUMMARY_PASSWORD_ASK: &str = "senha (perguntar ao conectar)";
pub const AUTH_SUMMARY_KEY: &str = "chave privada";
pub const AUTH_SUMMARY_AGENT: &str = "agente SSH";
pub const AUTH_SUMMARY_KI: &str = "keyboard-interactive";

pub const CLOSE_TAB_TITLE: &str = "Fechar aba com sessão ativa?";
pub const CLOSE_TAB_CONFIRM: &str = "Fechar aba";
pub const QUIT_TITLE: &str = "Sair do mySSH?";
pub const QUIT_BODY: &str = "Há abas com sessão ativa. Sair encerra todas elas.";
pub const QUIT_CONFIRM: &str = "Sair";

pub fn close_tab_body(title: &str) -> String {
    format!("A sessão em “{title}” está ativa. Fechar a aba encerra a sessão.")
}

pub fn shortcut_name(a: Action) -> &'static str {
    match a {
        Action::NewTab => "nova aba do perfil selecionado",
        Action::CloseTab => "fechar aba",
        Action::NewProfile => "novo perfil",
        Action::FocusSearch => "buscar perfis",
    }
}

// Diálogo de perfil
pub const NEW_PROFILE_TITLE: &str = "Novo perfil";
pub const EDIT_PROFILE_TITLE: &str = "Editar perfil";
pub const F_NAME: &str = "Nome";
pub const F_HOST: &str = "Host";
pub const F_PORT: &str = "Porta";
pub const F_USER: &str = "Usuário";
pub const F_AUTH: &str = "Autenticação";
pub const AUTH_PASSWORD: &str = "Senha";
pub const AUTH_KEY: &str = "Chave";
pub const AUTH_AGENT: &str = "Agente";
pub const AUTH_KI: &str = "Keyboard-interactive";
pub const PW_ASK: &str = "Perguntar sempre (não salvar)";
pub const PW_SAVE: &str = "Salvar no cofre do sistema";
pub const F_PASSWORD: &str = "Senha";
pub const KEEP_SAVED: &str = "em branco = manter a senha salva";
pub const F_KEY_PATH: &str = "Arquivo da chave";
pub const F_PASSPHRASE: &str = "Passphrase";
pub const PP_ASK: &str = "Perguntar quando necessário";
pub const PP_SAVE: &str = "Salvar no cofre do sistema";
pub const AGENT_HELP: &str = "Usa as chaves do agente SSH local (SSH_AUTH_SOCK / Pageant).";
pub const KI_HELP: &str = "Respostas (MFA/OTP) são pedidas na conexão e nunca salvas.";
pub const F_FOLDER: &str = "Pasta";
pub const OR_NEW_FOLDER: &str = "ou nova:";
pub const F_TAGS: &str = "Tags";
pub const TAGS_HINT: &str = "separadas por vírgula";
pub const F_ENV: &str = "Ambiente";
pub const F_FAVORITE: &str = "Favorito";
pub const F_JUMP: &str = "Jump host";
pub const NO_JUMP: &str = "(nenhum)";
pub const F_KEEPALIVE: &str = "Keepalive (s)";
pub const KEEPALIVE_OFF: &str = "vazio = desligado";
pub const F_TIMEOUT: &str = "Timeout (s)";

pub const ERR_PORT: &str = "Porta inválida (1 a 65535).";
pub const ERR_KEEPALIVE: &str = "Keepalive inválido: use um número de segundos ou deixe vazio.";
pub const ERR_TIMEOUT: &str = "Timeout inválido: use um número de segundos maior que zero.";
pub const ERR_KEY_PATH: &str = "Informe o arquivo da chave privada.";
pub const ERR_SECRET_REQUIRED: &str =
    "Digite o segredo para salvá-lo no cofre, ou escolha “perguntar”.";

pub fn cred_label_password(profile: &str) -> String {
    format!("Senha de {profile}")
}
pub fn cred_label_passphrase(profile: &str) -> String {
    format!("Passphrase de {profile}")
}
pub fn profile_saved(name: &str) -> String {
    format!("Perfil “{name}” salvo.")
}
pub fn profile_duplicated(name: &str) -> String {
    format!("Perfil duplicado como “{name}”.")
}
pub fn profile_deleted(name: &str) -> String {
    format!("Perfil “{name}” excluído.")
}

pub const COPY_SUFFIX: &str = "(cópia)";
pub const DELETE_TITLE: &str = "Excluir perfil?";
pub fn delete_body(name: &str) -> String {
    format!(
        "O perfil “{name}” será excluído. A senha/passphrase salva é removida do cofre se nenhum outro perfil a usa."
    )
}

// Cofre
pub const KEYRING_UNAVAILABLE: &str =
    "Cofre do sistema indisponível: não é possível salvar senhas com segurança.";
pub const KEYRING_LOCKED: &str = "cofre do sistema bloqueado ou acesso negado";
pub const USE_SESSION_ONLY: &str = "Usar só nesta sessão";
pub const SESSION_ONLY_HELP: &str = "Você pode guardar o segredo só em memória: ele some ao fechar o app e nunca é gravado em disco.";
pub const SESSION_VAULT_ON: &str = "Cofre em memória ativado: segredos valem apenas nesta sessão.";
pub const VAULT_LOCKED_BADGE: &str = "🔒 Cofre bloqueado";
pub const VAULT_SESSION_BADGE: &str = "Cofre: só nesta sessão";
pub const UNLOCK_VAULT: &str = "Desbloquear cofre";
pub const VAULT_UNLOCKED: &str = "Cofre desbloqueado.";
pub const VAULT_LOCKED_NOTICE: &str = "Cofre bloqueado.";
pub const ERR_VAULT_UNAVAILABLE: &str = "O cofre do sistema está indisponível.";
pub const ERR_VAULT_LOCKED: &str = "O cofre está bloqueado. Desbloqueie para salvar o segredo.";
pub const ERR_SECRET_NOT_FOUND: &str = "Segredo não encontrado no cofre.";
pub const ERR_SECRET_TOO_LONG: &str = "Segredo grande demais para o cofre do sistema.";
pub const ERR_VAULT_BACKEND: &str = "Falha no cofre do sistema.";

// Conexão rápida
pub const QUICK_TITLE: &str = "Conexão rápida";
pub const QUICK_HELP: &str = "Formato: usuário@host[:porta]";
pub const QUICK_HINT: &str = "ex.: deploy@servidor.exemplo.com:2222";
pub const QUICK_NOTE: &str = "O perfil não é salvo. A senha, se houver, é pedida ao conectar.";
pub const QUICK_ERR_EMPTY: &str = "Digite usuário@host[:porta].";
pub const QUICK_ERR_USER: &str = "Informe o usuário antes do @ (usuário@host).";
pub const QUICK_ERR_HOST: &str = "Host inválido.";

// Configurações
pub const SETTINGS_TITLE: &str = "Configurações";
pub const S_THEME: &str = "Tema";
pub const THEME_SYSTEM: &str = "Sistema";
pub const THEME_LIGHT: &str = "Claro";
pub const THEME_DARK: &str = "Escuro";
pub const S_FONT: &str = "Fonte do terminal";
pub const FONT_SAMPLE: &str = "user@host:~$ ls -la  áéíõç";
pub const S_PASTE: &str = "Colagem";
pub const S_PASTE_HELP: &str = "Confirmar colagem com várias linhas";
pub const S_PROD: &str = "Produção";
pub const S_PROD_HELP: &str = "Mostrar faixa e borda vermelhas em abas de produção";
pub const S_VAULT_TIMEOUT: &str = "Bloquear cofre após";
pub const S_VAULT_TIMEOUT_HELP: &str = "de inatividade (0 = nunca)";
pub const S_VAULT: &str = "Cofre de segredos";
pub const S_VAULT_SESSION_ONLY: &str = "Cofre em memória: senhas salvas valem só até fechar o app.";
pub const LOCK_NOW: &str = "Bloquear cofre agora";
pub const LOCK_NOW_HELP: &str = "Bloquear o cofre não encerra sessões abertas: ele só impede ler senhas e passphrases salvas em novas conexões até ser desbloqueado.";
