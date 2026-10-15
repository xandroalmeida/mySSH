//! Lógica pura da UI de sincronização: textos dos motivos, filtros, decisões e regra do botão.

use std::collections::HashMap;

use myssh_i18n::tr;
use myssh_syncrun::myssh_sync::{
    Action, ConflictReason, EntryKind, Exclusions, Plan, PlanItem, RelPath, SkipReason,
    UpdateReason,
};
use myssh_syncrun::{ConflictChoice, Decisions};

pub fn update_reason(r: &UpdateReason) -> String {
    match r {
        UpdateReason::SourceNewer => tr!(
            "A origem é mais nova que o destino.",
            "The source is newer than the destination.",
            "El origen es más nuevo que el destino."
        )
        .into(),
        UpdateReason::SizeDiffers => tr!(
            "O tamanho é diferente.",
            "The size is different.",
            "El tamaño es distinto."
        )
        .into(),
        UpdateReason::ContentDiffers => tr!(
            "O conteúdo é diferente.",
            "The content is different.",
            "El contenido es distinto."
        )
        .into(),
        UpdateReason::RequiresHash => tr!(
            "Mesmo tamanho: só a comparação de conteúdo pode decidir.",
            "Same size: only a content comparison can decide.",
            "Mismo tamaño: solo la comparación de contenido puede decidir."
        )
        .into(),
    }
}

pub fn conflict_reason(r: &ConflictReason) -> String {
    match r {
        ConflictReason::TypeMismatch => tr!(
            "Tipos diferentes: um lado é arquivo e o outro é pasta.",
            "Different types: one side is a file and the other is a folder.",
            "Tipos distintos: un lado es un archivo y el otro es una carpeta."
        )
        .into(),
        ConflictReason::DestModifiedAfter => tr!(
            "O destino foi modificado depois da origem e tem outro tamanho.",
            "The destination was modified after the source and has a different size.",
            "El destino se modificó después del origen y tiene otro tamaño."
        )
        .into(),
        ConflictReason::DestNewer => tr!(
            "O destino é mais novo que a origem, com o mesmo tamanho: não dá para saber qual vale.",
            "The destination is newer than the source, with the same size: it is not possible to tell which one counts.",
            "El destino es más nuevo que el origen, con el mismo tamaño: no se puede saber cuál vale."
        )
        .into(),
        ConflictReason::MtimeUnknown => tr!(
            "Um dos lados não informa a data de modificação.",
            "One of the sides does not report the modification date.",
            "Uno de los lados no informa la fecha de modificación."
        )
        .into(),
        ConflictReason::SourceCollision { other } => tr!(
            format!("Dois nomes da origem colidem no destino (este e «{other}»)."),
            format!("Two source names collide at the destination (this one and «{other}»)."),
            format!("Dos nombres del origen colisionan en el destino (este y «{other}»).")
        ),
        ConflictReason::DestCollision { existing } => tr!(
            format!("O nome colide com «{existing}», que já existe no destino."),
            format!("The name collides with «{existing}», which already exists at the destination."),
            format!("El nombre colisiona con «{existing}», que ya existe en el destino.")
        ),
        ConflictReason::DestSpecial => tr!(
            "O destino é um link ou arquivo especial.",
            "The destination is a link or a special file.",
            "El destino es un enlace o un archivo especial."
        )
        .into(),
    }
}

pub fn skip_reason(r: &SkipReason) -> String {
    match r {
        SkipReason::Identical => tr!("Já está igual.", "Already identical.", "Ya es igual.").into(),
        SkipReason::Symlink => tr!(
            "Link simbólico (nunca é seguido).",
            "Symbolic link (never followed).",
            "Enlace simbólico (nunca se sigue)."
        )
        .into(),
        SkipReason::SpecialFile => tr!(
            "Arquivo especial (ignorado).",
            "Special file (ignored).",
            "Archivo especial (ignorado)."
        )
        .into(),
        SkipReason::InvalidName { name } => tr!(
            format!("Nome inválido: «{name}»."),
            format!("Invalid name: «{name}»."),
            format!("Nombre no válido: «{name}».")
        ),
        SkipReason::WindowsInvalidName { name } => tr!(
            format!("Nome inválido ou reservado no Windows: «{name}»."),
            format!("Name invalid or reserved on Windows: «{name}»."),
            format!("Nombre no válido o reservado en Windows: «{name}».")
        ),
        SkipReason::MaxDepth => tr!(
            "Profundidade máxima excedida.",
            "Maximum depth exceeded.",
            "Profundidad máxima excedida."
        )
        .into(),
        SkipReason::ContainsExcluded => tr!(
            "A pasta do destino contém itens excluídos por padrão e não pode ser apagada.",
            "The destination folder contains items excluded by default and cannot be deleted.",
            "La carpeta de destino contiene elementos excluidos por defecto y no se puede borrar."
        )
        .into(),
    }
}

