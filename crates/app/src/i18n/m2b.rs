//! Textos do M2-B: temas por perfil, painéis divididos, importação do ssh config, cofre,
//! exportar/importar configuração.

// Tema por perfil
pub const TH_TITLE: &str = "Tema deste perfil";
pub const TH_PALETTE: &str = "Paleta do terminal";
pub const TH_INHERIT: &str = "Herdar do global";
pub const TH_FONT: &str = "Fonte do terminal";
pub const TH_FONT_OWN: &str = "Tamanho próprio";
pub const TH_ACCENT: &str = "Cor de destaque";
pub const TH_ACCENT_OWN: &str = "Usar destaque";
pub const TH_ACCENT_NONE: &str = "sem destaque (herda)";
pub const TH_RESET: &str = "Herdar tudo do global";
pub const TH_SCOPE_HELP: &str =
    "Vale só para as abas e painéis deste perfil, não para o restante do app.";
pub fn th_font_inherited(pt: f32) -> String {
    format!("herda do global ({pt:.1} pt)")
}
pub const TH_SYSTEM_HELP: &str = "“Sistema” acompanha o tema claro/escuro do sistema operacional.";

// Painéis divididos
pub const SPLIT: &str = "Dividir";
pub const SPLIT_SIDE: &str = "Lado a lado (divisor vertical)";
pub const SPLIT_STACK: &str = "Empilhado (divisor horizontal)";
pub const SPLIT_SAME: &str = "mesmo perfil";
pub const SPLIT_OTHER: &str = "outro perfil…";
pub const SPLIT_CLOSE_PANE: &str = "Fechar painel";
pub const SPLIT_PICK_TITLE_SIDE: &str = "Dividir lado a lado com…";
pub const SPLIT_PICK_TITLE_STACK: &str = "Dividir empilhado com…";
pub const SPLIT_PICK_HELP: &str = "Escolha o perfil da nova sessão. Ela abre em um painel novo desta aba; a sessão atual continua como está.";
pub const SPLIT_OPEN: &str = "Dividir";
pub const SPLIT_LIMIT: &str = "Limite de painéis por aba atingido.";
pub const SPLIT_ONLY_TERMINAL: &str = "Só abas de terminal podem ser divididas.";
pub const CLOSE_PANE_TITLE: &str = "Fechar painel com sessão ativa?";
pub const CLOSE_PANE_CONFIRM: &str = "Fechar painel";
pub fn close_pane_body(title: &str) -> String {
    format!("A sessão em “{title}” está ativa. Fechar o painel encerra a sessão.")
}

// Importação de ~/.ssh/config
pub const IMPORT_SSH: &str = "Importar ~/.ssh/config…";
pub const IMPORT_TITLE: &str = "Importar do ~/.ssh/config";
pub const IMPORT_INTRO: &str = "Lê o arquivo (somente leitura: o arquivo nunca é alterado) e mostra uma prévia antes de criar qualquer perfil. Diretivas que executam comandos locais são ignoradas.";
pub const IMPORT_USE_DEFAULT: &str = "Usar ~/.ssh/config";
pub const IMPORT_PICK_FILE: &str = "Escolher outro arquivo…";
pub const IMPORT_PICK_TITLE: &str = "Escolher arquivo ssh config";
pub const IMPORT_LOADING: &str = "Lendo o arquivo…";
pub const IMPORT_SAVING: &str = "Importando…";
pub const IMPORT_NONE: &str = "Nenhum host importável foi encontrado neste arquivo.";
pub const IMPORT_GLOBAL_WARNINGS: &str = "Avisos gerais";
pub const IMPORT_EXISTS: &str = "já existe";
pub const IMPORT_EXISTS_HELP: &str =
    "Já existe um perfil com o mesmo host, usuário e porta. Desmarcado por padrão.";
