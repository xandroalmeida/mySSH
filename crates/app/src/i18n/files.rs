//! Textos do painel de arquivos e da fila de transferências (M3-B).

use crate::files::fmt::fmt_size;
use myssh_i18n::tr;

pub fn FILES_BUTTON() -> &'static str {
    tr!("Arquivos", "Files", "Archivos")
}
pub fn FILES_BUTTON_HINT() -> &'static str {
    tr!(
        "Mostrar/ocultar o painel de arquivos (SFTP)",
        "Show/hide the files pane (SFTP)",
        "Mostrar/ocultar el panel de archivos (SFTP)"
    )
}
pub fn FILES_TITLE() -> &'static str {
    tr!("Arquivos", "Files", "Archivos")
}
pub fn FILES_CLOSE() -> &'static str {
    tr!("Fechar painel", "Close pane", "Cerrar panel")
}
pub fn FILES_LOCAL() -> &'static str {
    tr!("Local", "Local", "Local")
}
pub fn FILES_REMOTE() -> &'static str {
    tr!("Remoto", "Remote", "Remoto")
}
pub fn FILES_BACK() -> &'static str {
    tr!("Voltar", "Back", "Atrás")
}
pub fn FILES_UP() -> &'static str {
    tr!("Subir", "Up", "Subir")
}
pub fn FILES_REFRESH() -> &'static str {
    tr!("Atualizar", "Refresh", "Actualizar")
}
pub fn FILES_HOME() -> &'static str {
    tr!("Início", "Home", "Inicio")
}
pub fn FILES_HIDDEN() -> &'static str {
    tr!("Ocultos", "Hidden", "Ocultos")
}
pub fn FILES_HIDDEN_HINT() -> &'static str {
    tr!(
        "Mostrar arquivos ocultos (nomes que começam com ponto)",
        "Show hidden files (names starting with a dot)",
        "Mostrar archivos ocultos (nombres que empiezan con punto)"
    )
}
pub fn FILES_PATH_LABEL() -> &'static str {
    tr!("Caminho", "Path", "Ruta")
}
pub fn FILES_NAME_FIELD() -> &'static str {
    tr!("Nome do item", "Item name", "Nombre del elemento")
}
pub fn FILES_PATH_HINT() -> &'static str {
    tr!(
        "Caminho (Enter para ir)",
        "Path (Enter to go)",
        "Ruta (Enter para ir)"
    )
}
pub fn FILES_COL_NAME() -> &'static str {
    tr!("Nome", "Name", "Nombre")
}
pub fn FILES_COL_SIZE() -> &'static str {
    tr!("Tamanho", "Size", "Tamaño")
}
pub fn FILES_COL_MODIFIED() -> &'static str {
    tr!("Modificado", "Modified", "Modificado")
}
pub fn FILES_COL_KIND() -> &'static str {
    tr!("Tipo", "Type", "Tipo")
}
pub fn FILES_KIND_DIR() -> &'static str {
    tr!("Pasta", "Folder", "Carpeta")
}
pub fn FILES_KIND_FILE() -> &'static str {
    tr!("Arquivo", "File", "Archivo")
}
pub fn FILES_KIND_LINK() -> &'static str {
    tr!("Link", "Link", "Enlace")
}
pub fn FILES_KIND_OTHER() -> &'static str {
    tr!("Outro", "Other", "Otro")
}
pub fn FILES_UPLOAD() -> &'static str {
    tr!("⬆ Enviar", "⬆ Upload", "⬆ Subir")
}
pub fn FILES_DOWNLOAD() -> &'static str {
    tr!("⬇ Baixar", "⬇ Download", "⬇ Descargar")
}
pub fn FILES_UPLOAD_HINT() -> &'static str {
    tr!(
        "Enviar os itens selecionados da coluna local para a pasta remota aberta",
        "Upload the selected items from the local column to the open remote folder",
        "Subir los elementos seleccionados de la columna local a la carpeta remota abierta"
    )
}
pub fn FILES_DOWNLOAD_HINT() -> &'static str {
    tr!(
        "Baixar os itens selecionados da coluna remota para a pasta local aberta",
        "Download the selected items from the remote column to the open local folder",
        "Descargar los elementos seleccionados de la columna remota a la carpeta local abierta"
    )
}
pub fn FILES_CM_UPLOAD() -> &'static str {
    tr!(
        "Enviar para o remoto",
        "Upload to remote",
        "Subir al remoto"
    )
}
pub fn FILES_CM_DOWNLOAD() -> &'static str {
    tr!(
        "Baixar para a pasta local",
        "Download to local folder",
        "Descargar a la carpeta local"
    )
}
pub fn FILES_NEW_DIR() -> &'static str {
    tr!("Nova pasta…", "New folder…", "Nueva carpeta…")
}
pub fn FILES_RENAME() -> &'static str {
    tr!("Renomear…", "Rename…", "Renombrar…")
}
pub fn FILES_DELETE() -> &'static str {
    tr!("Excluir…", "Delete…", "Eliminar…")
}
pub fn FILES_EMPTY() -> &'static str {
    tr!("Pasta vazia.", "Empty folder.", "Carpeta vacía.")
}
pub fn FILES_LOADING() -> &'static str {
    tr!("Carregando…", "Loading…", "Cargando…")
}
pub fn FILES_RETRY() -> &'static str {
    tr!("Tentar novamente", "Try again", "Reintentar")
}
pub fn FILES_REMOTE_UNAVAILABLE() -> &'static str {
    tr!(
        "Navegação remota indisponível: o SFTP ainda não está ligado a esta sessão.",
        "Remote browsing unavailable: SFTP is not yet attached to this session.",
        "Navegación remota no disponible: SFTP aún no está conectado a esta sesión."
    )
}
pub fn FILES_REMOTE_OPENING() -> &'static str {
    tr!(
        "Abrindo o canal SFTP…",
        "Opening the SFTP channel…",
        "Abriendo el canal SFTP…"
    )
}
pub fn FILES_NO_CONNECTION() -> &'static str {
    tr!(
        "Sem conexão: conecte a sessão para navegar no servidor.",
        "Not connected: connect the session to browse the server.",
        "Sin conexión: conecta la sesión para explorar el servidor."
    )
}
pub fn files_sftp_failed(e: &str) -> String {
    tr!(
        format!("Não foi possível abrir o SFTP: {e}"),
        format!("Could not open SFTP: {e}"),
        format!("No se pudo abrir SFTP: {e}")
    )
}
pub fn FILES_NOTHING_SELECTED() -> &'static str {
    tr!(
        "Selecione itens para transferir.",
        "Select items to transfer.",
        "Selecciona elementos para transferir."
    )
}
pub fn FILES_NO_REMOTE_DIR() -> &'static str {
    tr!(
        "A pasta remota ainda não foi carregada.",
        "The remote folder has not been loaded yet.",
        "La carpeta remota aún no se ha cargado."
    )
}
pub fn FILES_DROP_HINT() -> &'static str {
    tr!(
        "Solte aqui para enviar para a pasta remota",
        "Drop here to upload to the remote folder",
        "Suelta aquí para subir a la carpeta remota"
    )
}
pub fn FILES_POLICY_LABEL() -> &'static str {
    tr!("Se já existir:", "If it already exists:", "Si ya existe:")
}
pub fn FILES_POLICY_ASK() -> &'static str {
    tr!("Perguntar", "Ask", "Preguntar")
}
pub fn FILES_POLICY_OVERWRITE() -> &'static str {
    tr!("Sobrescrever", "Overwrite", "Sobrescribir")
}
pub fn FILES_POLICY_SKIP() -> &'static str {
    tr!("Pular", "Skip", "Omitir")
}
pub fn FILES_POLICY_RENAME() -> &'static str {
    tr!("Renomear", "Rename", "Renombrar")
}
pub fn FILES_POLICY_HINT() -> &'static str {
    tr!(
        "Política de sobrescrita das próximas transferências",
        "Overwrite policy for upcoming transfers",
        "Política de sobrescritura de las próximas transferencias"
    )
}