/// Rótulo curto da ação.
pub fn action_label(a: &Action) -> &'static str {
    match a {
        Action::Create => tr!("Criar", "Create", "Crear"),
        Action::Update { .. } => tr!("Atualizar", "Update", "Actualizar"),
        Action::Delete => tr!("Excluir", "Delete", "Eliminar"),
        Action::Conflict { .. } => tr!("Conflito", "Conflict", "Conflicto"),
        Action::Skip { .. } => tr!("Pular", "Skip", "Omitir"),
    }
}

/// Motivo legível (no idioma ativo) de qualquer ação.
pub fn reason_text(a: &Action) -> String {
    match a {
        Action::Create => tr!(
            "Não existe no destino.",
            "It does not exist at the destination.",
            "No existe en el destino."
        )
        .into(),
        Action::Update { reason } => update_reason(reason),
        Action::Delete => tr!(
            "Existe só no destino (modo espelho).",
            "It exists only at the destination (mirror mode).",
            "Existe solo en el destino (modo espejo)."
        )
        .into(),
        Action::Conflict { reason } => conflict_reason(reason),
        Action::Skip { reason } => skip_reason(reason),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    All,
    Create,
    Update,
    Conflict,
    Delete,
    Skip,
}

impl Filter {
    pub const ALL: [Filter; 6] = [
        Filter::All,
        Filter::Create,
        Filter::Update,
        Filter::Conflict,
        Filter::Delete,
        Filter::Skip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => tr!("Todos", "All", "Todos"),
            Filter::Create => tr!("Criar", "Create", "Crear"),
            Filter::Update => tr!("Atualizar", "Update", "Actualizar"),
            Filter::Conflict => tr!("Conflitos", "Conflicts", "Conflictos"),
            Filter::Delete => tr!("Excluir", "Delete", "Eliminar"),
            Filter::Skip => tr!("Pular", "Skip", "Omitir"),
        }
    }

    pub fn matches(self, a: &Action) -> bool {
        match self {
            Filter::All => true,
            Filter::Create => matches!(a, Action::Create),
            Filter::Update => matches!(a, Action::Update { .. }),
            Filter::Conflict => matches!(a, Action::Conflict { .. }),
            Filter::Delete => matches!(a, Action::Delete),
            Filter::Skip => matches!(a, Action::Skip { .. }),
        }
    }
}

/// Índices dos itens que passam no filtro e no texto de busca (caminho, sem diferenciar caixa).
pub fn filter_items(plan: &Plan, f: Filter, query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    plan.items
        .iter()
        .enumerate()
        .filter(|(_, i)| f.matches(&i.action))
        .filter(|(_, i)| q.is_empty() || i.path.as_str().to_lowercase().contains(&q))
        .map(|(n, _)| n)
        .collect()
}

pub fn count(plan: &Plan, f: Filter) -> usize {
    plan.items.iter().filter(|i| f.matches(&i.action)).count()
}

/// O conflito aceita "Usar origem"? Só arquivos com divergência de data/tamanho; colisões de
/// nome e divergência de tipo nunca são resolvidas automaticamente.
pub fn conflict_resolvable(item: &PlanItem) -> bool {
    item.kind == EntryKind::File
        && matches!(
            item.action,
            Action::Conflict {
                reason: ConflictReason::DestModifiedAfter
                    | ConflictReason::DestNewer
                    | ConflictReason::MtimeUnknown
            }
        )
}

/// Monta as `Decisions`: só entram escolhas `UseSource` de conflitos resolvíveis do plano; os
/// deletes só são autorizados num plano espelho, com itens a apagar e o checkbox marcado.
pub fn build_decisions(
    plan: &Plan,
    choices: &HashMap<RelPath, ConflictChoice>,
    confirm_deletes: bool,
) -> Decisions {
    let conflicts = plan
        .items
        .iter()
        .filter(|i| conflict_resolvable(i))
        .filter(|i| choices.get(&i.path) == Some(&ConflictChoice::UseSource))
        .map(|i| (i.path.clone(), ConflictChoice::UseSource))
        .collect();
    Decisions {
        confirm_deletes: confirm_deletes && plan.mirror_deletes && delete_count(plan) > 0,
        conflicts,
    }
}

pub fn delete_count(plan: &Plan) -> usize {
    plan.items
        .iter()
        .filter(|i| matches!(i.action, Action::Delete))
        .count()
}

