//! Textos de interface, centralizados, em inglês, pt-BR e espanhol (ver `myssh_i18n`).
//!
//! Textos fixos são funções em MAIÚSCULAS (`i18n::SAVE()`), herdeiras das antigas constantes:
//! o valor depende do idioma ativo, escolhido com `tr!(pt_br, en, es)`.

#![allow(non_snake_case)]

use crate::shortcuts::Action;

mod about;
mod files;
mod m2b;
mod sync;
pub use about::*;
pub use files::*;
pub use m2b::*;
pub use sync::*;

pub fn TITLE() -> &'static str {
    "mySSH"
}
pub fn STORE_FALLBACK() -> &'static str {
    "Não foi possível abrir o banco de dados local: perfis e configurações não serão salvos nesta execução."
}

// Barra superior / geral
pub fn NEW_PROFILE() -> &'static str {
    "Novo perfil"
}
pub fn QUICK_CONNECT() -> &'static str {
    "Conexão rápida"
}
pub fn SETTINGS() -> &'static str {
    "Configurações"
}
pub fn CANCEL() -> &'static str {
    "Cancelar"
}
pub fn SAVE() -> &'static str {
    "Salvar"
}
pub fn SAVING() -> &'static str {
    "Salvando…"
}
pub fn CLOSE() -> &'static str {
    "Fechar"
}
pub fn CONNECT() -> &'static str {
    "Conectar"
}
pub fn OPEN() -> &'static str {
    "Abrir"
}
pub fn EDIT() -> &'static str {
    "Editar"
}
pub fn DUPLICATE() -> &'static str {
    "Duplicar"
}
pub fn DELETE() -> &'static str {
    "Excluir"
}
pub fn FAVORITE() -> &'static str {
    "Favoritar"
}
pub fn UNFAVORITE() -> &'static str {
    "Remover dos favoritos"
}
pub fn ALL() -> &'static str {
    "Todos"
}
pub fn ALL_F() -> &'static str {
    "Todas"
}
pub fn BROWSE() -> &'static str {
    "Escolher…"
}
pub fn PICK_KEY_TITLE() -> &'static str {
    "Escolher chave privada"
}

// Ambientes
pub fn ENV_NONE() -> &'static str {
    "Sem ambiente"
}
pub fn ENV_PROD() -> &'static str {
    "PRODUÇÃO"
}
pub fn ENV_STAGING() -> &'static str {
    "HOMOLOGAÇÃO"
}
pub fn ENV_DEV() -> &'static str {
    "DEV"
}

// Sidebar
pub fn SEARCH_HINT() -> &'static str {
    "Buscar (nome, host, usuário, tag)"
}
pub fn FILTER_FAVORITES() -> &'static str {
    "Somente favoritos"
}
pub fn FILTER_CLEAR() -> &'static str {
    "Limpar filtros"
}
pub fn FILTER_ENV() -> &'static str {
    "Ambiente:"
}
pub fn FILTER_FOLDER() -> &'static str {
    "Pasta:"
}
pub fn NO_FOLDER() -> &'static str {
    "Sem pasta"
}
pub fn NO_PROFILES() -> &'static str {
    "Nenhum perfil ainda. Use “Novo perfil” ou “Conexão rápida”."
}
pub fn NO_MATCH() -> &'static str {
    "Nenhum perfil corresponde aos filtros."
}

