//! Textos da sincronização (M4-B).

pub fn FILES_SYNC() -> &'static str {
    "Sincronizar…"
}
pub fn FILES_SYNC_HINT() -> &'static str {
    "Sincronizar a pasta local aberta com a pasta remota aberta (com prévia)"
}
pub fn SYNC_UNAVAILABLE() -> &'static str {
    "Sincronização indisponível: o SFTP ainda não está aberto."
}
pub fn SYNC_TITLE() -> &'static str {
    "Sincronizar pastas"
}
pub fn SYNC_RESUME() -> &'static str {
    "Retomar"
}
pub fn SYNC_DISCARD() -> &'static str {
    "Descartar"
}
pub fn SYNC_RESUME_HELP() -> &'static str {
    "A retomada usa as pastas atuais das colunas, mostra o que falta e revalida tudo antes de executar."
}
pub fn sync_resume_found(n: usize) -> String {
    format!("Há uma sincronização interrompida com {n} item(ns) pendentes.")
}
pub fn SYNC_DIRECTION() -> &'static str {
    "Direção"
}
pub fn SYNC_L2R() -> &'static str {
    "Local ➡ Remoto"
}
pub fn SYNC_R2L() -> &'static str {
    "Remoto ➡ Local"
}
pub fn sync_dest_changes(dest: &str) -> String {
    format!("O destino ({dest}) será alterado: arquivos novos e diferentes serão gravados nele.")
}
pub fn SYNC_COMPARE() -> &'static str {
    "Comparação"
}
pub fn SYNC_COMPARE_FAST() -> &'static str {
    "Tamanho e data (rápida; tolerância de 2 s)"
}
pub fn SYNC_COMPARE_CONTENT() -> &'static str {
    "Conteúdo (hash dos arquivos de mesmo tamanho)"
}
pub fn SYNC_CONTENT_COST() -> &'static str {
    "Custo de rede: cada arquivo de mesmo tamanho é lido por inteiro dos dois lados pelo SFTP. Pode ser lento e consumir banda."
}
pub fn SYNC_EXCLUDES() -> &'static str {
    "Exclusões"
}
pub fn SYNC_EXCLUDES_HELP() -> &'static str {
    "Um padrão por linha (ex.: .git/, *.log, build/). Itens excluídos nunca são apagados no destino."
}
pub fn SYNC_MIRROR() -> &'static str {
    "Modo espelho: apagar do destino o que não existe na origem"
}
pub fn SYNC_MIRROR_WARNING() -> &'static str {
    "ATENÇÃO: no modo espelho, itens que existem só no destino aparecem como exclusões na prévia e serão APAGADOS se você confirmar. Nada é apagado sem a sua confirmação explícita."
}
pub fn SYNC_PREVIEW() -> &'static str {
    "Gerar prévia"
}
pub fn SYNC_PLANNING() -> &'static str {
    "Gerando a prévia (nada é alterado)…"
}
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
pub fn SYNC_SEARCH() -> &'static str {
    "Filtrar por caminho"
}
pub fn SYNC_SKIP() -> &'static str {
    "Pular"
}
pub fn SYNC_USE_SOURCE() -> &'static str {
    "Usar origem"
}
pub fn SYNC_NOT_RESOLVABLE() -> &'static str {
    "(não resolvido automaticamente)"
}
pub fn SYNC_NO_ITEMS() -> &'static str {
    "Nenhum item neste filtro."
}
pub fn sync_delete_section(n: usize) -> String {
    format!("Exclusões no destino: {n} itens existem só no destino e serão APAGADOS.")
}
pub fn SYNC_NOTHING_TO_DO() -> &'static str {
    "Nada a executar com as escolhas atuais."
}
pub fn SYNC_BACK() -> &'static str {
    "Voltar às opções"
}
pub fn SYNC_RUNNING() -> &'static str {
    "Sincronizando…"
}
pub fn sync_progress_line(
    items_done: usize,
    items_total: usize,
    done: &str,
    total: &str,
) -> String {
    format!("itens {items_done}/{items_total} · {done} / {total}")
}
pub fn SYNC_CANCEL() -> &'static str {
    "Cancelar sincronização"
}
pub fn SYNC_HIDE() -> &'static str {
    "Ocultar"
}
pub fn SYNC_SHOW() -> &'static str {
    "Mostrar"
}
pub fn SYNC_RUNNING_HIDDEN() -> &'static str {
    "Sincronização em andamento"
}
pub fn SYNC_CANCELLED() -> &'static str {
    "Sincronização cancelada"
}
pub fn SYNC_DONE_WITH_FAILURES() -> &'static str {
    "Sincronização concluída com falhas"
}
pub fn SYNC_DONE() -> &'static str {
    "Sincronização concluída"
}
pub fn sync_final_line(
    done: usize,
    failed: usize,
    skipped: usize,
    pending: usize,
    bytes: &str,
) -> String {
    format!("{done} ok · {failed} com falha · {skipped} pulados · {pending} pendentes · {bytes}")
}
pub fn SYNC_PENDING_HELP() -> &'static str {
    "Os itens pendentes ficam registrados: abra “Sincronizar…” de novo para retomar."
}
pub fn SYNC_NEW() -> &'static str {
    "Nova sincronização"
}
pub fn SYNC_STALE_TITLE() -> &'static str {
    "A prévia ficou velha: nada foi executado"
}
pub fn sync_stale_body(n: usize) -> String {
    format!(
        "A origem ou o destino mudou depois da prévia ({n} item(ns)). Por segurança o plano antigo nunca é executado: gere uma nova prévia e confira de novo."
    )
}
pub fn SYNC_SIDE_SOURCE() -> &'static str {
    "mudou na origem"
}
pub fn SYNC_SIDE_DEST() -> &'static str {
    "mudou no destino"
}
pub fn SYNC_NEW_PREVIEW() -> &'static str {
    "Gerar nova prévia"
}
pub fn SYNC_FAILED_TITLE() -> &'static str {
    "A sincronização não pôde continuar"
}
pub fn sync_active_warning(n: usize) -> String {
    if n == 1 {
        "Há uma sincronização em andamento. Se continuar, ela será cancelada; os itens pendentes ficam registrados para retomar depois.".into()
    } else {
        format!(
            "Há {n} sincronizações em andamento. Se continuar, serão canceladas; os itens pendentes ficam registrados para retomar depois."
        )
    }
}