pub const IMPORT_INVALID: &str = "inválido";
pub const IMPORT_SELECT_ALL: &str = "Marcar todos";
pub const IMPORT_SELECT_NONE: &str = "Desmarcar todos";
pub const IMPORT_SELECT_NEW: &str = "Marcar só os novos";
pub const IMPORT_TARGET: &str = "Os perfis vão para a pasta “Importados”.";
pub const IMPORT_JUMP: &str = "salto";
pub fn import_button(n: usize) -> String {
    match n {
        1 => "Importar 1 perfil".to_string(),
        n => format!("Importar {n} perfis"),
    }
}
pub fn import_summary(total: usize, conflicts: usize) -> String {
    match conflicts {
        0 => format!("{total} hosts encontrados."),
        c => format!("{total} hosts encontrados, {c} já existem."),
    }
}
pub fn import_done(n: usize) -> String {
    match n {
        1 => "1 perfil importado para “Importados”.".to_string(),
        n => format!("{n} perfis importados para “Importados”."),
    }
}
pub fn import_warning_at(file: &str, line: usize, msg: &str) -> String {
    if line == 0 {
        msg.to_string()
    } else {
        let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
        format!("{name}:{line}: {msg}")
    }
}
pub const IMPORT_ERR_NOT_FOUND: &str = "Arquivo não encontrado.";
pub const IMPORT_ERR_DENIED: &str = "Sem permissão para ler o arquivo.";
pub const IMPORT_ERR_NOT_TEXT: &str = "O arquivo não é um texto válido.";
pub const IMPORT_ERR_READ: &str = "Não foi possível ler o arquivo.";
pub const IMPORT_ERR_NO_HOME: &str = "Não foi possível localizar o diretório do usuário.";
pub const IMPORT_ERR_SAVE: &str = "Não foi possível importar os perfis: ";

// Cofre: indicador e sessões
pub const VAULT_OPEN_BADGE: &str = "🔓 Cofre aberto";
pub const VAULT_LOCK_BTN_HELP: &str = "Bloquear o cofre: impede ler senhas e passphrases salvas em novas conexões. NÃO encerra as sessões abertas.";
pub const VAULT_UNLOCK_BTN_HELP: &str =
    "Desbloquear o cofre (as sessões abertas não foram afetadas).";
pub const END_ALL: &str = "Encerrar todas as sessões";
pub const END_ALL_HELP: &str =
    "Desconecta todas as sessões abertas. É diferente de bloquear o cofre.";
pub const END_ALL_TITLE: &str = "Encerrar todas as sessões?";
pub fn end_all_body(n: usize) -> String {
    format!(
        "{n} sessão(ões) ativa(s) serão desconectadas; as abas continuam abertas, desconectadas. Isto não bloqueia o cofre: para isso use o cadeado."
    )
}
pub const END_ALL_CONFIRM: &str = "Encerrar sessões";
pub const END_ALL_DONE: &str = "Sessões encerradas.";
pub const END_ALL_NONE: &str = "Nenhuma sessão ativa.";
pub const VAULT_LOCKED_SUSPEND: &str = "Cofre bloqueado após suspensão do computador.";
pub const S_LOCK_SUSPEND: &str = "Bloquear ao suspender";
pub const S_LOCK_SUSPEND_HELP: &str = "Bloqueia o cofre ao detectar que o computador suspendeu (por salto de relógio, percebido ao acordar).";

// Exportar / importar configuração
pub const S_CONFIG: &str = "Configuração (perfis e pastas)";
pub const CFG_EXPORT: &str = "Exportar configuração…";
pub const CFG_IMPORT: &str = "Importar configuração…";
pub const CFG_WARN: &str = "Credenciais NÃO são exportadas: senhas, passphrases e chaves confiáveis ficam de fora. Os caminhos das chaves privadas são incluídos, mas não os arquivos delas. A importação nunca sobrescreve perfis existentes.";
pub const CFG_EXPORT_TITLE: &str = "Exportar configuração";
pub const CFG_IMPORT_TITLE: &str = "Importar configuração";
pub const CFG_EXPORT_NAME: &str = "myssh-config.json";
pub fn cfg_exported(path: &str) -> String {
    format!("Configuração exportada para {path}.")
}
pub fn cfg_imported(profiles: usize, folders: usize) -> String {
    format!("Configuração importada: {profiles} perfil(is) e {folders} pasta(s) adicionados.")
}
pub const CFG_ERR_WRITE: &str = "Não foi possível gravar o arquivo de exportação.";
pub const CFG_ERR_READ: &str = "Não foi possível ler o arquivo de configuração.";
pub const CFG_ERR_TOO_BIG: &str = "O arquivo é grande demais para ser uma configuração.";