/// Itens que a execução vai tocar com as decisões dadas (criar + atualizar + conflitos
/// resolvidos + deletes autorizados).
pub fn executable_count(plan: &Plan, d: &Decisions) -> usize {
    plan.items
        .iter()
        .filter(|i| match i.action {
            Action::Create | Action::Update { .. } => true,
            Action::Delete => d.confirm_deletes,
            Action::Conflict { .. } => d.conflicts.contains_key(&i.path),
            Action::Skip { .. } => false,
        })
        .count()
}

/// O botão "Executar" pode ser usado? Com exclusões no plano exige o checkbox; sem nada a fazer
/// fica desabilitado.
pub fn can_execute(plan: &Plan, confirm_deletes: bool, d: &Decisions) -> bool {
    let n_del = delete_count(plan);
    if n_del > 0 && !confirm_deletes {
        return false;
    }
    executable_count(plan, d) > 0
}

pub fn execute_label(plan: &Plan, confirm_deletes: bool) -> String {
    let n = delete_count(plan);
    if n > 0 && confirm_deletes {
        tr!(
            format!("Executar e excluir {n}"),
            format!("Run and delete {n}"),
            format!("Ejecutar y eliminar {n}")
        )
    } else {
        tr!("Executar", "Run", "Ejecutar").into()
    }
}

pub fn confirm_deletes_label(n: usize) -> String {
    tr!(
        format!("Confirmo excluir {n} itens no destino"),
        format!("I confirm deleting {n} items at the destination"),
        format!("Confirmo eliminar {n} elementos en el destino")
    )
}

/// Linhas do campo de exclusões → `Exclusions` (vazias e comentários `#` são ignorados).
pub fn parse_exclusions(text: &str) -> Result<Exclusions, String> {
    let mut e = Exclusions::none();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        e.add(l).map_err(|err| {
            tr!(
                format!("Padrão inválido «{l}»: {err}"),
                format!("Invalid pattern «{l}»: {err}"),
                format!("Patrón no válido «{l}»: {err}")
            )
        })?;
    }
    Ok(e)
}