// Abas
pub fn CLOSE_TAB() -> &'static str {
    "Fechar aba"
}
pub fn NO_TABS() -> &'static str {
    "Nenhuma aba aberta. Selecione um perfil e use “Abrir”."
}
pub fn WELCOME_TITLE() -> &'static str {
    "Início"
}
pub fn WELCOME_HEADING() -> &'static str {
    "Bem-vindo ao mySSH"
}
pub fn WELCOME_BODY() -> &'static str {
    "Crie um perfil, ou use a conexão rápida, para abrir uma sessão."
}
pub fn QUICK_NOT_SAVED() -> &'static str {
    "Conexão rápida: este perfil não foi salvo."
}
pub fn AUTH_LABEL() -> &'static str {
    "Autenticação"
}
pub fn AUTH_SUMMARY_PASSWORD_SAVED() -> &'static str {
    "senha (salva no cofre)"
}
pub fn AUTH_SUMMARY_PASSWORD_ASK() -> &'static str {
    "senha (perguntar ao conectar)"
}
pub fn AUTH_SUMMARY_KEY() -> &'static str {
    "chave privada"
}
pub fn AUTH_SUMMARY_AGENT() -> &'static str {
    "agente SSH"
}
pub fn AUTH_SUMMARY_KI() -> &'static str {
    "keyboard-interactive"
}

pub fn CLOSE_TAB_TITLE() -> &'static str {
    "Fechar aba com sessão ativa?"
}
pub fn CLOSE_TAB_CONFIRM() -> &'static str {
    "Fechar aba"
}
pub fn QUIT_TITLE() -> &'static str {
    "Sair do mySSH?"
}
pub fn QUIT_BODY() -> &'static str {
    "Há abas com sessão ativa. Sair encerra todas elas."
}
pub fn QUIT_CONFIRM() -> &'static str {
    "Sair"
}

pub fn close_tab_body(title: &str) -> String {
    format!("A sessão em “{title}” está ativa. Fechar a aba encerra a sessão.")
}

pub fn shortcut_name(a: Action) -> &'static str {
    match a {
        Action::NewTab => "nova aba do perfil selecionado",
        Action::CloseTab => "fechar aba",
        Action::NewProfile => "novo perfil",
        Action::FocusSearch => "buscar perfis",
        Action::SplitSide => "dividir painel lado a lado (mesmo perfil)",
        Action::SplitStack => "dividir painel empilhado (mesmo perfil)",
        Action::ClosePane => "fechar painel",
        Action::NextPane => "próximo painel",
        Action::ToggleFiles => "painel de arquivos (SFTP)",
    }
}

// Diálogo de perfil
pub fn NEW_PROFILE_TITLE() -> &'static str {
    "Novo perfil"
}
pub fn EDIT_PROFILE_TITLE() -> &'static str {
    "Editar perfil"
}
pub fn F_NAME() -> &'static str {
    "Nome"
}
pub fn F_HOST() -> &'static str {
    "Host"
}
pub fn F_PORT() -> &'static str {
    "Porta"
}
pub fn F_USER() -> &'static str {
    "Usuário"
}
pub fn F_AUTH() -> &'static str {
    "Autenticação"
}
pub fn AUTH_PASSWORD() -> &'static str {
    "Senha"
}
pub fn AUTH_KEY() -> &'static str {
    "Chave"
}
pub fn AUTH_AGENT() -> &'static str {
    "Agente"
}
pub fn AUTH_KI() -> &'static str {
    "Keyboard-interactive"
}
pub fn PW_ASK() -> &'static str {
    "Perguntar sempre (não salvar)"
}
pub fn PW_SAVE() -> &'static str {
    "Salvar no cofre do sistema"
}
pub fn F_PASSWORD() -> &'static str {
    "Senha"
}
pub fn KEEP_SAVED() -> &'static str {
    "em branco = manter a senha salva"
}
pub fn F_KEY_PATH() -> &'static str {
    "Arquivo da chave"
}
pub fn F_PASSPHRASE() -> &'static str {
    "Passphrase"
}
pub fn PP_ASK() -> &'static str {
    "Perguntar quando necessário"
}
pub fn PP_SAVE() -> &'static str {
    "Salvar no cofre do sistema"
}
pub fn AGENT_HELP() -> &'static str {
    "Usa as chaves do agente SSH local (SSH_AUTH_SOCK / Pageant)."
}
pub fn KI_HELP() -> &'static str {
    "Respostas (MFA/OTP) são pedidas na conexão e nunca salvas."
}
pub fn F_FOLDER() -> &'static str {
    "Pasta"
}
pub fn OR_NEW_FOLDER() -> &'static str {
    "ou nova:"
}
pub fn F_TAGS() -> &'static str {
    "Tags"
}
pub fn TAGS_HINT() -> &'static str {
    "separadas por vírgula"
}
pub fn F_ENV() -> &'static str {
    "Ambiente"
}
pub fn F_FAVORITE() -> &'static str {
    "Favorito"
}
pub fn F_JUMP() -> &'static str {
    "Jump host"
}
pub fn NO_JUMP() -> &'static str {
    "(nenhum)"
}
pub fn F_KEEPALIVE() -> &'static str {
    "Keepalive (s)"
}
pub fn KEEPALIVE_OFF() -> &'static str {
    "vazio = desligado"
}
pub fn F_TIMEOUT() -> &'static str {
    "Timeout (s)"
}

