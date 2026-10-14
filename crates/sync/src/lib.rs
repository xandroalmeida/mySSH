//! Núcleo de planejamento de sincronização unidirecional do mySSH (sem rede).

pub mod exclude;
pub mod journal;
pub mod plan;
pub mod relpath;
pub mod tree;

pub use exclude::Exclusions;
pub use journal::{Journal, JournalItem, Op, Status};
pub use plan::{
    Action, Changed, CompareMode, ConflictReason, ContentHasher, Direction, Plan, PlanError,
    PlanItem, PlanOptions, Side, SkipReason, Snapshot, Summary, UpdateReason,
    is_invalid_windows_name, plan,
};
pub use relpath::{PathError, RelPath};
pub use tree::{Entry, EntryKind, FileTree, LocalTree, MemTree, TreeError};

#[cfg(test)]
mod tests;
