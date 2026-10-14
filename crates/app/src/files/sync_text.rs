//! Lógica pura da UI de sincronização: textos dos motivos, filtros, decisões e regra do botão.

use std::collections::HashMap;

use myssh_syncrun::myssh_sync::{
    Action, ConflictReason, EntryKind, Exclusions, Plan, PlanItem, RelPath, SkipReason,
    UpdateReason,
};
use myssh_syncrun::{ConflictChoice, Decisions};

pub fn update_reason(r: &UpdateReason) -> String {
    match r {
        UpdateReason::SourceNewer => "A origem é mais nova que o destino.".into(),
        UpdateReason::SizeDiffers => "O tamanho é diferente.".into(),
        UpdateReason::ContentDiffers => "O conteúdo é diferente.".into(),
        UpdateReason::RequiresHash => {
            "Mesmo tamanho: só a comparação de conteúdo pode decidir.".into()
        }
    }
}

pub fn conflict_reason(r: &ConflictReason) -> String {
    match r {
        ConflictReason::TypeMismatch => {
            "Tipos diferentes: um lado é arquivo e o outro é pasta.".into()
        }
        ConflictReason::DestModifiedAfter => {
            "O destino foi modificado depois da origem e tem outro tamanho.".into()
        }
        ConflictReason::DestNewer => {
            "O destino é mais novo que a origem, com o mesmo tamanho: não dá para saber qual vale."
                .into()
        }
        ConflictReason::MtimeUnknown => "Um dos lados não informa a data de modificação.".into(),
        ConflictReason::SourceCollision { other } => {
            format!("Dois nomes da origem colidem no destino (este e «{other}»).")
        }
        ConflictReason::DestCollision { existing } => {
            format!("O nome colide com «{existing}», que já existe no destino.")
        }
        ConflictReason::DestSpecial => "O destino é um link ou arquivo especial.".into(),
    }
}

pub fn skip_reason(r: &SkipReason) -> String {
    match r {
        SkipReason::Identical => "Já está igual.".into(),
        SkipReason::Symlink => "Link simbólico (nunca é seguido).".into(),
        SkipReason::SpecialFile => "Arquivo especial (ignorado).".into(),
        SkipReason::InvalidName { name } => format!("Nome inválido: «{name}»."),
        SkipReason::WindowsInvalidName { name } => {
            format!("Nome inválido ou reservado no Windows: «{name}».")
        }
        SkipReason::MaxDepth => "Profundidade máxima excedida.".into(),
        SkipReason::ContainsExcluded => {
            "A pasta do destino contém itens excluídos por padrão e não pode ser apagada.".into()
        }
    }
}

/// Rótulo curto da ação.
pub fn action_label(a: &Action) -> &'static str {
    match a {
        Action::Create => "Criar",
        Action::Update { .. } => "Atualizar",
        Action::Delete => "Excluir",
        Action::Conflict { .. } => "Conflito",
        Action::Skip { .. } => "Pular",
    }
}

/// Motivo legível em pt-BR de qualquer ação.
pub fn reason_text(a: &Action) -> String {
    match a {
        Action::Create => "Não existe no destino.".into(),
        Action::Update { reason } => update_reason(reason),
        Action::Delete => "Existe só no destino (modo espelho).".into(),
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
            Filter::All => "Todos",
            Filter::Create => "Criar",
            Filter::Update => "Atualizar",
            Filter::Conflict => "Conflitos",
            Filter::Delete => "Excluir",
            Filter::Skip => "Pular",
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
        format!("Executar e excluir {n}")
    } else {
        "Executar".into()
    }
}

pub fn confirm_deletes_label(n: usize) -> String {
    format!("Confirmo excluir {n} itens no destino")
}

/// Linhas do campo de exclusões → `Exclusions` (vazias e comentários `#` são ignorados).
pub fn parse_exclusions(text: &str) -> Result<Exclusions, String> {
    let mut e = Exclusions::none();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        e.add(l)
            .map_err(|err| format!("Padrão inválido «{l}»: {err}"))?;
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