pub fn FILES_ERR_NOT_FOUND() -> &'static str {
    tr!("Não encontrado.", "Not found.", "No encontrado.")
}
pub fn FILES_ERR_PERMISSION() -> &'static str {
    tr!(
        "Permissão negada.",
        "Permission denied.",
        "Permiso denegado."
    )
}
pub fn FILES_ERR_EXISTS() -> &'static str {
    tr!(
        "Já existe um item com esse nome.",
        "An item with that name already exists.",
        "Ya existe un elemento con ese nombre."
    )
}
pub fn files_err_io(e: &str) -> String {
    tr!(
        format!("Erro de E/S: {e}"),
        format!("I/O error: {e}"),
        format!("Error de E/S: {e}")
    )
}

pub fn FILES_NAME_EMPTY() -> &'static str {
    tr!("Informe um nome.", "Enter a name.", "Escribe un nombre.")
}
pub fn FILES_NAME_INVALID() -> &'static str {
    tr!(
        "Nome inválido: não use “/”, “.” nem “..”.",
        "Invalid name: don't use “/”, “.” or “..”.",
        "Nombre no válido: no uses “/”, “.” ni “..”."
    )
}
pub fn FILES_NEWDIR_TITLE() -> &'static str {
    tr!("Nova pasta", "New folder", "Nueva carpeta")
}
pub fn FILES_RENAME_TITLE() -> &'static str {
    tr!("Renomear", "Rename", "Renombrar")
}
pub fn FILES_NAME_LABEL() -> &'static str {
    tr!("Nome", "Name", "Nombre")
}
pub fn FILES_CREATE() -> &'static str {
    tr!("Criar", "Create", "Crear")
}
pub fn FILES_RENAME_CONFIRM() -> &'static str {
    tr!("Renomear", "Rename", "Renombrar")
}

