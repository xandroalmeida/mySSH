//! Textos do M2-B: temas por perfil, painéis divididos, importação do ssh config, cofre,
//! exportar/importar configuração.

// Tema por perfil
pub fn TH_TITLE() -> &'static str {
    "Tema deste perfil"
}
pub fn TH_PALETTE() -> &'static str {
    "Paleta do terminal"
}
pub fn TH_INHERIT() -> &'static str {
    "Herdar do global"
}
pub fn TH_FONT() -> &'static str {
    "Fonte do terminal"
}
pub fn TH_FONT_OWN() -> &'static str {
    "Tamanho próprio"
}
pub fn TH_ACCENT() -> &'static str {
    "Cor de destaque"
}
pub fn TH_ACCENT_OWN() -> &'static str {
    "Usar destaque"
}
pub fn TH_ACCENT_NONE() -> &'static str {
    "sem destaque (herda)"
}
pub fn TH_RESET() -> &'static str {
    "Herdar tudo do global"
}
pub fn TH_SCOPE_HELP() -> &'static str {
    "Vale só para as abas e painéis deste perfil, não para o restante do app."
}
pub fn th_font_inherited(pt: f32) -> String {
    format!("herda do global ({pt:.1} pt)")
}
pub fn TH_SYSTEM_HELP() -> &'static str {
    "“Sistema” acompanha o tema claro/escuro do sistema operacional."
}

// Painéis divididos
pub fn SPLIT() -> &'static str {
    "Dividir"
}
pub fn SPLIT_SIDE() -> &'static str {
    "Lado a lado (divisor vertical)"
}
pub fn SPLIT_STACK() -> &'static str {
    "Empilhado (divisor horizontal)"
}
pub fn SPLIT_SAME() -> &'static str {
    "mesmo perfil"
}
pub fn SPLIT_OTHER() -> &'static str {
    "outro perfil…"
}
pub fn SPLIT_CLOSE_PANE() -> &'static str {
    "Fechar painel"
}
pub fn SPLIT_PICK_TITLE_SIDE() -> &'static str {
    "Dividir lado a lado com…"
}
pub fn SPLIT_PICK_TITLE_STACK() -> &'static str {
    "Dividir empilhado com…"
}
pub fn SPLIT_PICK_HELP() -> &'static str {
    "Escolha o perfil da nova sessão. Ela abre em um painel novo desta aba; a sessão atual continua como está."
}
pub fn SPLIT_OPEN() -> &'static str {
    "Dividir"
}
pub fn SPLIT_LIMIT() -> &'static str {
    "Limite de painéis por aba atingido."
}
pub fn SPLIT_ONLY_TERMINAL() -> &'static str {
    "Só abas de terminal podem ser divididas."
}
pub fn CLOSE_PANE_TITLE() -> &'static str {
    "Fechar painel com sessão ativa?"
}
pub fn CLOSE_PANE_CONFIRM() -> &'static str {
    "Fechar painel"
}
pub fn close_pane_body(title: &str) -> String {
    format!("A sessão em “{title}” está ativa. Fechar o painel encerra a sessão.")
}

// Importação de ~/.ssh/config
pub fn IMPORT_SSH() -> &'static str {
    "Importar ~/.ssh/config…"
}
pub fn IMPORT_TITLE() -> &'static str {
    "Importar do ~/.ssh/config"
}
pub fn IMPORT_INTRO() -> &'static str {
    "Lê o arquivo (somente leitura: o arquivo nunca é alterado) e mostra uma prévia antes de criar qualquer perfil. Diretivas que executam comandos locais são ignoradas."
}
pub fn IMPORT_USE_DEFAULT() -> &'static str {
    "Usar ~/.ssh/config"
}
pub fn IMPORT_PICK_FILE() -> &'static str {
    "Escolher outro arquivo…"
}
pub fn IMPORT_PICK_TITLE() -> &'static str {
    "Escolher arquivo ssh config"
}
pub fn IMPORT_LOADING() -> &'static str {
    "Lendo o arquivo…"
}
pub fn IMPORT_SAVING() -> &'static str {
    "Importando…"
}
pub fn IMPORT_NONE() -> &'static str {
    "Nenhum host importável foi encontrado neste arquivo."
}
pub fn IMPORT_GLOBAL_WARNINGS() -> &'static str {
    "Avisos gerais"
}
pub fn IMPORT_EXISTS() -> &'static str {
    "já existe"
}
pub fn IMPORT_EXISTS_HELP() -> &'static str {
    "Já existe um perfil com o mesmo host, usuário e porta. Desmarcado por padrão."
}
pub fn IMPORT_INVALID() -> &'static str {
    "inválido"
}
pub fn IMPORT_SELECT_ALL() -> &'static str {
    "Marcar todos"
}
pub fn IMPORT_SELECT_NONE() -> &'static str {
    "Desmarcar todos"
}
pub fn IMPORT_SELECT_NEW() -> &'static str {
    "Marcar só os novos"
}
pub fn IMPORT_TARGET() -> &'static str {
    "Os perfis vão para a pasta “Importados”."
}
pub fn IMPORT_JUMP() -> &'static str {
    "salto"
}
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
pub fn IMPORT_ERR_NOT_FOUND() -> &'static str {
    "Arquivo não encontrado."
}
pub fn IMPORT_ERR_DENIED() -> &'static str {
    "Sem permissão para ler o arquivo."
}
pub fn IMPORT_ERR_NOT_TEXT() -> &'static str {
    "O arquivo não é um texto válido."
}
pub fn IMPORT_ERR_READ() -> &'static str {
    "Não foi possível ler o arquivo."
}
pub fn IMPORT_ERR_NO_HOME() -> &'static str {
    "Não foi possível localizar o diretório do usuário."
}
pub fn IMPORT_ERR_SAVE() -> &'static str {
    "Não foi possível importar os perfis: "
}

