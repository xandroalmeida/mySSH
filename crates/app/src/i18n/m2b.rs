//! Textos do M2-B: temas por perfil, painéis divididos, importação do ssh config, cofre,
//! exportar/importar configuração.

use myssh_i18n::tr;

// Tema por perfil
pub fn TH_TITLE() -> &'static str {
    tr!(
        "Tema deste perfil",
        "Theme for this profile",
        "Tema de este perfil"
    )
}
pub fn TH_PALETTE() -> &'static str {
    tr!(
        "Paleta do terminal",
        "Terminal palette",
        "Paleta del terminal"
    )
}
pub fn TH_INHERIT() -> &'static str {
    tr!(
        "Herdar do global",
        "Inherit from global",
        "Heredar del global"
    )
}
pub fn TH_FONT() -> &'static str {
    tr!("Fonte do terminal", "Terminal font", "Fuente del terminal")
}
pub fn TH_FONT_OWN() -> &'static str {
    tr!("Tamanho próprio", "Custom size", "Tamaño propio")
}
pub fn TH_ACCENT() -> &'static str {
    tr!("Cor de destaque", "Accent color", "Color de acento")
}
pub fn TH_ACCENT_OWN() -> &'static str {
    tr!("Usar destaque", "Use accent", "Usar acento")
}
pub fn TH_ACCENT_NONE() -> &'static str {
    tr!(
        "sem destaque (herda)",
        "no accent (inherits)",
        "sin acento (hereda)"
    )
}
pub fn TH_RESET() -> &'static str {
    tr!(
        "Herdar tudo do global",
        "Inherit everything from global",
        "Heredar todo del global"
    )
}
pub fn TH_SCOPE_HELP() -> &'static str {
    tr!(
        "Vale só para as abas e painéis deste perfil, não para o restante do app.",
        "Applies only to this profile's tabs and panes, not to the rest of the app.",
        "Vale solo para las pestañas y paneles de este perfil, no para el resto de la app."
    )
}
pub fn th_font_inherited(pt: f32) -> String {
    tr!(
        format!("herda do global ({pt:.1} pt)"),
        format!("inherits global ({pt:.1} pt)"),
        format!("hereda del global ({pt:.1} pt)")
    )
}
pub fn TH_SYSTEM_HELP() -> &'static str {
    tr!(
        "“Sistema” acompanha o tema claro/escuro do sistema operacional.",
        "“System” follows the operating system's light/dark theme.",
        "“Sistema” sigue el tema claro/oscuro del sistema operativo."
    )
}

// Painéis divididos
pub fn SPLIT() -> &'static str {
    tr!("Dividir", "Split", "Dividir")
}
pub fn SPLIT_SIDE() -> &'static str {
    tr!(
        "Lado a lado (divisor vertical)",
        "Side by side (vertical divider)",
        "Lado a lado (divisor vertical)"
    )
}
pub fn SPLIT_STACK() -> &'static str {
    tr!(
        "Empilhado (divisor horizontal)",
        "Stacked (horizontal divider)",
        "Apilado (divisor horizontal)"
    )
}
pub fn SPLIT_SAME() -> &'static str {
    tr!("mesmo perfil", "same profile", "mismo perfil")
}
pub fn SPLIT_OTHER() -> &'static str {
    tr!("outro perfil…", "another profile…", "otro perfil…")
}
pub fn SPLIT_CLOSE_PANE() -> &'static str {
    tr!("Fechar painel", "Close pane", "Cerrar panel")
}
pub fn SPLIT_PICK_TITLE_SIDE() -> &'static str {
    tr!(
        "Dividir lado a lado com…",
        "Split side by side with…",
        "Dividir lado a lado con…"
    )
}
pub fn SPLIT_PICK_TITLE_STACK() -> &'static str {
    tr!(
        "Dividir empilhado com…",
        "Split stacked with…",
        "Dividir apilado con…"
    )
}
pub fn SPLIT_PICK_HELP() -> &'static str {
    tr!(
        "Escolha o perfil da nova sessão. Ela abre em um painel novo desta aba; a sessão atual continua como está.",
        "Choose the profile for the new session. It opens in a new pane in this tab; the current session stays as it is.",
        "Elige el perfil de la nueva sesión. Se abre en un panel nuevo de esta pestaña; la sesión actual sigue como está."
    )
}
pub fn SPLIT_OPEN() -> &'static str {
    tr!("Dividir", "Split", "Dividir")
}
pub fn SPLIT_LIMIT() -> &'static str {
    tr!(
        "Limite de painéis por aba atingido.",
        "Pane limit per tab reached.",
        "Se alcanzó el límite de paneles por pestaña."
    )
}
pub fn SPLIT_ONLY_TERMINAL() -> &'static str {
    tr!(
        "Só abas de terminal podem ser divididas.",
        "Only terminal tabs can be split.",
        "Solo se pueden dividir las pestañas de terminal."
    )
}
pub fn CLOSE_PANE_TITLE() -> &'static str {
    tr!(
        "Fechar painel com sessão ativa?",
        "Close pane with an active session?",
        "¿Cerrar el panel con una sesión activa?"
    )
}
pub fn CLOSE_PANE_CONFIRM() -> &'static str {
    tr!("Fechar painel", "Close pane", "Cerrar panel")
}
pub fn close_pane_body(title: &str) -> String {
    tr!(
        format!("A sessão em “{title}” está ativa. Fechar o painel encerra a sessão."),
        format!("The session in “{title}” is active. Closing the pane ends the session."),
        format!("La sesión en “{title}” está activa. Cerrar el panel termina la sesión.")
    )
}

