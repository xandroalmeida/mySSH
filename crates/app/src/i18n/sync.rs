//! Textos da sincronização (M4-B).

use myssh_i18n::tr;

pub fn FILES_SYNC() -> &'static str {
    tr!("Sincronizar…", "Sync…", "Sincronizar…")
}
pub fn FILES_SYNC_HINT() -> &'static str {
    tr!(
        "Sincronizar a pasta local aberta com a pasta remota aberta (com prévia)",
        "Sync the open local folder with the open remote folder (with preview)",
        "Sincronizar la carpeta local abierta con la carpeta remota abierta (con vista previa)"
    )
}
pub fn SYNC_UNAVAILABLE() -> &'static str {
    tr!(
        "Sincronização indisponível: o SFTP ainda não está aberto.",
        "Sync unavailable: SFTP is not open yet.",
        "Sincronización no disponible: SFTP aún no está abierto."
    )
}
pub fn SYNC_TITLE() -> &'static str {
    tr!("Sincronizar pastas", "Sync folders", "Sincronizar carpetas")
}
pub fn SYNC_RESUME() -> &'static str {
    tr!("Retomar", "Resume", "Reanudar")
}
pub fn SYNC_DISCARD() -> &'static str {
    tr!("Descartar", "Discard", "Descartar")
}
pub fn SYNC_RESUME_HELP() -> &'static str {
    tr!(
        "A retomada usa as pastas atuais das colunas, mostra o que falta e revalida tudo antes de executar.",
        "Resuming uses the folders currently open in the columns, shows what remains and revalidates everything before running.",
        "Reanudar usa las carpetas actuales de las columnas, muestra lo que falta y revalida todo antes de ejecutar."
    )
}
pub fn sync_resume_found(n: usize) -> String {
    tr!(
        format!("Há uma sincronização interrompida com {n} item(ns) pendentes."),
        format!("There is an interrupted sync with {n} pending item(s)."),
        format!("Hay una sincronización interrumpida con {n} elemento(s) pendiente(s).")
    )
}
pub fn SYNC_DIRECTION() -> &'static str {
    tr!("Direção", "Direction", "Dirección")
}
pub fn SYNC_L2R() -> &'static str {
    tr!("Local ➡ Remoto", "Local ➡ Remote", "Local ➡ Remoto")
}
pub fn SYNC_R2L() -> &'static str {
    tr!("Remoto ➡ Local", "Remote ➡ Local", "Remoto ➡ Local")
}
pub fn sync_dest_changes(dest: &str) -> String {
    tr!(
        format!(
            "O destino ({dest}) será alterado: arquivos novos e diferentes serão gravados nele."
        ),
        format!(
            "The destination ({dest}) will be changed: new and different files will be written to it."
        ),
        format!(
            "El destino ({dest}) se modificará: se escribirán en él los archivos nuevos y diferentes."
        )
    )
}
pub fn SYNC_COMPARE() -> &'static str {
    tr!("Comparação", "Comparison", "Comparación")
}
pub fn SYNC_COMPARE_FAST() -> &'static str {
    tr!(
        "Tamanho e data (rápida; tolerância de 2 s)",
        "Size and date (fast; 2 s tolerance)",
        "Tamaño y fecha (rápida; tolerancia de 2 s)"
    )
}
pub fn SYNC_COMPARE_CONTENT() -> &'static str {
    tr!(
        "Conteúdo (hash dos arquivos de mesmo tamanho)",
        "Content (hash of same-size files)",
        "Contenido (hash de los archivos del mismo tamaño)"
    )
}
pub fn SYNC_CONTENT_COST() -> &'static str {
    tr!(
        "Custo de rede: cada arquivo de mesmo tamanho é lido por inteiro dos dois lados pelo SFTP. Pode ser lento e consumir banda.",
        "Network cost: every same-size file is read in full on both sides over SFTP. This can be slow and use bandwidth.",
        "Costo de red: cada archivo del mismo tamaño se lee completo en ambos lados por SFTP. Puede ser lento y consumir ancho de banda."
    )
}
pub fn SYNC_EXCLUDES() -> &'static str {
    tr!("Exclusões", "Exclusions", "Exclusiones")
}
pub fn SYNC_EXCLUDES_HELP() -> &'static str {
    tr!(
        "Um padrão por linha (ex.: .git/, *.log, build/). Itens excluídos nunca são apagados no destino.",
        "One pattern per line (e.g. .git/, *.log, build/). Excluded items are never deleted at the destination.",
        "Un patrón por línea (p. ej.: .git/, *.log, build/). Los elementos excluidos nunca se borran en el destino."
    )
}
pub fn SYNC_MIRROR() -> &'static str {
    tr!(
        "Modo espelho: apagar do destino o que não existe na origem",
        "Mirror mode: delete from the destination what doesn't exist at the source",
        "Modo espejo: borrar del destino lo que no existe en el origen"
    )
}
pub fn SYNC_MIRROR_WARNING() -> &'static str {
    tr!(
        "ATENÇÃO: no modo espelho, itens que existem só no destino aparecem como exclusões na prévia e serão APAGADOS se você confirmar. Nada é apagado sem a sua confirmação explícita.",
        "WARNING: in mirror mode, items that exist only at the destination appear as deletions in the preview and will be DELETED if you confirm. Nothing is deleted without your explicit confirmation.",
        "ATENCIÓN: en el modo espejo, los elementos que existen solo en el destino aparecen como eliminaciones en la vista previa y se BORRARÁN si confirmas. Nada se borra sin tu confirmación explícita."
    )
}
pub fn SYNC_PREVIEW() -> &'static str {
    tr!("Gerar prévia", "Generate preview", "Generar vista previa")
}
pub fn SYNC_PLANNING() -> &'static str {
    tr!(
        "Gerando a prévia (nada é alterado)…",
        "Generating the preview (nothing is changed)…",
        "Generando la vista previa (no se modifica nada)…"
    )
}
pub fn sync_hashing(done: usize, total: usize) -> String {
    tr!(
        format!("Comparando conteúdo: {done} de {total}"),
        format!("Comparing content: {done} of {total}"),
        format!("Comparando contenido: {done} de {total}")
    )
}
pub fn sync_summary(
    create: usize,
    update: usize,
    conflict: usize,
    delete: usize,
    skip: usize,
    bytes: &str,
) -> String {
    tr!(
        format!(
            "Criar {create} · Atualizar {update} · Conflitos {conflict} · Excluir {delete} · Pular {skip} · {bytes} a transferir"
        ),
        format!(
            "Create {create} · Update {update} · Conflicts {conflict} · Delete {delete} · Skip {skip} · {bytes} to transfer"
        ),
        format!(
            "Crear {create} · Actualizar {update} · Conflictos {conflict} · Eliminar {delete} · Omitir {skip} · {bytes} por transferir"
        )
    )
}
pub fn sync_direction_line(l2r: bool) -> &'static str {
    if l2r {
        tr!(
            "Local ➡ Remoto: o remoto será alterado.",
            "Local ➡ Remote: the remote will be changed.",
            "Local ➡ Remoto: se modificará el remoto."
        )
    } else {
        tr!(
            "Remoto ➡ Local: o local será alterado.",
            "Remote ➡ Local: the local side will be changed.",
            "Remoto ➡ Local: se modificará el local."
        )
    }
}
pub fn SYNC_SEARCH() -> &'static str {
    tr!("Filtrar por caminho", "Filter by path", "Filtrar por ruta")
}
pub fn SYNC_SKIP() -> &'static str {
    tr!("Pular", "Skip", "Omitir")
}
pub fn SYNC_USE_SOURCE() -> &'static str {
    tr!("Usar origem", "Use source", "Usar origen")
}
pub fn SYNC_NOT_RESOLVABLE() -> &'static str {
    tr!(
        "(não resolvido automaticamente)",
        "(not resolved automatically)",
        "(no se resuelve automáticamente)"
    )
}
pub fn SYNC_NO_ITEMS() -> &'static str {
    tr!(
        "Nenhum item neste filtro.",
        "No items match this filter.",
        "Ningún elemento en este filtro."
    )
}
pub fn sync_delete_section(n: usize) -> String {
    tr!(
        format!("Exclusões no destino: {n} itens existem só no destino e serão APAGADOS."),
        format!(
            "Deletions at the destination: {n} items exist only at the destination and will be DELETED."
        ),
        format!(
            "Eliminaciones en el destino: {n} elementos existen solo en el destino y se BORRARÁN."
        )
    )
}
pub fn SYNC_NOTHING_TO_DO() -> &'static str {
    tr!(
        "Nada a executar com as escolhas atuais.",
        "Nothing to run with the current choices.",
        "Nada que ejecutar con las opciones actuales."
    )
}
pub fn SYNC_BACK() -> &'static str {
    tr!(
        "Voltar às opções",
        "Back to options",
        "Volver a las opciones"
    )
}
pub fn SYNC_RUNNING() -> &'static str {
    tr!("Sincronizando…", "Syncing…", "Sincronizando…")
}
pub fn sync_progress_line(
    items_done: usize,
    items_total: usize,
    done: &str,
    total: &str,
) -> String {
    tr!(
        format!("itens {items_done}/{items_total} · {done} / {total}"),
        format!("items {items_done}/{items_total} · {done} / {total}"),
        format!("elementos {items_done}/{items_total} · {done} / {total}")
    )
}
pub fn SYNC_CANCEL() -> &'static str {
    tr!(
        "Cancelar sincronização",
        "Cancel sync",
        "Cancelar sincronización"
    )
}
pub fn SYNC_HIDE() -> &'static str {
    tr!("Ocultar", "Hide", "Ocultar")
}
pub fn SYNC_SHOW() -> &'static str {
    tr!("Mostrar", "Show", "Mostrar")
}
pub fn SYNC_RUNNING_HIDDEN() -> &'static str {
    tr!(
        "Sincronização em andamento",
        "Sync in progress",
        "Sincronización en curso"
    )
}
pub fn SYNC_CANCELLED() -> &'static str {
    tr!(
        "Sincronização cancelada",
        "Sync cancelled",
        "Sincronización cancelada"
    )
}
pub fn SYNC_DONE_WITH_FAILURES() -> &'static str {
    tr!(
        "Sincronização concluída com falhas",
        "Sync completed with failures",
        "Sincronización completada con fallos"
    )
}
pub fn SYNC_DONE() -> &'static str {
    tr!(
        "Sincronização concluída",
        "Sync completed",
        "Sincronización completada"
    )
}
pub fn sync_final_line(
    done: usize,
    failed: usize,
    skipped: usize,
    pending: usize,
    bytes: &str,
) -> String {
    tr!(
        format!(
            "{done} ok · {failed} com falha · {skipped} pulados · {pending} pendentes · {bytes}"
        ),
        format!("{done} ok · {failed} failed · {skipped} skipped · {pending} pending · {bytes}"),
        format!(
            "{done} ok · {failed} con fallos · {skipped} omitidos · {pending} pendientes · {bytes}"
        )
    )
}
pub fn SYNC_PENDING_HELP() -> &'static str {
    tr!(
        "Os itens pendentes ficam registrados: abra “Sincronizar…” de novo para retomar.",
        "Pending items are saved: open “Sync…” again to resume.",
        "Los elementos pendientes quedan registrados: abre “Sincronizar…” de nuevo para reanudar."
    )
}
pub fn SYNC_NEW() -> &'static str {
    tr!("Nova sincronização", "New sync", "Nueva sincronización")
}
pub fn SYNC_STALE_TITLE() -> &'static str {
    tr!(
        "A prévia ficou velha: nada foi executado",
        "The preview is outdated: nothing was run",
        "La vista previa quedó obsoleta: no se ejecutó nada"
    )
}
pub fn sync_stale_body(n: usize) -> String {
    tr!(
        format!(
            "A origem ou o destino mudou depois da prévia ({n} item(ns)). Por segurança o plano antigo nunca é executado: gere uma nova prévia e confira de novo."
        ),
        format!(
            "The source or destination changed after the preview ({n} item(s)). For safety the old plan is never run: generate a new preview and check again."
        ),
        format!(
            "El origen o el destino cambió después de la vista previa ({n} elemento(s)). Por seguridad el plan antiguo nunca se ejecuta: genera una nueva vista previa y revisa de nuevo."
        )
    )
}
pub fn SYNC_SIDE_SOURCE() -> &'static str {
    tr!(
        "mudou na origem",
        "changed at source",
        "cambió en el origen"
    )
}
pub fn SYNC_SIDE_DEST() -> &'static str {
    tr!(
        "mudou no destino",
        "changed at destination",
        "cambió en el destino"
    )
}
pub fn SYNC_NEW_PREVIEW() -> &'static str {
    tr!(
        "Gerar nova prévia",
        "Generate new preview",
        "Generar nueva vista previa"
    )
}
pub fn SYNC_FAILED_TITLE() -> &'static str {
    tr!(
        "A sincronização não pôde continuar",
        "The sync could not continue",
        "La sincronización no pudo continuar"
    )
}
pub fn sync_active_warning(n: usize) -> String {
    if n == 1 {
        tr!(
            "Há uma sincronização em andamento. Se continuar, ela será cancelada; os itens pendentes ficam registrados para retomar depois.".to_string(),
            "There is a sync in progress. If you continue, it will be cancelled; pending items are saved to resume later.".to_string(),
            "Hay una sincronización en curso. Si continúas, se cancelará; los elementos pendientes quedan registrados para reanudar después.".to_string()
        )
    } else {
        tr!(
            format!(
                "Há {n} sincronizações em andamento. Se continuar, serão canceladas; os itens pendentes ficam registrados para retomar depois."
            ),
            format!(
                "There are {n} syncs in progress. If you continue, they will be cancelled; pending items are saved to resume later."
            ),
            format!(
                "Hay {n} sincronizaciones en curso. Si continúas, se cancelarán; los elementos pendientes quedan registrados para reanudar después."
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_i18n::{Lang, with_lang};

    #[test]
    fn textos_de_arquivos_e_sync_por_idioma() {
        assert_eq!(super::super::files::FILES_CLOSE(), "Fechar painel");
        with_lang(Lang::En, || {
            assert_eq!(super::super::files::FILES_CLOSE(), "Close pane");
            assert_eq!(FILES_SYNC(), "Sync…");
            assert_eq!(
                super::super::files::files_delete_title(true, 2),
                "Delete 2 items (remote)?"
            );
            assert_eq!(super::super::m2b::import_button(1), "Import 1 profile");
        });
        with_lang(Lang::Es, || {
            assert_eq!(SYNC_TITLE(), "Sincronizar carpetas");
            assert_eq!(
                super::super::m2b::SPLIT_LIMIT(),
                "Se alcanzó el límite de paneles por pestaña."
            );
        });
    }
}
