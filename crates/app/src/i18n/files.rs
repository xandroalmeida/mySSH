//! Textos do painel de arquivos e da fila de transferências (M3-B).

use crate::files::fmt::fmt_size;

pub fn FILES_BUTTON() -> &'static str {
    "Arquivos"
}
pub fn FILES_BUTTON_HINT() -> &'static str {
    "Mostrar/ocultar o painel de arquivos (SFTP)"
}
pub fn FILES_TITLE() -> &'static str {
    "Arquivos"
}
pub fn FILES_CLOSE() -> &'static str {
    "Fechar painel"
}
pub fn FILES_LOCAL() -> &'static str {
    "Local"
}
pub fn FILES_REMOTE() -> &'static str {
    "Remoto"
}
pub fn FILES_BACK() -> &'static str {
    "Voltar"
}
pub fn FILES_UP() -> &'static str {
    "Subir"
}
pub fn FILES_REFRESH() -> &'static str {
    "Atualizar"
}
pub fn FILES_HOME() -> &'static str {
    "Início"
}
pub fn FILES_HIDDEN() -> &'static str {
    "Ocultos"
}
pub fn FILES_HIDDEN_HINT() -> &'static str {
    "Mostrar arquivos ocultos (nomes que começam com ponto)"
}
pub fn FILES_PATH_LABEL() -> &'static str {
    "Caminho"
}
pub fn FILES_NAME_FIELD() -> &'static str {
    "Nome do item"
}
pub fn FILES_PATH_HINT() -> &'static str {
    "Caminho (Enter para ir)"
}
pub fn FILES_COL_NAME() -> &'static str {
    "Nome"
}
pub fn FILES_COL_SIZE() -> &'static str {
    "Tamanho"
}
pub fn FILES_COL_MODIFIED() -> &'static str {
    "Modificado"
}
pub fn FILES_COL_KIND() -> &'static str {
    "Tipo"
}
pub fn FILES_KIND_DIR() -> &'static str {
    "Pasta"
}
pub fn FILES_KIND_FILE() -> &'static str {
    "Arquivo"
}
pub fn FILES_KIND_LINK() -> &'static str {
    "Link"
}
pub fn FILES_KIND_OTHER() -> &'static str {
    "Outro"
}
pub fn FILES_UPLOAD() -> &'static str {
    "⬆ Enviar"
}
pub fn FILES_DOWNLOAD() -> &'static str {
    "⬇ Baixar"
}
pub fn FILES_UPLOAD_HINT() -> &'static str {
    "Enviar os itens selecionados da coluna local para a pasta remota aberta"
}
pub fn FILES_DOWNLOAD_HINT() -> &'static str {
    "Baixar os itens selecionados da coluna remota para a pasta local aberta"
}
pub fn FILES_CM_UPLOAD() -> &'static str {
    "Enviar para o remoto"
}
pub fn FILES_CM_DOWNLOAD() -> &'static str {
    "Baixar para a pasta local"
}
pub fn FILES_NEW_DIR() -> &'static str {
    "Nova pasta…"
}
pub fn FILES_RENAME() -> &'static str {
    "Renomear…"
}
pub fn FILES_DELETE() -> &'static str {
    "Excluir…"
}
pub fn FILES_EMPTY() -> &'static str {
    "Pasta vazia."
}
pub fn FILES_LOADING() -> &'static str {
    "Carregando…"
}
pub fn FILES_RETRY() -> &'static str {
    "Tentar novamente"
}
pub fn FILES_REMOTE_UNAVAILABLE() -> &'static str {
    "Navegação remota indisponível: o SFTP ainda não está ligado a esta sessão."
}
pub fn FILES_REMOTE_OPENING() -> &'static str {
    "Abrindo o canal SFTP…"
}
pub fn FILES_NO_CONNECTION() -> &'static str {
    "Sem conexão: conecte a sessão para navegar no servidor."
}
pub fn files_sftp_failed(e: &str) -> String {
    format!("Não foi possível abrir o SFTP: {e}")
}
pub fn FILES_NOTHING_SELECTED() -> &'static str {
    "Selecione itens para transferir."
}
pub fn FILES_NO_REMOTE_DIR() -> &'static str {
    "A pasta remota ainda não foi carregada."
}
pub fn FILES_DROP_HINT() -> &'static str {
    "Solte aqui para enviar para a pasta remota"
}
pub fn FILES_POLICY_LABEL() -> &'static str {
    "Se já existir:"
}
pub fn FILES_POLICY_ASK() -> &'static str {
    "Perguntar"
}
pub fn FILES_POLICY_OVERWRITE() -> &'static str {
    "Sobrescrever"
}
pub fn FILES_POLICY_SKIP() -> &'static str {
    "Pular"
}
pub fn FILES_POLICY_RENAME() -> &'static str {
    "Renomear"
}
pub fn FILES_POLICY_HINT() -> &'static str {
    "Política de sobrescrita das próximas transferências"
}