// Importação de ~/.ssh/config
pub fn IMPORT_SSH() -> &'static str {
    tr!(
        "Importar ~/.ssh/config…",
        "Import ~/.ssh/config…",
        "Importar ~/.ssh/config…"
    )
}
pub fn IMPORT_TITLE() -> &'static str {
    tr!(
        "Importar do ~/.ssh/config",
        "Import from ~/.ssh/config",
        "Importar desde ~/.ssh/config"
    )
}
pub fn IMPORT_INTRO() -> &'static str {
    tr!(
        "Lê o arquivo (somente leitura: o arquivo nunca é alterado) e mostra uma prévia antes de criar qualquer perfil. Diretivas que executam comandos locais são ignoradas.",
        "Reads the file (read-only: the file is never modified) and shows a preview before creating any profile. Directives that run local commands are ignored.",
        "Lee el archivo (solo lectura: el archivo nunca se modifica) y muestra una vista previa antes de crear cualquier perfil. Las directivas que ejecutan comandos locales se ignoran."
    )
}
pub fn IMPORT_USE_DEFAULT() -> &'static str {
    tr!(
        "Usar ~/.ssh/config",
        "Use ~/.ssh/config",
        "Usar ~/.ssh/config"
    )
}
pub fn IMPORT_PICK_FILE() -> &'static str {
    tr!(
        "Escolher outro arquivo…",
        "Choose another file…",
        "Elegir otro archivo…"
    )
}
pub fn IMPORT_PICK_TITLE() -> &'static str {
    tr!(
        "Escolher arquivo ssh config",
        "Choose ssh config file",
        "Elegir archivo ssh config"
    )
}
pub fn IMPORT_LOADING() -> &'static str {
    tr!(
        "Lendo o arquivo…",
        "Reading the file…",
        "Leyendo el archivo…"
    )
}
pub fn IMPORT_SAVING() -> &'static str {
    tr!("Importando…", "Importing…", "Importando…")
}
pub fn IMPORT_NONE() -> &'static str {
    tr!(
        "Nenhum host importável foi encontrado neste arquivo.",
        "No importable hosts were found in this file.",
        "No se encontró ningún host importable en este archivo."
    )
}
pub fn IMPORT_GLOBAL_WARNINGS() -> &'static str {
    tr!("Avisos gerais", "General warnings", "Avisos generales")
}
pub fn IMPORT_EXISTS() -> &'static str {
    tr!("já existe", "already exists", "ya existe")
}
pub fn IMPORT_EXISTS_HELP() -> &'static str {
    tr!(
        "Já existe um perfil com o mesmo host, usuário e porta. Desmarcado por padrão.",
        "A profile with the same host, user and port already exists. Unchecked by default.",
        "Ya existe un perfil con el mismo host, usuario y puerto. Desmarcado por defecto."
    )
}
pub fn IMPORT_INVALID() -> &'static str {
    tr!("inválido", "invalid", "no válido")
}
pub fn IMPORT_SELECT_ALL() -> &'static str {
    tr!("Marcar todos", "Select all", "Marcar todos")
}
pub fn IMPORT_SELECT_NONE() -> &'static str {
    tr!("Desmarcar todos", "Select none", "Desmarcar todos")
}
pub fn IMPORT_SELECT_NEW() -> &'static str {
    tr!(
        "Marcar só os novos",
        "Select only new",
        "Marcar solo los nuevos"
    )
}
pub fn IMPORT_TARGET() -> &'static str {
    tr!(
        "Os perfis vão para a pasta “Imported”.",
        "Profiles go to the “Imported” folder.",
        "Los perfiles van a la carpeta “Imported”."
    )
}
pub fn IMPORT_JUMP() -> &'static str {
    tr!("salto", "jump", "salto")
}
pub fn import_button(n: usize) -> String {
    match n {
        1 => tr!(
            "Importar 1 perfil".to_string(),
            "Import 1 profile".to_string(),
            "Importar 1 perfil".to_string()
        ),
        n => tr!(
            format!("Importar {n} perfis"),
            format!("Import {n} profiles"),
            format!("Importar {n} perfiles")
        ),
    }
}
pub fn import_summary(total: usize, conflicts: usize) -> String {
    match conflicts {
        0 => tr!(
            format!("{total} hosts encontrados."),
            format!("{total} hosts found."),
            format!("{total} hosts encontrados.")
        ),
        c => tr!(
            format!("{total} hosts encontrados, {c} já existem."),
            format!("{total} hosts found, {c} already exist."),
            format!("{total} hosts encontrados, {c} ya existen.")
        ),
    }
}
pub fn import_done(n: usize) -> String {
    match n {
        1 => tr!(
            "1 perfil importado para “Imported”.".to_string(),
            "1 profile imported into “Imported”.".to_string(),
            "1 perfil importado en “Imported”.".to_string()
        ),
        n => tr!(
            format!("{n} perfis importados para “Imported”."),
            format!("{n} profiles imported into “Imported”."),
            format!("{n} perfiles importados en “Imported”.")
        ),
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
    tr!(
        "Arquivo não encontrado.",
        "File not found.",
        "Archivo no encontrado."
    )
}
pub fn IMPORT_ERR_DENIED() -> &'static str {
    tr!(
        "Sem permissão para ler o arquivo.",
        "No permission to read the file.",
        "Sin permiso para leer el archivo."
    )
}
pub fn IMPORT_ERR_NOT_TEXT() -> &'static str {
    tr!(
        "O arquivo não é um texto válido.",
        "The file is not valid text.",
        "El archivo no es un texto válido."
    )
}
pub fn IMPORT_ERR_READ() -> &'static str {
    tr!(
        "Não foi possível ler o arquivo.",
        "Could not read the file.",
        "No se pudo leer el archivo."
    )
}
pub fn IMPORT_ERR_NO_HOME() -> &'static str {
    tr!(
        "Não foi possível localizar o diretório do usuário.",
        "Could not locate the user's home directory.",
        "No se pudo localizar el directorio del usuario."
    )
}
pub fn IMPORT_ERR_SAVE() -> &'static str {
    tr!(
        "Não foi possível importar os perfis: ",
        "Could not import the profiles: ",
        "No se pudieron importar los perfiles: "
    )
}

