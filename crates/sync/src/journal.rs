//! Journal de execução, serializável em JSON, para retomada.
//!
//! Contém apenas caminhos relativos, tipo, operação e status. Nunca guarda
//! segredos, caminhos absolutos nem hosts.

use crate::plan::{Action, Plan};
use crate::relpath::RelPath;
use crate::tree::EntryKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Done,
    Failed(String),
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalItem {
    pub path: RelPath,
    pub kind: EntryKind,
    pub op: Op,
    pub status: Status,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Journal {
    pub items: Vec<JournalItem>,
    #[serde(skip)]
    index: HashMap<(RelPath, Op), usize>,
}

impl Journal {
    /// Cria o journal com os itens executáveis do plano (Create/Update/Delete)
    /// como `Pending`, na ordem do plano. Conflitos e `Skip` ficam de fora.
    pub fn from_plan(plan: &Plan) -> Journal {
        let items = plan
            .items
            .iter()
            .filter_map(|it| {
                let op = match it.action {
                    Action::Create => Op::Create,
                    Action::Update { .. } => Op::Update,
                    Action::Delete => Op::Delete,
                    _ => return None,
                };
                Some(JournalItem {
                    path: it.path.clone(),
                    kind: it.kind,
                    op,
                    status: Status::Pending,
                })
            })
            .collect();
        let mut j = Journal {
            items,
            index: HashMap::new(),
        };
        j.reindex();
        j
    }

    fn reindex(&mut self) {
        self.index = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| ((it.path.clone(), it.op), i))
            .collect();
    }

    /// Atualiza o status; devolve `false` se o item não existe.
    pub fn mark(&mut self, path: &RelPath, op: Op, status: Status) -> bool {
        match self.index.get(&(path.clone(), op)) {
            Some(&i) => {
                self.items[i].status = status;
                true
            }
            None => false,
        }
    }

    pub fn status(&self, path: &RelPath, op: Op) -> Option<&Status> {
        self.index
            .get(&(path.clone(), op))
            .map(|&i| &self.items[i].status)
    }

    /// Itens a (re)executar: pendentes e falhos, na ordem original.
    pub fn remaining(&self) -> impl Iterator<Item = &JournalItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.status, Status::Pending | Status::Failed(_)))
    }

    pub fn is_complete(&self) -> bool {
        self.items
            .iter()
            .all(|i| matches!(i.status, Status::Done | Status::Skipped))
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Journal, serde_json::Error> {
        let mut j: Journal = serde_json::from_str(s)?;
        j.reindex();
        Ok(j)
    }
}