// Cofre: indicador e sessões
pub fn VAULT_OPEN_BADGE() -> &'static str {
    "🔓 Cofre aberto"
}
pub fn VAULT_LOCK_BTN_HELP() -> &'static str {
    "Bloquear o cofre: impede ler senhas e passphrases salvas em novas conexões. NÃO encerra as sessões abertas."
}
pub fn VAULT_UNLOCK_BTN_HELP() -> &'static str {
    "Desbloquear o cofre (as sessões abertas não foram afetadas)."
}
pub fn END_ALL() -> &'static str {
    "Encerrar todas as sessões"
}
pub fn END_ALL_HELP() -> &'static str {
    "Desconecta todas as sessões abertas. É diferente de bloquear o cofre."
}
pub fn END_ALL_TITLE() -> &'static str {
    "Encerrar todas as sessões?"
}
pub fn end_all_body(n: usize) -> String {
    format!(
        "{n} sessão(ões) ativa(s) serão desconectadas; as abas continuam abertas, desconectadas. Isto não bloqueia o cofre: para isso use o cadeado."
    )
}
pub fn END_ALL_CONFIRM() -> &'static str {
    "Encerrar sessões"
}
pub fn END_ALL_DONE() -> &'static str {
    "Sessões encerradas."
}
pub fn END_ALL_NONE() -> &'static str {
    "Nenhuma sessão ativa."
}
pub fn VAULT_LOCKED_SUSPEND() -> &'static str {
    "Cofre bloqueado após suspensão do computador."
}
pub fn S_LOCK_SUSPEND() -> &'static str {
    "Bloquear ao suspender"
}
pub fn S_LOCK_SUSPEND_HELP() -> &'static str {
    "Bloqueia o cofre ao detectar que o computador suspendeu (por salto de relógio, percebido ao acordar)."
}

// Exportar / importar configuração
pub fn S_CONFIG() -> &'static str {
    "Configuração (perfis e pastas)"
}
pub fn CFG_EXPORT() -> &'static str {
    "Exportar configuração…"
}
pub fn CFG_IMPORT() -> &'static str {
    "Importar configuração…"
}
pub fn CFG_WARN() -> &'static str {
    "Credenciais NÃO são exportadas: senhas, passphrases e chaves confiáveis ficam de fora. Os caminhos das chaves privadas são incluídos, mas não os arquivos delas. A importação nunca sobrescreve perfis existentes."
}
pub fn CFG_EXPORT_TITLE() -> &'static str {
    "Exportar configuração"
}
pub fn CFG_IMPORT_TITLE() -> &'static str {
    "Importar configuração"
}
pub fn CFG_EXPORT_NAME() -> &'static str {
    "myssh-config.json"
}
pub fn cfg_exported(path: &str) -> String {
    format!("Configuração exportada para {path}.")
}
pub fn cfg_imported(profiles: usize, folders: usize) -> String {
    format!("Configuração importada: {profiles} perfil(is) e {folders} pasta(s) adicionados.")
}
pub fn CFG_ERR_WRITE() -> &'static str {
    "Não foi possível gravar o arquivo de exportação."
}
pub fn CFG_ERR_READ() -> &'static str {
    "Não foi possível ler o arquivo de configuração."
}
pub fn CFG_ERR_TOO_BIG() -> &'static str {
    "O arquivo é grande demais para ser uma configuração."
}