// Cofre: indicador e sessões
pub fn VAULT_OPEN_BADGE() -> &'static str {
    tr!("🔓 Cofre aberto", "🔓 Vault unlocked", "🔓 Bóveda abierta")
}
pub fn VAULT_LOCK_BTN_HELP() -> &'static str {
    tr!(
        "Bloquear o cofre: impede ler senhas e passphrases salvas em novas conexões. NÃO encerra as sessões abertas.",
        "Lock the vault: prevents reading saved passwords and passphrases for new connections. Does NOT end open sessions.",
        "Bloquear la bóveda: impide leer contraseñas y passphrases guardadas en nuevas conexiones. NO cierra las sesiones abiertas."
    )
}
pub fn VAULT_UNLOCK_BTN_HELP() -> &'static str {
    tr!(
        "Desbloquear o cofre (as sessões abertas não foram afetadas).",
        "Unlock the vault (open sessions were not affected).",
        "Desbloquear la bóveda (las sesiones abiertas no se han visto afectadas)."
    )
}
pub fn END_ALL() -> &'static str {
    tr!(
        "Encerrar todas as sessões",
        "End all sessions",
        "Cerrar todas las sesiones"
    )
}
pub fn END_ALL_HELP() -> &'static str {
    tr!(
        "Desconecta todas as sessões abertas. É diferente de bloquear o cofre.",
        "Disconnects all open sessions. This is different from locking the vault.",
        "Desconecta todas las sesiones abiertas. Es distinto de bloquear la bóveda."
    )
}
pub fn END_ALL_TITLE() -> &'static str {
    tr!(
        "Encerrar todas as sessões?",
        "End all sessions?",
        "¿Cerrar todas las sesiones?"
    )
}
pub fn end_all_body(n: usize) -> String {
    tr!(
        format!(
            "{n} sessão(ões) ativa(s) serão desconectadas; as abas continuam abertas, desconectadas. Isto não bloqueia o cofre: para isso use o cadeado."
        ),
        format!(
            "{n} active session(s) will be disconnected; the tabs stay open, disconnected. This does not lock the vault: use the padlock for that."
        ),
        format!(
            "{n} sesión(es) activa(s) se desconectarán; las pestañas siguen abiertas, desconectadas. Esto no bloquea la bóveda: para eso usa el candado."
        )
    )
}
pub fn END_ALL_CONFIRM() -> &'static str {
    tr!("Encerrar sessões", "End sessions", "Cerrar sesiones")
}
pub fn END_ALL_DONE() -> &'static str {
    tr!(
        "Sessões encerradas.",
        "Sessions ended.",
        "Sesiones cerradas."
    )
}
pub fn END_ALL_NONE() -> &'static str {
    tr!(
        "Nenhuma sessão ativa.",
        "No active sessions.",
        "Ninguna sesión activa."
    )
}
pub fn VAULT_LOCKED_SUSPEND() -> &'static str {
    tr!(
        "Cofre bloqueado após suspensão do computador.",
        "Vault locked after the computer was suspended.",
        "Bóveda bloqueada tras la suspensión del equipo."
    )
}
pub fn S_LOCK_SUSPEND() -> &'static str {
    tr!(
        "Bloquear ao suspender",
        "Lock on suspend",
        "Bloquear al suspender"
    )
}
pub fn S_LOCK_SUSPEND_HELP() -> &'static str {
    tr!(
        "Bloqueia o cofre ao detectar que o computador suspendeu (por salto de relógio, percebido ao acordar).",
        "Locks the vault when it detects the computer was suspended (via a clock jump, noticed on wake).",
        "Bloquea la bóveda al detectar que el equipo se suspendió (por un salto de reloj, detectado al despertar)."
    )
}