pub fn files_delete_title(remote: bool, n: usize) -> String {
    let onde = if remote {
        tr!("remota", "remote", "remota")
    } else {
        tr!("local", "local", "local")
    };
    if n == 1 {
        tr!(
            format!("Excluir 1 item ({onde})?"),
            format!("Delete 1 item ({onde})?"),
            format!("¿Eliminar 1 elemento ({onde})?")
        )
    } else {
        tr!(
            format!("Excluir {n} itens ({onde})?"),
            format!("Delete {n} items ({onde})?"),
            format!("¿Eliminar {n} elementos ({onde})?")
        )
    }
}
pub fn FILES_DELETE_PERMANENT() -> &'static str {
    tr!(
        "A exclusão é permanente: os itens não vão para a lixeira e não podem ser recuperados.",
        "Deletion is permanent: items do not go to the trash and cannot be recovered.",
        "La eliminación es permanente: los elementos no van a la papelera y no se pueden recuperar."
    )
}
pub fn FILES_DELETE_RECURSIVE() -> &'static str {
    tr!(
        "A seleção inclui pastas: todo o conteúdo delas será excluído, recursivamente.",
        "The selection includes folders: all of their contents will be deleted, recursively.",
        "La selección incluye carpetas: todo su contenido se eliminará, de forma recursiva."
    )
}
pub fn FILES_DELETE_ACK() -> &'static str {
    tr!(
        "Entendo que isto exclui o conteúdo das pastas de forma permanente",
        "I understand this permanently deletes the contents of the folders",
        "Entiendo que esto elimina el contenido de las carpetas de forma permanente"
    )
}
pub fn FILES_DELETE_CONFIRM() -> &'static str {
    tr!("Excluir", "Delete", "Eliminar")
}
pub fn files_delete_more(n: usize) -> String {
    tr!(
        format!("… e mais {n}"),
        format!("… and {n} more"),
        format!("… y {n} más")
    )
}

// Conflito
pub fn CONFLICT_TITLE() -> &'static str {
    tr!(
        "Já existe um item com esse nome",
        "An item with that name already exists",
        "Ya existe un elemento con ese nombre"
    )
}
pub fn CONFLICT_EXISTING() -> &'static str {
    tr!(
        "No destino (já existe)",
        "At the destination (already exists)",
        "En el destino (ya existe)"
    )
}
pub fn CONFLICT_INCOMING() -> &'static str {
    tr!(
        "O que seria gravado",
        "What would be written",
        "Lo que se escribiría"
    )
}
pub fn CONFLICT_SIZE() -> &'static str {
    tr!("Tamanho", "Size", "Tamaño")
}
pub fn CONFLICT_DATE() -> &'static str {
    tr!("Modificado", "Modified", "Modificado")
}
pub fn CONFLICT_UNKNOWN() -> &'static str {
    tr!("desconhecido", "unknown", "desconocido")
}
pub fn CONFLICT_OVERWRITE() -> &'static str {
    tr!("Sobrescrever", "Overwrite", "Sobrescribir")
}
pub fn CONFLICT_SKIP() -> &'static str {
    tr!("Pular", "Skip", "Omitir")
}
pub fn CONFLICT_RENAME() -> &'static str {
    tr!("Renomear", "Rename", "Renombrar")
}
pub fn CONFLICT_ALL() -> &'static str {
    tr!(
        "Aplicar a todos os conflitos desta transferência",
        "Apply to all conflicts in this transfer",
        "Aplicar a todos los conflictos de esta transferencia"
    )
}
pub fn CONFLICT_CANCEL() -> &'static str {
    tr!(
        "Cancelar transferência",
        "Cancel transfer",
        "Cancelar transferencia"
    )
}
pub fn conflict_pending(n: usize) -> String {
    if n > 1 {
        tr!(
            format!("({} conflitos aguardando)", n),
            format!("({} conflicts waiting)", n),
            format!("({} conflictos en espera)", n)
        )
    } else {
        String::new()
    }
}