pub fn FILES_ERR_NOT_FOUND() -> &'static str {
    "Não encontrado."
}
pub fn FILES_ERR_PERMISSION() -> &'static str {
    "Permissão negada."
}
pub fn FILES_ERR_EXISTS() -> &'static str {
    "Já existe um item com esse nome."
}
pub fn files_err_io(e: &str) -> String {
    format!("Erro de E/S: {e}")
}

pub fn FILES_NAME_EMPTY() -> &'static str {
    "Informe um nome."
}
pub fn FILES_NAME_INVALID() -> &'static str {
    "Nome inválido: não use “/”, “.” nem “..”."
}
pub fn FILES_NEWDIR_TITLE() -> &'static str {
    "Nova pasta"
}
pub fn FILES_RENAME_TITLE() -> &'static str {
    "Renomear"
}
pub fn FILES_NAME_LABEL() -> &'static str {
    "Nome"
}
pub fn FILES_CREATE() -> &'static str {
    "Criar"
}
pub fn FILES_RENAME_CONFIRM() -> &'static str {
    "Renomear"
}

pub fn files_delete_title(remote: bool, n: usize) -> String {
    let onde = if remote { "remota" } else { "local" };
    if n == 1 {
        format!("Excluir 1 item ({onde})?")
    } else {
        format!("Excluir {n} itens ({onde})?")
    }
}
pub fn FILES_DELETE_PERMANENT() -> &'static str {
    "A exclusão é permanente: os itens não vão para a lixeira e não podem ser recuperados."
}
pub fn FILES_DELETE_RECURSIVE() -> &'static str {
    "A seleção inclui pastas: todo o conteúdo delas será excluído, recursivamente."
}
pub fn FILES_DELETE_ACK() -> &'static str {
    "Entendo que isto exclui o conteúdo das pastas de forma permanente"
}
pub fn FILES_DELETE_CONFIRM() -> &'static str {
    "Excluir"
}
pub fn files_delete_more(n: usize) -> String {
    format!("… e mais {n}")
}

// Conflito
pub fn CONFLICT_TITLE() -> &'static str {
    "Já existe um item com esse nome"
}
pub fn CONFLICT_EXISTING() -> &'static str {
    "No destino (já existe)"
}
pub fn CONFLICT_INCOMING() -> &'static str {
    "O que seria gravado"
}
pub fn CONFLICT_SIZE() -> &'static str {
    "Tamanho"
}
pub fn CONFLICT_DATE() -> &'static str {
    "Modificado"
}
pub fn CONFLICT_UNKNOWN() -> &'static str {
    "desconhecido"
}
pub fn CONFLICT_OVERWRITE() -> &'static str {
    "Sobrescrever"
}
pub fn CONFLICT_SKIP() -> &'static str {
    "Pular"
}
pub fn CONFLICT_RENAME() -> &'static str {
    "Renomear"
}
pub fn CONFLICT_ALL() -> &'static str {
    "Aplicar a todos os conflitos desta transferência"
}
pub fn CONFLICT_CANCEL() -> &'static str {
    "Cancelar transferência"
}
pub fn conflict_pending(n: usize) -> String {
    if n > 1 {
        format!("({} conflitos aguardando)", n)
    } else {
        String::new()
    }
}

// Fila
pub fn XFER_QUEUED() -> &'static str {
    "Na fila"
}
pub fn XFER_SCANNING() -> &'static str {
    "Contando"
}
pub fn XFER_RUNNING() -> &'static str {
    "Transferindo"
}
pub fn XFER_DONE() -> &'static str {
    "Concluída"
}
pub fn XFER_FAILED() -> &'static str {
    "Com falhas"
}
pub fn XFER_FAILED_BAR() -> &'static str {
    "Falhou: há arquivos com erro"
}
pub fn XFER_CANCELLED() -> &'static str {
    "Cancelada"
}
pub fn XFER_CANCELLING() -> &'static str {
    "Cancelando…"
}
pub fn XFER_CANCEL() -> &'static str {
    "Cancelar"
}
pub fn XFER_CANCEL_HINT() -> &'static str {
    "Cancela esta transferência e remove os arquivos temporários"
}
pub fn XFER_CANCEL_ALL() -> &'static str {
    "Cancelar todas"
}
pub fn XFER_CLEAR() -> &'static str {
    "Limpar concluídas"
}
pub fn XFER_ETA() -> &'static str {
    "falta"
}
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
pub fn S_FILES_POLICY() -> &'static str {
    "Se o arquivo já existir"
}
pub fn S_FILES_POLICY_HELP() -> &'static str {
    "Padrão das transferências; pode ser mudado em cada uma no painel de arquivos."
}