pub fn ERR_PORT() -> &'static str {
    "Porta inválida (1 a 65535)."
}
pub fn ERR_KEEPALIVE() -> &'static str {
    "Keepalive inválido: use um número de segundos ou deixe vazio."
}
pub fn ERR_TIMEOUT() -> &'static str {
    "Timeout inválido: use um número de segundos maior que zero."
}
pub fn ERR_KEY_PATH() -> &'static str {
    "Informe o arquivo da chave privada."
}
pub fn ERR_SECRET_REQUIRED() -> &'static str {
    "Digite o segredo para salvá-lo no cofre, ou escolha “perguntar”."
}

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

pub fn COPY_SUFFIX() -> &'static str {
    "(cópia)"
}
pub fn DELETE_TITLE() -> &'static str {
    "Excluir perfil?"
}
pub fn delete_body(name: &str) -> String {
    format!(
        "O perfil “{name}” será excluído. A senha/passphrase salva é removida do cofre se nenhum outro perfil a usa."
    )
}

// Cofre
pub fn KEYRING_UNAVAILABLE() -> &'static str {
    "Cofre do sistema indisponível: não é possível salvar senhas com segurança."
}
pub fn KEYRING_LOCKED() -> &'static str {
    "cofre do sistema bloqueado ou acesso negado"
}
pub fn USE_SESSION_ONLY() -> &'static str {
    "Usar só nesta sessão"
}
pub fn SESSION_ONLY_HELP() -> &'static str {
    "Você pode guardar o segredo só em memória: ele some ao fechar o app e nunca é gravado em disco."
}
pub fn SESSION_VAULT_ON() -> &'static str {
    "Cofre em memória ativado: segredos valem apenas nesta sessão."
}
pub fn VAULT_LOCKED_BADGE() -> &'static str {
    "🔒 Cofre bloqueado"
}
pub fn VAULT_SESSION_BADGE() -> &'static str {
    "Cofre: só nesta sessão"
}
pub fn UNLOCK_VAULT() -> &'static str {
    "Desbloquear cofre"
}
pub fn VAULT_UNLOCKED() -> &'static str {
    "Cofre desbloqueado."
}
pub fn VAULT_LOCKED_NOTICE() -> &'static str {
    "Cofre bloqueado."
}
pub fn ERR_VAULT_UNAVAILABLE() -> &'static str {
    "O cofre do sistema está indisponível."
}
pub fn ERR_VAULT_LOCKED() -> &'static str {
    "O cofre está bloqueado. Desbloqueie para salvar o segredo."
}
pub fn ERR_SECRET_NOT_FOUND() -> &'static str {
    "Segredo não encontrado no cofre."
}
pub fn ERR_SECRET_TOO_LONG() -> &'static str {
    "Segredo grande demais para o cofre do sistema."
}
pub fn ERR_VAULT_BACKEND() -> &'static str {
    "Falha no cofre do sistema."
}