// Fila
pub fn XFER_QUEUED() -> &'static str {
    tr!("Na fila", "Queued", "En cola")
}
pub fn XFER_SCANNING() -> &'static str {
    tr!("Contando", "Counting", "Contando")
}
pub fn XFER_RUNNING() -> &'static str {
    tr!("Transferindo", "Transferring", "Transfiriendo")
}
pub fn XFER_DONE() -> &'static str {
    tr!("Concluída", "Completed", "Completada")
}
pub fn XFER_FAILED() -> &'static str {
    tr!("Com falhas", "With failures", "Con fallos")
}
pub fn XFER_FAILED_BAR() -> &'static str {
    tr!(
        "Falhou: há arquivos com erro",
        "Failed: some files have errors",
        "Falló: hay archivos con error"
    )
}
pub fn XFER_CANCELLED() -> &'static str {
    tr!("Cancelada", "Cancelled", "Cancelada")
}
pub fn XFER_CANCELLING() -> &'static str {
    tr!("Cancelando…", "Cancelling…", "Cancelando…")
}
pub fn XFER_CANCEL() -> &'static str {
    tr!("Cancelar", "Cancel", "Cancelar")
}
pub fn XFER_CANCEL_HINT() -> &'static str {
    tr!(
        "Cancela esta transferência e remove os arquivos temporários",
        "Cancels this transfer and removes temporary files",
        "Cancela esta transferencia y elimina los archivos temporales"
    )
}
pub fn XFER_CANCEL_ALL() -> &'static str {
    tr!("Cancelar todas", "Cancel all", "Cancelar todas")
}
pub fn XFER_CLEAR() -> &'static str {
    tr!(
        "Limpar concluídas",
        "Clear completed",
        "Limpiar completadas"
    )
}
pub fn XFER_ETA() -> &'static str {
    tr!("falta", "ETA", "faltan")
}
pub fn xfer_title(total: usize, active: usize) -> String {
    tr!(
        format!("Transferências ({active} ativas de {total})"),
        format!("Transfers ({active} active of {total})"),
        format!("Transferencias ({active} activas de {total})")
    )
}
pub fn xfer_files(done: u64, total: u64) -> String {
    tr!(
        format!("arquivos {done}/{total}"),
        format!("files {done}/{total}"),
        format!("archivos {done}/{total}")
    )
}
pub fn xfer_result(ok: u64, failed: u64, skipped: u64) -> String {
    tr!(
        format!("{ok} ok, {failed} com falha, {skipped} pulados"),
        format!("{ok} ok, {failed} failed, {skipped} skipped"),
        format!("{ok} ok, {failed} con fallos, {skipped} omitidos")
    )
}
pub fn xfer_counting(files: u64, bytes: u64) -> String {
    tr!(
        format!("Contando… {files} arquivos, {}", fmt_size(bytes)),
        format!("Counting… {files} files, {}", fmt_size(bytes)),
        format!("Contando… {files} archivos, {}", fmt_size(bytes))
    )
}
pub fn xfer_errors(n: u64) -> String {
    if n == 1 {
        tr!(
            "1 arquivo com falha".to_string(),
            "1 file failed".to_string(),
            "1 archivo con fallo".to_string()
        )
    } else {
        tr!(
            format!("{n} arquivos com falha"),
            format!("{n} files failed"),
            format!("{n} archivos con fallos")
        )
    }
}
pub fn xfer_errors_hidden(n: u64) -> String {
    tr!(
        format!("… e mais {n} falhas não listadas"),
        format!("… and {n} more failures not listed"),
        format!("… y {n} fallos más sin listar")
    )
}

// Fechamento com fila ativa
pub fn transfers_warning(n: usize) -> String {
    if n == 1 {
        tr!(
            "Há 1 transferência em andamento. Se continuar, ela será cancelada e os arquivos temporários removidos.".to_string(),
            "There is 1 transfer in progress. If you continue, it will be cancelled and temporary files removed.".to_string(),
            "Hay 1 transferencia en curso. Si continúas, se cancelará y se eliminarán los archivos temporales.".to_string()
        )
    } else {
        tr!(
            format!(
                "Há {n} transferências em andamento. Se continuar, todas serão canceladas e os arquivos temporários removidos."
            ),
            format!(
                "There are {n} transfers in progress. If you continue, all will be cancelled and temporary files removed."
            ),
            format!(
                "Hay {n} transferencias en curso. Si continúas, todas se cancelarán y se eliminarán los archivos temporales."
            )
        )
    }
}

// Configurações
pub fn S_FILES_POLICY() -> &'static str {
    tr!(
        "Se o arquivo já existir",
        "If the file already exists",
        "Si el archivo ya existe"
    )
}
pub fn S_FILES_POLICY_HELP() -> &'static str {
    tr!(
        "Padrão das transferências; pode ser mudado em cada uma no painel de arquivos.",
        "Default for transfers; can be changed for each one in the files pane.",
        "Valor predeterminado de las transferencias; se puede cambiar en cada una desde el panel de archivos."
    )
}
