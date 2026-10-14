//! Textos do painel de arquivos e da fila de transferências (M3-B).

use crate::files::fmt::fmt_size;

pub const FILES_BUTTON: &str = "Arquivos";
pub const FILES_BUTTON_HINT: &str = "Mostrar/ocultar o painel de arquivos (SFTP)";
pub const FILES_TITLE: &str = "Arquivos";
pub const FILES_CLOSE: &str = "Fechar painel";
pub const FILES_LOCAL: &str = "Local";
pub const FILES_REMOTE: &str = "Remoto";
pub const FILES_BACK: &str = "Voltar";
pub const FILES_UP: &str = "Subir";
pub const FILES_REFRESH: &str = "Atualizar";
pub const FILES_HOME: &str = "Início";
pub const FILES_HIDDEN: &str = "Ocultos";
pub const FILES_HIDDEN_HINT: &str = "Mostrar arquivos ocultos (nomes que começam com ponto)";
pub const FILES_PATH_LABEL: &str = "Caminho";
pub const FILES_NAME_FIELD: &str = "Nome do item";
pub const FILES_PATH_HINT: &str = "Caminho (Enter para ir)";
pub const FILES_COL_NAME: &str = "Nome";
pub const FILES_COL_SIZE: &str = "Tamanho";
pub const FILES_COL_MODIFIED: &str = "Modificado";
pub const FILES_COL_KIND: &str = "Tipo";
pub const FILES_KIND_DIR: &str = "Pasta";
pub const FILES_KIND_FILE: &str = "Arquivo";
pub const FILES_KIND_LINK: &str = "Link";
pub const FILES_KIND_OTHER: &str = "Outro";
pub const FILES_UPLOAD: &str = "⬆ Enviar";
pub const FILES_DOWNLOAD: &str = "⬇ Baixar";
pub const FILES_UPLOAD_HINT: &str =
    "Enviar os itens selecionados da coluna local para a pasta remota aberta";
pub const FILES_DOWNLOAD_HINT: &str =
    "Baixar os itens selecionados da coluna remota para a pasta local aberta";
pub const FILES_CM_UPLOAD: &str = "Enviar para o remoto";
pub const FILES_CM_DOWNLOAD: &str = "Baixar para a pasta local";
pub const FILES_NEW_DIR: &str = "Nova pasta…";
pub const FILES_RENAME: &str = "Renomear…";
pub const FILES_DELETE: &str = "Excluir…";
pub const FILES_EMPTY: &str = "Pasta vazia.";
pub const FILES_LOADING: &str = "Carregando…";
pub const FILES_RETRY: &str = "Tentar novamente";
pub const FILES_REMOTE_UNAVAILABLE: &str =
    "Navegação remota indisponível: o SFTP ainda não está ligado a esta sessão.";
pub const FILES_NOTHING_SELECTED: &str = "Selecione itens para transferir.";
pub const FILES_NO_REMOTE_DIR: &str = "A pasta remota ainda não foi carregada.";
pub const FILES_DROP_HINT: &str = "Solte aqui para enviar para a pasta remota";
pub const FILES_POLICY_LABEL: &str = "Se já existir:";
pub const FILES_POLICY_ASK: &str = "Perguntar";
pub const FILES_POLICY_OVERWRITE: &str = "Sobrescrever";
pub const FILES_POLICY_SKIP: &str = "Pular";
pub const FILES_POLICY_RENAME: &str = "Renomear";
pub const FILES_POLICY_HINT: &str = "Política de sobrescrita das próximas transferências";

pub const FILES_ERR_NOT_FOUND: &str = "Não encontrado.";
pub const FILES_ERR_PERMISSION: &str = "Permissão negada.";
pub const FILES_ERR_EXISTS: &str = "Já existe um item com esse nome.";
pub fn files_err_io(e: &str) -> String {
    format!("Erro de E/S: {e}")
}

pub const FILES_NAME_EMPTY: &str = "Informe um nome.";
pub const FILES_NAME_INVALID: &str = "Nome inválido: não use “/”, “.” nem “..”.";
pub const FILES_NEWDIR_TITLE: &str = "Nova pasta";
pub const FILES_RENAME_TITLE: &str = "Renomear";
pub const FILES_NAME_LABEL: &str = "Nome";
pub const FILES_CREATE: &str = "Criar";
pub const FILES_RENAME_CONFIRM: &str = "Renomear";