// Conexão rápida
pub fn QUICK_TITLE() -> &'static str {
    "Conexão rápida"
}
pub fn QUICK_HELP() -> &'static str {
    "Formato: usuário@host[:porta]"
}
pub fn QUICK_HINT() -> &'static str {
    "ex.: deploy@servidor.exemplo.com:2222"
}
pub fn QUICK_NOTE() -> &'static str {
    "O perfil não é salvo. A senha, se houver, é pedida ao conectar."
}
pub fn QUICK_ERR_EMPTY() -> &'static str {
    "Digite usuário@host[:porta]."
}
pub fn QUICK_ERR_USER() -> &'static str {
    "Informe o usuário antes do @ (usuário@host)."
}
pub fn QUICK_ERR_HOST() -> &'static str {
    "Host inválido."
}

// Configurações
pub fn SETTINGS_TITLE() -> &'static str {
    "Configurações"
}
pub fn S_THEME() -> &'static str {
    "Tema"
}
pub fn THEME_SYSTEM() -> &'static str {
    "Sistema"
}
pub fn THEME_LIGHT() -> &'static str {
    "Claro"
}
pub fn THEME_DARK() -> &'static str {
    "Escuro"
}
pub fn S_FONT() -> &'static str {
    "Fonte do terminal"
}
pub fn FONT_SAMPLE() -> &'static str {
    "user@host:~$ ls -la  áéíõç"
}
pub fn S_PASTE() -> &'static str {
    "Colagem"
}
pub fn S_PASTE_HELP() -> &'static str {
    "Confirmar colagem com várias linhas"
}
pub fn S_PROD() -> &'static str {
    "Produção"
}
pub fn S_PROD_HELP() -> &'static str {
    "Mostrar faixa e borda vermelhas em abas de produção"
}
pub fn S_VAULT_TIMEOUT() -> &'static str {
    "Bloquear cofre após"
}
pub fn S_VAULT_TIMEOUT_HELP() -> &'static str {
    "de inatividade (0 = nunca)"
}
pub fn S_VAULT() -> &'static str {
    "Cofre de segredos"
}
pub fn S_VAULT_SESSION_ONLY() -> &'static str {
    "Cofre em memória: senhas salvas valem só até fechar o app."
}
pub fn LOCK_NOW() -> &'static str {
    "Bloquear cofre agora"
}
pub fn LOCK_NOW_HELP() -> &'static str {
    "Bloquear o cofre não encerra sessões abertas: ele só impede ler senhas e passphrases salvas em novas conexões até ser desbloqueado."
}

// Aba de terminal: estados
pub fn ST_IDLE() -> &'static str {
    "Não conectado"
}
pub fn ST_PREPARING() -> &'static str {
    "Preparando"
}
pub fn ST_NEEDS_INPUT() -> &'static str {
    "Aguardando credenciais"
}
pub fn ST_CONNECTING() -> &'static str {
    "Conectando"
}
pub fn ST_AWAITING_TRUST() -> &'static str {
    "Aguardando confiança"
}
pub fn ST_AUTHENTICATING() -> &'static str {
    "Autenticando"
}
pub fn ST_CONNECTED() -> &'static str {
    "Conectado"
}
pub fn ST_DISCONNECTED() -> &'static str {
    "Desconectado"
}
pub fn ST_ERROR() -> &'static str {
    "Erro"
}
pub fn ST_BLOCKED() -> &'static str {
    "Bloqueado"
}