pub fn exclusion_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_syncrun::myssh_sync::{Direction, Snapshot};

    fn item(p: &str, kind: EntryKind, a: Action) -> PlanItem {
        PlanItem {
            path: RelPath::parse(p).unwrap(),
            kind,
            action: a,
            src: Some(Snapshot {
                kind,
                size: 10,
                mtime: None,
            }),
            dst: None,
        }
    }

    fn conflict(p: &str, r: ConflictReason) -> PlanItem {
        item(p, EntryKind::File, Action::Conflict { reason: r })
    }

    fn plan(mirror: bool, items: Vec<PlanItem>) -> Plan {
        Plan {
            direction: Direction::LocalToRemote,
            mirror_deletes: mirror,
            items,
        }
    }

    #[test]
    fn todo_motivo_tem_texto_em_portugues() {
        let all_updates = [
            UpdateReason::SourceNewer,
            UpdateReason::SizeDiffers,
            UpdateReason::ContentDiffers,
            UpdateReason::RequiresHash,
        ];
        for r in &all_updates {
            assert!(update_reason(r).ends_with('.'), "{r:?}");
        }
        let conflicts = [
            ConflictReason::TypeMismatch,
            ConflictReason::DestModifiedAfter,
            ConflictReason::DestNewer,
            ConflictReason::MtimeUnknown,
            ConflictReason::SourceCollision { other: "A".into() },
            ConflictReason::DestCollision {
                existing: "b".into(),
            },
            ConflictReason::DestSpecial,
        ];
        for r in &conflicts {
            let t = conflict_reason(r);
            assert!(t.ends_with('.') && !t.contains("::"), "{t}");
        }
        assert!(conflict_reason(&conflicts[4]).contains("«A»"));
        assert!(conflict_reason(&conflicts[5]).contains("«b»"));
        let skips = [
            SkipReason::Identical,
            SkipReason::Symlink,
            SkipReason::SpecialFile,
            SkipReason::InvalidName { name: "x".into() },
            SkipReason::WindowsInvalidName { name: "con".into() },
            SkipReason::MaxDepth,
            SkipReason::ContainsExcluded,
        ];
        for r in &skips {
            assert!(skip_reason(r).ends_with('.'), "{r:?}");
        }
        assert!(skip_reason(&skips[4]).contains("«con»"));
        assert_eq!(reason_text(&Action::Create), "Não existe no destino.");
        assert!(reason_text(&Action::Delete).contains("espelho"));
        assert_eq!(action_label(&Action::Delete), "Excluir");
    }

    #[test]
    fn textos_em_ingles_e_espanhol() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(
                reason_text(&Action::Create),
                "It does not exist at the destination."
            );
            assert_eq!(action_label(&Action::Delete), "Delete");
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(action_label(&Action::Delete), "Eliminar");
        });
    }

    #[test]
    fn filtros_e_busca() {
        let p = plan(
            true,
            vec![
                item("a.txt", EntryKind::File, Action::Create),
                item(
                    "Docs/b.txt",
                    EntryKind::File,
                    Action::Update {
                        reason: UpdateReason::SizeDiffers,
                    },
                ),
                conflict("c.txt", ConflictReason::DestNewer),
                item("velho.txt", EntryKind::File, Action::Delete),
                item(
                    "igual.txt",
                    EntryKind::File,
                    Action::Skip {
                        reason: SkipReason::Identical,
                    },
                ),
            ],
        );
        assert_eq!(filter_items(&p, Filter::All, ""), vec![0, 1, 2, 3, 4]);
        assert_eq!(filter_items(&p, Filter::Create, ""), vec![0]);
        assert_eq!(filter_items(&p, Filter::Conflict, ""), vec![2]);
        assert_eq!(filter_items(&p, Filter::Delete, ""), vec![3]);
        assert_eq!(filter_items(&p, Filter::All, "DOCS"), vec![1]);
        assert_eq!(filter_items(&p, Filter::Skip, "a.txt"), Vec::<usize>::new());
        assert_eq!(count(&p, Filter::Update), 1);
    }

    #[test]
    fn decisoes_so_incluem_conflitos_resolviveis_marcados() {
        let p = plan(
            false,
            vec![
                conflict("a", ConflictReason::DestNewer),
                conflict("b", ConflictReason::DestModifiedAfter),
                conflict("c", ConflictReason::TypeMismatch),
                conflict(
                    "d",
                    ConflictReason::DestCollision {
                        existing: "D".into(),
                    },
                ),
                item(
                    "e",
                    EntryKind::Dir,
                    Action::Conflict {
                        reason: ConflictReason::DestNewer,
                    },
                ),
            ],
        );
        let mut ch = HashMap::new();
        for n in ["a", "c", "d", "e", "fantasma"] {
            ch.insert(RelPath::parse(n).unwrap(), ConflictChoice::UseSource);
        }
        ch.insert(RelPath::parse("b").unwrap(), ConflictChoice::Skip);
        let d = build_decisions(&p, &ch, true);
        let keys: Vec<_> = d.conflicts.keys().map(|k| k.to_string()).collect();
        assert_eq!(keys, vec!["a"]);
        // plano sem espelho nunca autoriza deletes, mesmo com o checkbox
        assert!(!d.confirm_deletes);
    }

    #[test]
    fn exclusao_exige_checkbox_e_o_rotulo_traz_o_numero() {
        let p = plan(
            true,
            vec![
                item("novo", EntryKind::File, Action::Create),
                item("x", EntryKind::File, Action::Delete),
                item("y", EntryKind::Dir, Action::Delete),
                item("z", EntryKind::File, Action::Delete),
            ],
        );
        let none = HashMap::new();
        let d0 = build_decisions(&p, &none, false);
        assert!(!d0.confirm_deletes);
        assert!(!can_execute(&p, false, &d0), "sem marcar não habilita");
        assert_eq!(execute_label(&p, false), "Executar");
        let d1 = build_decisions(&p, &none, true);
        assert!(d1.confirm_deletes);
        assert!(can_execute(&p, true, &d1));
        assert_eq!(execute_label(&p, true), "Executar e excluir 3");
        assert_eq!(executable_count(&p, &d0), 1);
        assert_eq!(executable_count(&p, &d1), 4);
        assert_eq!(
            confirm_deletes_label(3),
            "Confirmo excluir 3 itens no destino"
        );
    }

    #[test]
    fn sem_nada_a_fazer_o_botao_fica_desabilitado() {
        let p = plan(
            false,
            vec![
                item(
                    "a",
                    EntryKind::File,
                    Action::Skip {
                        reason: SkipReason::Identical,
                    },
                ),
                conflict("b", ConflictReason::DestNewer),
            ],
        );
        let d = build_decisions(&p, &HashMap::new(), false);
        assert!(!can_execute(&p, false, &d));
        let mut ch = HashMap::new();
        ch.insert(RelPath::parse("b").unwrap(), ConflictChoice::UseSource);
        let d = build_decisions(&p, &ch, false);
        assert!(can_execute(&p, false, &d));
    }

    #[test]
    fn exclusoes_do_usuario() {
        let e = parse_exclusions(".git/\n# comentário\n\n  *.log  \nnode_modules/\n").unwrap();
        let _ = e;
        assert_eq!(
            exclusion_patterns(".git/\n# c\n\n*.log\n"),
            vec![".git/", "*.log"]
        );
        assert!(parse_exclusions("[abc").is_err());
    }
}