// Exportar / importar configuração
pub fn S_CONFIG() -> &'static str {
    tr!(
        "Configuração (perfis e pastas)",
        "Configuration (profiles and folders)",
        "Configuración (perfiles y carpetas)"
    )
}
pub fn CFG_EXPORT() -> &'static str {
    tr!(
        "Exportar configuração…",
        "Export configuration…",
        "Exportar configuración…"
    )
}
pub fn CFG_IMPORT() -> &'static str {
    tr!(
        "Importar configuração…",
        "Import configuration…",
        "Importar configuración…"
    )
}
pub fn CFG_WARN() -> &'static str {
    tr!(
        "Credenciais NÃO são exportadas: senhas, passphrases e chaves confiáveis ficam de fora. Os caminhos das chaves privadas são incluídos, mas não os arquivos delas. A importação nunca sobrescreve perfis existentes.",
        "Credentials are NOT exported: passwords, passphrases and trusted keys are left out. Private key paths are included, but not the key files themselves. Importing never overwrites existing profiles.",
        "Las credenciales NO se exportan: contraseñas, passphrases y claves de confianza quedan fuera. Se incluyen las rutas de las claves privadas, pero no sus archivos. La importación nunca sobrescribe perfiles existentes."
    )
}
pub fn CFG_EXPORT_TITLE() -> &'static str {
    tr!(
        "Exportar configuração",
        "Export configuration",
        "Exportar configuración"
    )
}
pub fn CFG_IMPORT_TITLE() -> &'static str {
    tr!(
        "Importar configuração",
        "Import configuration",
        "Importar configuración"
    )
}
pub fn CFG_EXPORT_NAME() -> &'static str {
    "myssh-config.json"
}
pub fn cfg_exported(path: &str) -> String {
    tr!(
        format!("Configuração exportada para {path}."),
        format!("Configuration exported to {path}."),
        format!("Configuración exportada a {path}.")
    )
}
pub fn cfg_imported(profiles: usize, folders: usize) -> String {
    tr!(
        format!("Configuração importada: {profiles} perfil(is) e {folders} pasta(s) adicionados."),
        format!("Configuration imported: {profiles} profile(s) and {folders} folder(s) added."),
        format!("Configuración importada: {profiles} perfil(es) y {folders} carpeta(s) añadidos.")
    )
}
pub fn CFG_ERR_WRITE() -> &'static str {
    tr!(
        "Não foi possível gravar o arquivo de exportação.",
        "Could not write the export file.",
        "No se pudo escribir el archivo de exportación."
    )
}
pub fn CFG_ERR_READ() -> &'static str {
    tr!(
        "Não foi possível ler o arquivo de configuração.",
        "Could not read the configuration file.",
        "No se pudo leer el archivo de configuración."
    )
}
pub fn CFG_ERR_TOO_BIG() -> &'static str {
    tr!(
        "O arquivo é grande demais para ser uma configuração.",
        "The file is too large to be a configuration.",
        "El archivo es demasiado grande para ser una configuración."
    )
}