// Aba de terminal: ações
pub fn TAB_CONNECT() -> &'static str {
    "Conectar"
}
pub fn TAB_DISCONNECT() -> &'static str {
    "Desconectar"
}
pub fn TAB_RECONNECT() -> &'static str {
    "Reconectar"
}
pub fn TAB_RETRY() -> &'static str {
    "Tentar novamente"
}
pub fn TAB_PREPARING() -> &'static str {
    "Preparando a conexão…"
}
pub fn TAB_WAITING_INPUT() -> &'static str {
    "Aguardando você informar as credenciais…"
}
pub fn TAB_NOT_CONNECTED() -> &'static str {
    "Esta aba não está conectada."
}
pub fn TAB_SESSION_ENDED() -> &'static str {
    "A sessão foi encerrada."
}
pub fn TAB_CANCELLED() -> &'static str {
    "Conexão cancelada."
}
pub fn JUMP_LABEL() -> &'static str {
    "Jump host"
}

// Erros / avisos da aba
pub fn ERR_AUTH_PASSWORD() -> &'static str {
    "Autenticação recusada: senha incorreta ou usuário sem acesso a este servidor."
}
pub fn ERR_AUTH_KEY() -> &'static str {
    "Autenticação recusada: o servidor não aceitou esta chave."
}
pub fn ERR_INTERNAL() -> &'static str {
    "Falha interna ao preparar a conexão."
}
pub fn NOTE_CRED_MISSING() -> &'static str {
    "A credencial salva deste perfil não foi encontrada."
}
pub fn NOTE_SECRET_MISSING() -> &'static str {
    "A senha salva não está mais no cofre."
}
pub fn NOTE_VAULT_UNAVAILABLE() -> &'static str {
    "O cofre do sistema está indisponível."
}
pub fn SAVED_TO_VAULT() -> &'static str {
    "Segredo salvo no cofre."
}
pub fn save_failed(why: &str) -> String {
    format!("Não foi possível salvar no cofre: {why}")
}

// Pedido de senha / passphrase
pub fn ask_password_title(user: &str, host: &str) -> String {
    format!("Senha de {user}@{host}")
}
pub fn ask_passphrase_title(file: &str) -> String {
    format!("Passphrase da chave “{file}”")
}
pub fn ASK_PASSWORD_HINT() -> &'static str {
    "Senha"
}
pub fn ASK_PASSPHRASE_HINT() -> &'static str {
    "Passphrase"
}
pub fn ASK_SAVE() -> &'static str {
    "Salvar no cofre do sistema"
}
pub fn ASK_SAVE_HELP() -> &'static str {
    "Não marque em computadores compartilhados."
}

// Cofre bloqueado
pub fn VAULT_LOCKED_TITLE() -> &'static str {
    "Cofre bloqueado"
}
pub fn VAULT_LOCKED_BODY() -> &'static str {
    "A senha ou passphrase deste perfil está salva no cofre, que está bloqueado. Desbloqueie o cofre ou digite o segredo agora (sem salvar)."
}
pub fn VAULT_UNLOCKING() -> &'static str {
    "Desbloqueando…"
}
pub fn TYPE_NOW() -> &'static str {
    "Digitar agora"
}

// Host key desconhecida
pub fn HK_TITLE() -> &'static str {
    "Servidor desconhecido"
}
pub fn HK_BODY() -> &'static str {
    "É a primeira vez que você se conecta a este servidor. Confirme a impressão digital abaixo por um canal confiável (com o administrador, no console do provedor ou em documentação oficial) antes de aceitar. Se não for possível confirmar, rejeite."
}
pub fn HK_HOST() -> &'static str {
    "Servidor"
}
pub fn HK_ALGO() -> &'static str {
    "Algoritmo"
}
pub fn HK_FINGERPRINT() -> &'static str {
    "Impressão digital (SHA-256)"
}
pub fn HK_REJECT() -> &'static str {
    "Rejeitar"
}
pub fn HK_ACCEPT_ONCE() -> &'static str {
    "Aceitar uma vez"
}
pub fn HK_ACCEPT_SAVE() -> &'static str {
    "Aceitar e salvar"
}

