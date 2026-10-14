//! Textos da sincronização (M4-B).

pub const FILES_SYNC: &str = "Sincronizar…";
pub const FILES_SYNC_HINT: &str =
    "Sincronizar a pasta local aberta com a pasta remota aberta (com prévia)";
pub const SYNC_UNAVAILABLE: &str = "Sincronização indisponível: o SFTP ainda não está aberto.";
pub const SYNC_TITLE: &str = "Sincronizar pastas";
pub const SYNC_RESUME: &str = "Retomar";
pub const SYNC_DISCARD: &str = "Descartar";
pub const SYNC_RESUME_HELP: &str = "A retomada usa as pastas atuais das colunas, mostra o que falta e revalida tudo antes de executar.";
pub fn sync_resume_found(n: usize) -> String {
    format!("Há uma sincronização interrompida com {n} item(ns) pendentes.")
}
pub const SYNC_DIRECTION: &str = "Direção";
pub const SYNC_L2R: &str = "Local ➡ Remoto";
pub const SYNC_R2L: &str = "Remoto ➡ Local";
pub fn sync_dest_changes(dest: &str) -> String {
    format!("O destino ({dest}) será alterado: arquivos novos e diferentes serão gravados nele.")
}
pub const SYNC_COMPARE: &str = "Comparação";
pub const SYNC_COMPARE_FAST: &str = "Tamanho e data (rápida; tolerância de 2 s)";
pub const SYNC_COMPARE_CONTENT: &str = "Conteúdo (hash dos arquivos de mesmo tamanho)";
pub const SYNC_CONTENT_COST: &str = "Custo de rede: cada arquivo de mesmo tamanho é lido por inteiro dos dois lados pelo SFTP. Pode ser lento e consumir banda.";
pub const SYNC_EXCLUDES: &str = "Exclusões";
pub const SYNC_EXCLUDES_HELP: &str = "Um padrão por linha (ex.: .git/, *.log, build/). Itens excluídos nunca são apagados no destino.";
pub const SYNC_MIRROR: &str = "Modo espelho: apagar do destino o que não existe na origem";
pub const SYNC_MIRROR_WARNING: &str = "ATENÇÃO: no modo espelho, itens que existem só no destino aparecem como exclusões na prévia e serão APAGADOS se você confirmar. Nada é apagado sem a sua confirmação explícita.";
pub const SYNC_PREVIEW: &str = "Gerar prévia";
pub const SYNC_PLANNING: &str = "Gerando a prévia (nada é alterado)…";
pub fn sync_hashing(done: usize, total: usize) -> String {
    format!("Comparando conteúdo: {done} de {total}")
}
pub fn sync_summary(
    create: usize,
    update: usize,
    conflict: usize,
    delete: usize,
    skip: usize,
    bytes: &str,
) -> String {
    format!(
        "Criar {create} · Atualizar {update} · Conflitos {conflict} · Excluir {delete} · Pular {skip} · {bytes} a transferir"
    )
}
pub fn sync_direction_line(l2r: bool) -> &'static str {
    if l2r {
        "Local ➡ Remoto: o remoto será alterado."
    } else {
        "Remoto ➡ Local: o local será alterado."
    }
}
pub const SYNC_SEARCH: &str = "Filtrar por caminho";
pub const SYNC_SKIP: &str = "Pular";
pub const SYNC_USE_SOURCE: &str = "Usar origem";
pub const SYNC_NOT_RESOLVABLE: &str = "(não resolvido automaticamente)";
pub const SYNC_NO_ITEMS: &str = "Nenhum item neste filtro.";
pub fn sync_delete_section(n: usize) -> String {
    format!("Exclusões no destino: {n} itens existem só no destino e serão APAGADOS.")
}
pub const SYNC_NOTHING_TO_DO: &str = "Nada a executar com as escolhas atuais.";
pub const SYNC_BACK: &str = "Voltar às opções";
pub const SYNC_RUNNING: &str = "Sincronizando…";
pub fn sync_progress_line(
    items_done: usize,
    items_total: usize,
    done: &str,
    total: &str,
) -> String {
    format!("itens {items_done}/{items_total} · {done} / {total}")
}
pub const SYNC_CANCEL: &str = "Cancelar sincronização";
pub const SYNC_HIDE: &str = "Ocultar";
pub const SYNC_SHOW: &str = "Mostrar";
pub const SYNC_RUNNING_HIDDEN: &str = "Sincronização em andamento";
pub const SYNC_CANCELLED: &str = "Sincronização cancelada";
pub const SYNC_DONE_WITH_FAILURES: &str = "Sincronização concluída com falhas";
pub const SYNC_DONE: &str = "Sincronização concluída";
pub fn sync_final_line(
    done: usize,
    failed: usize,
    skipped: usize,
    pending: usize,
    bytes: &str,
) -> String {
    format!("{done} ok · {failed} com falha · {skipped} pulados · {pending} pendentes · {bytes}")
}
pub const SYNC_PENDING_HELP: &str =
    "Os itens pendentes ficam registrados: abra “Sincronizar…” de novo para retomar.";
pub const SYNC_NEW: &str = "Nova sincronização";
pub const SYNC_STALE_TITLE: &str = "A prévia ficou velha: nada foi executado";
pub fn sync_stale_body(n: usize) -> String {
    format!(
        "A origem ou o destino mudou depois da prévia ({n} item(ns)). Por segurança o plano antigo nunca é executado: gere uma nova prévia e confira de novo."
    )
}
pub const SYNC_SIDE_SOURCE: &str = "mudou na origem";
pub const SYNC_SIDE_DEST: &str = "mudou no destino";
pub const SYNC_NEW_PREVIEW: &str = "Gerar nova prévia";
pub const SYNC_FAILED_TITLE: &str = "A sincronização não pôde continuar";
pub fn sync_active_warning(n: usize) -> String {
    if n == 1 {
        "Há uma sincronização em andamento. Se continuar, ela será cancelada; os itens pendentes ficam registrados para retomar depois.".into()
    } else {
        format!(
            "Há {n} sincronizações em andamento. Se continuar, serão canceladas; os itens pendentes ficam registrados para retomar depois."
        )
    }
}