pub fn files_delete_title(remote: bool, n: usize) -> String {
    let onde = if remote { "remota" } else { "local" };
    if n == 1 {
        format!("Excluir 1 item ({onde})?")
    } else {
        format!("Excluir {n} itens ({onde})?")
    }
}
pub const FILES_DELETE_PERMANENT: &str =
    "A exclusão é permanente: os itens não vão para a lixeira e não podem ser recuperados.";
pub const FILES_DELETE_RECURSIVE: &str =
    "A seleção inclui pastas: todo o conteúdo delas será excluído, recursivamente.";
pub const FILES_DELETE_ACK: &str =
    "Entendo que isto exclui o conteúdo das pastas de forma permanente";
pub const FILES_DELETE_CONFIRM: &str = "Excluir";
pub fn files_delete_more(n: usize) -> String {
    format!("… e mais {n}")
}

// Conflito
pub const CONFLICT_TITLE: &str = "Já existe um item com esse nome";
pub const CONFLICT_EXISTING: &str = "No destino (já existe)";
pub const CONFLICT_INCOMING: &str = "O que seria gravado";
pub const CONFLICT_SIZE: &str = "Tamanho";
pub const CONFLICT_DATE: &str = "Modificado";
pub const CONFLICT_UNKNOWN: &str = "desconhecido";
pub const CONFLICT_OVERWRITE: &str = "Sobrescrever";
pub const CONFLICT_SKIP: &str = "Pular";
pub const CONFLICT_RENAME: &str = "Renomear";
pub const CONFLICT_ALL: &str = "Aplicar a todos os conflitos desta transferência";
pub const CONFLICT_CANCEL: &str = "Cancelar transferência";
pub fn conflict_pending(n: usize) -> String {
    if n > 1 {
        format!("({} conflitos aguardando)", n)
    } else {
        String::new()
    }
}

// Fila
pub const XFER_QUEUED: &str = "Na fila";
pub const XFER_SCANNING: &str = "Contando";
pub const XFER_RUNNING: &str = "Transferindo";
pub const XFER_DONE: &str = "Concluída";
pub const XFER_FAILED: &str = "Com falhas";
pub const XFER_CANCELLED: &str = "Cancelada";
pub const XFER_CANCELLING: &str = "Cancelando…";
pub const XFER_CANCEL: &str = "Cancelar";
pub const XFER_CANCEL_HINT: &str = "Cancela esta transferência e remove os arquivos temporários";
pub const XFER_CANCEL_ALL: &str = "Cancelar todas";
pub const XFER_CLEAR: &str = "Limpar concluídas";
pub const XFER_ETA: &str = "falta";
pub fn xfer_title(total: usize, active: usize) -> String {
    format!("Transferências ({active} ativas de {total})")
}
pub fn xfer_files(done: u64, total: u64) -> String {
    format!("arquivos {done}/{total}")
}
pub fn xfer_result(ok: u64, failed: u64, skipped: u64) -> String {
    format!("{ok} ok, {failed} com falha, {skipped} pulados")
}
pub fn xfer_counting(files: u64, bytes: u64) -> String {
    format!("Contando… {files} arquivos, {}", fmt_size(bytes))
}
pub fn xfer_errors(n: u64) -> String {
    if n == 1 {
        "1 arquivo com falha".into()
    } else {
        format!("{n} arquivos com falha")
    }
}
pub fn xfer_errors_hidden(n: u64) -> String {
    format!("… e mais {n} falhas não listadas")
}

// Fechamento com fila ativa
pub fn transfers_warning(n: usize) -> String {
    if n == 1 {
        "Há 1 transferência em andamento. Se continuar, ela será cancelada e os arquivos temporários removidos.".into()
    } else {
        format!(
            "Há {n} transferências em andamento. Se continuar, todas serão canceladas e os arquivos temporários removidos."
        )
    }
}

// Configurações
pub const S_FILES_POLICY: &str = "Se o arquivo já existir";
pub const S_FILES_POLICY_HELP: &str =
    "Padrão das transferências; pode ser mudado em cada uma no painel de arquivos.";