// Host key alterada
pub fn HKC_TITLE() -> &'static str {
    "ALERTA: a chave do servidor mudou"
}
pub fn HKC_BODY() -> &'static str {
    "A chave apresentada agora é diferente da que você confiou antes. Isso pode indicar um ataque de interceptação (MITM), em que alguém se passa pelo servidor para roubar senhas e dados. Por segurança a conexão foi bloqueada e nada foi enviado. Se o servidor foi reinstalado ou trocou de chave de forma legítima, confirme com o administrador por um canal confiável antes de esquecer a chave antiga."
}
pub fn HKC_PRESENTED() -> &'static str {
    "Chave apresentada agora"
}
pub fn HKC_KNOWN() -> &'static str {
    "Chave(s) confiável(is) conhecida(s)"
}
pub fn HKC_REVOKED() -> &'static str {
    "Esta chave foi revogada anteriormente por você."
}
pub fn HKC_OPEN_TRUSTED() -> &'static str {
    "Abrir hosts confiáveis"
}
pub fn HKC_HINT() -> &'static str {
    "Depois de esquecer a chave antiga em Configurações, uma nova conexão pedirá confirmação como servidor desconhecido."
}

// Colagem
pub fn PASTE_TITLE() -> &'static str {
    "Colar várias linhas?"
}
pub fn PASTE_BODY() -> &'static str {
    "O texto abaixo tem várias linhas e cada uma pode ser executada como comando."
}
pub fn PASTE_CONFIRM() -> &'static str {
    "Colar"
}
pub fn paste_lines(n: usize) -> String {
    format!("{n} linhas")
}

// Hosts confiáveis (Configurações)
pub fn S_TRUSTED() -> &'static str {
    "Hosts confiáveis"
}
pub fn TRUSTED_EMPTY() -> &'static str {
    "Nenhum host confiável salvo."
}
pub fn TRUSTED_FORGET() -> &'static str {
    "Esquecer"
}
pub fn TRUSTED_HELP() -> &'static str {
    "Chaves aceitas com “Aceitar e salvar”. Esquecer remove a chave: a próxima conexão pedirá confirmação de novo."
}
pub fn FORGET_TITLE() -> &'static str {
    "Esquecer este host?"
}
pub fn forget_body(host: &str, port: u16) -> String {
    format!(
        "A chave confiável de {host}:{port} será removida. Na próxima conexão você precisará confirmar a impressão digital novamente."
    )
}
pub fn FORGET_CONFIRM() -> &'static str {
    "Esquecer"
}
pub fn TRUSTED_ERR() -> &'static str {
    "Não foi possível ler ou alterar os hosts confiáveis."
}

// M2-A: jump host, keyboard-interactive/MFA, reconexão
pub fn ERR_JUMP_LOOP() -> &'static str {
    "A cadeia de jump hosts tem um ciclo: um perfil aparece duas vezes."
}
pub fn ERR_JUMP_MISSING() -> &'static str {
    "O jump host deste perfil não existe mais. Edite o perfil e escolha outro."
}
pub fn KI_TITLE() -> &'static str {
    "Autenticação adicional"
}
pub fn KI_DEFAULT_INSTRUCTIONS() -> &'static str {
    "O servidor pede informações adicionais para concluir o login."
}
pub fn KI_SEND() -> &'static str {
    "Enviar"
}
pub fn KI_ECHO_HINT() -> &'static str {
    "Resposta"
}
pub fn RECONNECTED_SEPARATOR() -> &'static str {
    "— reconectado —"
}
pub fn HK_VIA() -> &'static str {
    "Caminho"
}
pub fn hk_via_jump(n: usize, total: usize, name: &str) -> String {
    format!("Salto {n} de {total}: {name}")
}
pub fn hk_via_target(last_jump: &str) -> String {
    format!("Destino final, via bastion {last_jump}")
}
pub fn ask_password_title_jump(user: &str, host: &str, name: &str) -> String {
    format!("Senha de {user}@{host} (jump host {name})")
}
pub fn ki_title_jump(name: &str) -> String {
    format!("Autenticação adicional no jump host {name}")
}
