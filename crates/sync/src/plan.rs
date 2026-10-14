//! Planejamento (dry-run) de sincronização unidirecional.
//!
//! # Limitações documentadas
//!
//! * **Tamanho/mtime** não prova igualdade: arquivos de mesmo tamanho e mtime
//!   dentro da tolerância são considerados iguais mesmo com conteúdo diferente.
//!   Use [`CompareMode::Content`] quando isso importar (custa leitura completa).
//! * **Tolerância de mtime** = `max(tolerance configurada (2 s por padrão,
//!   granularidade do FAT), resolução da origem, resolução do destino)`.
//!   Fusos/relógios dessincronizados entre máquinas não são corrigidos.
//! * Sem mtime em um dos lados não há como saber quem é mais novo: vira
//!   `Conflict` (use comparação por conteúdo).
//! * Symlinks e arquivos especiais **nunca** são seguidos nem copiados; são
//!   reportados como `Skip`. Só `follow_symlinks = false` é suportado.
//! * Diretórios já existentes nos dois lados não geram item; um diretório raiz
//!   de destino inexistente é tratado como vazio (o executor deve criá-lo).
//! * Permissões não participam da decisão (apenas `Entry::perms` é exposto).
//! * O plano é um retrato: alterações concorrentes são detectadas por
//!   [`Plan::revalidate`], não evitadas.
//! * Nomes que colidem por caixa/normalização Unicode viram `Conflict` quando
//!   o destino é marcado como case-insensitive ou normalizador (macOS); em
//!   destino case-sensitive os nomes são distintos e não há detecção.

use crate::exclude::Exclusions;
use crate::relpath::RelPath;
use crate::tree::{Entry, EntryKind, FileTree, TreeError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::time::{Duration, SystemTime};
use unicode_normalization::UnicodeNormalization;

// ------------------------------------------------------------------ opções

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    LocalToRemote,
    RemoteToLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    /// Tamanho + mtime com tolerância mínima configurável (padrão 2 s).
    SizeMtime { tolerance: Duration },
    /// Conteúdo: arquivos de mesmo tamanho ficam "requer hash" (hash vem depois).
    Content,
}

#[derive(Debug)]
pub struct PlanOptions {
    pub direction: Direction,
    pub compare: CompareMode,
    pub mirror_deletes: bool,
    /// Apenas `false` é suportado; `true` retorna erro.
    pub follow_symlinks: bool,
    pub case_insensitive_dest: bool,
    /// Destino normaliza Unicode (macOS/APFS/HFS+): NFC e NFD colidem.
    pub unicode_normalizing_dest: bool,
    /// Destino é Windows: nomes inválidos/reservados viram `Skip`.
    pub dest_windows: bool,
    pub exclusions: Exclusions,
    pub max_depth: usize,
    pub max_entries: usize,
}

impl PlanOptions {
    pub fn new(direction: Direction) -> Self {
        PlanOptions {
            direction,
            compare: CompareMode::SizeMtime {
                tolerance: Duration::from_secs(2),
            },
            mirror_deletes: false,
            follow_symlinks: false,
            case_insensitive_dest: false,
            unicode_normalizing_dest: false,
            dest_windows: false,
            exclusions: Exclusions::default(),
            max_depth: 64,
            max_entries: 1_000_000,
        }
    }

    fn needs_key(&self) -> bool {
        self.case_insensitive_dest || self.unicode_normalizing_dest
    }

    fn key(&self, name: &str) -> String {
        let n: String = name.nfc().collect();
        if self.case_insensitive_dest {
            n.to_lowercase()
        } else {
            n
        }
    }
}

// ------------------------------------------------------------------ plano

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub kind: EntryKind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

impl From<&Entry> for Snapshot {
    fn from(e: &Entry) -> Self {
        Snapshot {
            kind: e.kind,
            size: e.size,
            mtime: e.mtime,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateReason {
    SourceNewer,
    SizeDiffers,
    ContentDiffers,
    /// Mesmo tamanho; só o hash (ainda não calculado) decide.
    RequiresHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictReason {
    TypeMismatch,
    /// Destino mais novo que a origem com tamanho diferente.
    DestModifiedAfter,
    MtimeUnknown,
    /// Dois nomes da origem colidem no destino.
    SourceCollision {
        other: String,
    },
    /// Nome da origem colide com outro já existente no destino.
    DestCollision {
        existing: String,
    },
    /// Destino é symlink/arquivo especial.
    DestSpecial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkipReason {
    Identical,
    Symlink,
    SpecialFile,
    InvalidName {
        name: String,
    },
    WindowsInvalidName {
        name: String,
    },
    MaxDepth,
    /// Destino mais novo, mesmo tamanho: não sobrescrever.
    DestNewerSameSize,
    /// Diretório do destino contém itens excluídos; não pode ser apagado.
    ContainsExcluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Create,
    Update {
        reason: UpdateReason,
    },
    /// Só aparece com `mirror_deletes`.
    Delete,
    Conflict {
        reason: ConflictReason,
    },
    Skip {
        reason: SkipReason,
    },
}

impl Action {
    pub fn needs_hash(&self) -> bool {
        matches!(
            self,
            Action::Update {
                reason: UpdateReason::RequiresHash
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    /// Caminho do item (para nomes inválidos, o diretório pai).
    pub path: RelPath,
    pub kind: EntryKind,
    pub action: Action,
    pub src: Option<Snapshot>,
    pub dst: Option<Snapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub create: usize,
    pub update: usize,
    pub delete: usize,
    pub conflict: usize,
    pub skip: usize,
    pub needs_hash: usize,
    /// Bytes a transferir (Create/Update de arquivos).
    pub bytes_to_transfer: u64,
    /// Bytes liberados por Delete de arquivos.
    pub bytes_to_delete: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Source,
    Dest,
}

/// Mudança detectada por [`Plan::revalidate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub path: RelPath,
    pub side: Side,
    pub expected: Option<Snapshot>,
    pub actual: Option<Snapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub direction: Direction,
    pub mirror_deletes: bool,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("seguir symlinks não é suportado")]
    FollowSymlinksUnsupported,
    #[error("raiz da origem não existe")]
    SourceMissing,
    #[error("raiz da origem não é um diretório")]
    SourceNotDir,
    #[error("raiz do destino não é um diretório")]
    DestNotDir,
    #[error("limite de {0} entradas excedido")]
    TooManyEntries(usize),
    #[error(transparent)]
    Tree(#[from] TreeError),
}

/// Calcula hash de conteúdo (implementado depois, com SFTP/disco).
pub trait ContentHasher: Sync {
    fn hash(&self, path: &RelPath) -> impl Future<Output = Result<Vec<u8>, TreeError>> + Send;
}

impl Plan {
    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for it in &self.items {
            let is_file = it.kind == EntryKind::File;
            match &it.action {
                Action::Create => {
                    s.create += 1;
                    if is_file {
                        s.bytes_to_transfer += it.src.map_or(0, |x| x.size);
                    }
                }
                Action::Update { .. } => {
                    s.update += 1;
                    if is_file {
                        s.bytes_to_transfer += it.src.map_or(0, |x| x.size);
                    }
                    if it.action.needs_hash() {
                        s.needs_hash += 1;
                    }
                }
                Action::Delete => {
                    s.delete += 1;
                    if is_file {
                        s.bytes_to_delete += it.dst.map_or(0, |x| x.size);
                    }
                }
                Action::Conflict { .. } => s.conflict += 1,
                Action::Skip { .. } => s.skip += 1,
            }
        }
        s
    }

    /// Reconsulta origem e destino e devolve o que mudou desde o plano.
    /// Itens `Skip` não são revalidados. Chamar antes de operações
    /// destrutivas; se houver mudanças, gerar novo plano.
    pub async fn revalidate<S: FileTree, D: FileTree>(
        &self,
        source: &S,
        dest: &D,
    ) -> Result<Vec<Changed>, TreeError> {
        let mut out = Vec::new();
        for it in &self.items {
            if matches!(it.action, Action::Skip { .. }) {
                continue;
            }
            // Itens com nome inválido não são alcançáveis; já são Skip.
            if it.src.is_some() || matches!(it.action, Action::Create | Action::Update { .. }) {
                let now = source.stat(&it.path).await?.as_ref().map(Snapshot::from);
                if now != it.src {
                    out.push(Changed {
                        path: it.path.clone(),
                        side: Side::Source,
                        expected: it.src,
                        actual: now,
                    });
                }
            }
            let now = dest.stat(&it.path).await?.as_ref().map(Snapshot::from);
            if now != it.dst {
                out.push(Changed {
                    path: it.path.clone(),
                    side: Side::Dest,
                    expected: it.dst,
                    actual: now,
                });
            }
        }
        Ok(out)
    }

    /// Resolve itens `RequiresHash` com os hashers dos dois lados: iguais viram
    /// `Skip(Identical)`, diferentes `Update(ContentDiffers)`. Devolve quantos
    /// itens foram resolvidos.
    pub async fn resolve_hashes<SH: ContentHasher, DH: ContentHasher>(
        &mut self,
        src: &SH,
        dst: &DH,
    ) -> Result<usize, TreeError> {
        let mut n = 0;
        for it in &mut self.items {
            if !it.action.needs_hash() {
                continue;
            }
            let (a, b) = (src.hash(&it.path).await?, dst.hash(&it.path).await?);
            it.action = if a == b {
                Action::Skip {
                    reason: SkipReason::Identical,
                }
            } else {
                Action::Update {
                    reason: UpdateReason::ContentDiffers,
                }
            };
            n += 1;
        }
        Ok(n)
    }
}

// --------------------------------------------------------- nomes Windows

const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// Nome inválido em Windows: `<>:"|?*`, controles, reservados (CON, NUL,
/// COM1-9, LPT1-9, com ou sem extensão) e final com ponto ou espaço.
pub fn is_invalid_windows_name(name: &str) -> bool {
    if name
        .chars()
        .any(|c| "<>:\"|?*".contains(c) || (c as u32) < 32)
    {
        return true;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return true;
    }
    let stem = name.split('.').next().unwrap_or("").trim_end();
    let up = stem.to_ascii_uppercase();
    if RESERVED.contains(&up.as_str()) {
        return true;
    }
    for pre in ["COM", "LPT"] {
        if let Some(d) = up.strip_prefix(pre)
            && d.len() == 1
            && matches!(d.as_bytes()[0], b'1'..=b'9')
        {
            return true;
        }
    }
    false
}

// ------------------------------------------------------------- planejador

struct Builder<'a> {
    opts: &'a PlanOptions,
    items: Vec<PlanItem>,
    seen: usize,
    visited: HashSet<RelPath>,
}

impl Builder<'_> {
    fn count(&mut self, n: usize) -> Result<(), PlanError> {
        self.seen += n;
        if self.seen > self.opts.max_entries {
            return Err(PlanError::TooManyEntries(self.opts.max_entries));
        }
        Ok(())
    }

    fn push(
        &mut self,
        path: RelPath,
        kind: EntryKind,
        action: Action,
        src: Option<&Entry>,
        dst: Option<&Entry>,
    ) {
        self.items.push(PlanItem {
            path,
            kind,
            action,
            src: src.map(Snapshot::from),
            dst: dst.map(Snapshot::from),
        });
    }

    fn skip(&mut self, path: RelPath, kind: EntryKind, reason: SkipReason, s: Option<&Entry>) {
        self.push(path, kind, Action::Skip { reason }, s, None);
    }
}

fn compare_files(s: &Entry, d: &Entry, mode: CompareMode) -> Action {
    match mode {
        CompareMode::Content => {
            if s.size != d.size {
                Action::Update {
                    reason: UpdateReason::SizeDiffers,
                }
            } else {
                Action::Update {
                    reason: UpdateReason::RequiresHash,
                }
            }
        }
        CompareMode::SizeMtime { tolerance } => {
            let (Some(sm), Some(dm)) = (s.mtime, d.mtime) else {
                return Action::Conflict {
                    reason: ConflictReason::MtimeUnknown,
                };
            };
            let tol = tolerance.max(s.mtime_resolution).max(d.mtime_resolution);
            if let Ok(diff) = dm.duration_since(sm)
                && diff > tol
            {
                // Destino mais novo que a origem.
                return if s.size != d.size {
                    Action::Conflict {
                        reason: ConflictReason::DestModifiedAfter,
                    }
                } else {
                    Action::Skip {
                        reason: SkipReason::DestNewerSameSize,
                    }
                };
            }
            if let Ok(diff) = sm.duration_since(dm)
                && diff > tol
            {
                return Action::Update {
                    reason: UpdateReason::SourceNewer,
                };
            }
            if s.size != d.size {
                Action::Update {
                    reason: UpdateReason::SizeDiffers,
                }
            } else {
                Action::Skip {
                    reason: SkipReason::Identical,
                }
            }
        }
    }
}

/// Gera o plano para sincronizar `source` → `dest`.
pub async fn plan<S: FileTree, D: FileTree>(
    source: &S,
    dest: &D,
    opts: &PlanOptions,
) -> Result<Plan, PlanError> {
    if opts.follow_symlinks {
        return Err(PlanError::FollowSymlinksUnsupported);
    }
    let root = RelPath::root();
    match source.stat(&root).await? {
        None => return Err(PlanError::SourceMissing),
        Some(e) if e.kind != EntryKind::Dir => return Err(PlanError::SourceNotDir),
        Some(_) => {}
    }
    let dest_root_exists = match dest.stat(&root).await? {
        None => false,
        Some(e) if e.kind == EntryKind::Dir => true,
        Some(_) => return Err(PlanError::DestNotDir),
    };

    let mut b = Builder {
        opts,
        items: Vec::new(),
        seen: 0,
        visited: HashSet::new(),
    };
    let mut stack = vec![(root, dest_root_exists)];

    while let Some((dir, dest_exists)) = stack.pop() {
        if !b.visited.insert(dir.clone()) {
            continue; // proteção contra ciclos
        }
        let mut src = source.list_dir(&dir).await?;
        src.sort_by(|a, b| a.name.cmp(&b.name));
        let mut dst = if dest_exists {
            dest.list_dir(&dir).await?
        } else {
            Vec::new()
        };
        dst.sort_by(|a, b| a.name.cmp(&b.name));
        b.count(src.len() + dst.len())?;

        let dst_by_name: HashMap<&str, &Entry> = dst.iter().map(|e| (e.name.as_str(), e)).collect();
        let src_names: HashSet<&str> = src.iter().map(|e| e.name.as_str()).collect();

        // Candidatos válidos e não excluídos.
        let mut cands: Vec<(RelPath, &Entry)> = Vec::new();
        for e in &src {
            let Ok(path) = dir.join(&e.name) else {
                b.skip(
                    dir.clone(),
                    e.kind,
                    SkipReason::InvalidName {
                        name: e.name.clone(),
                    },
                    None,
                );
                continue;
            };
            if opts.exclusions.is_excluded(&path, e.kind == EntryKind::Dir) {
                continue;
            }
            cands.push((path, e));
        }

        // Colisões por caixa/normalização.
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        let mut dst_keys: HashMap<String, &Entry> = HashMap::new();
        if opts.needs_key() {
            for (i, (_, e)) in cands.iter().enumerate() {
                groups.entry(opts.key(&e.name)).or_default().push(i);
            }
            for d in &dst {
                dst_keys.entry(opts.key(&d.name)).or_insert(d);
            }
        }

        let mut child_dirs = Vec::new();
        for (path, e) in cands.iter() {
            let s = Some(*e);
            if opts.needs_key() {
                let g = &groups[&opts.key(&e.name)];
                if g.len() > 1 {
                    let other = g
                        .iter()
                        .map(|&j| &cands[j].1.name)
                        .find(|n| **n != e.name)
                        .cloned()
                        .unwrap_or_default();
                    b.push(
                        path.clone(),
                        e.kind,
                        Action::Conflict {
                            reason: ConflictReason::SourceCollision { other },
                        },
                        s,
                        dst_by_name.get(e.name.as_str()).copied(),
                    );
                    continue;
                }
            }
            match e.kind {
                EntryKind::Symlink => {
                    b.skip(path.clone(), e.kind, SkipReason::Symlink, s);
                    continue;
                }
                EntryKind::Other => {
                    b.skip(path.clone(), e.kind, SkipReason::SpecialFile, s);
                    continue;
                }
                _ => {}
            }
            if opts.dest_windows && is_invalid_windows_name(&e.name) {
                b.skip(
                    path.clone(),
                    e.kind,
                    SkipReason::WindowsInvalidName {
                        name: e.name.clone(),
                    },
                    s,
                );
                continue;
            }

            let too_deep = e.kind == EntryKind::Dir && path.depth() > opts.max_depth;
            match dst_by_name.get(e.name.as_str()) {
                None => {
                    if opts.needs_key()
                        && let Some(d) = dst_keys.get(&opts.key(&e.name))
                    {
                        b.push(
                            path.clone(),
                            e.kind,
                            Action::Conflict {
                                reason: ConflictReason::DestCollision {
                                    existing: d.name.clone(),
                                },
                            },
                            s,
                            Some(d),
                        );
                        continue;
                    }
                    if too_deep {
                        b.skip(path.clone(), e.kind, SkipReason::MaxDepth, s);
                        continue;
                    }
                    b.push(path.clone(), e.kind, Action::Create, s, None);
                    if e.kind == EntryKind::Dir {
                        child_dirs.push((path.clone(), false));
                    }
                }
                Some(d) => {
                    if matches!(d.kind, EntryKind::Symlink | EntryKind::Other) {
                        b.push(
                            path.clone(),
                            e.kind,
                            Action::Conflict {
                                reason: ConflictReason::DestSpecial,
                            },
                            s,
                            Some(d),
                        );
                    } else if d.kind != e.kind {
                        b.push(
                            path.clone(),
                            e.kind,
                            Action::Conflict {
                                reason: ConflictReason::TypeMismatch,
                            },
                            s,
                            Some(d),
                        );
                    } else if e.kind == EntryKind::Dir {
                        if too_deep {
                            b.skip(path.clone(), e.kind, SkipReason::MaxDepth, s);
                        } else {
                            child_dirs.push((path.clone(), true));
                        }
                    } else {
                        let action = compare_files(e, d, opts.compare);
                        b.push(path.clone(), e.kind, action, s, Some(d));
                    }
                }
            }
        }

        // Modo espelho: itens só no destino.
        if opts.mirror_deletes {
            let cand_keys: HashSet<String> = if opts.needs_key() {
                src.iter().map(|e| opts.key(&e.name)).collect()
            } else {
                HashSet::new()
            };
            for d in &dst {
                if src_names.contains(d.name.as_str())
                    || (opts.needs_key() && cand_keys.contains(&opts.key(&d.name)))
                {
                    continue;
                }
                let Ok(path) = dir.join(&d.name) else {
                    b.skip(
                        dir.clone(),
                        d.kind,
                        SkipReason::InvalidName {
                            name: d.name.clone(),
                        },
                        None,
                    );
                    continue;
                };
                if opts.exclusions.is_excluded(&path, d.kind == EntryKind::Dir) {
                    continue;
                }
                delete_walk(&mut b, dest, path, d).await?;
            }
        }

        for c in child_dirs.into_iter().rev() {
            stack.push(c);
        }
    }

    Ok(Plan {
        direction: opts.direction,
        mirror_deletes: opts.mirror_deletes,
        items: b.items,
    })
}

/// Apaga (planeja) `root` do destino; filhos antes do pai. Não segue symlinks.
async fn delete_walk<D: FileTree>(
    b: &mut Builder<'_>,
    dest: &D,
    root: RelPath,
    root_entry: &Entry,
) -> Result<(), PlanError> {
    let mut order: Vec<(RelPath, Entry)> = vec![(root, root_entry.clone())];
    let mut excluded: Vec<RelPath> = Vec::new();
    let mut i = 0;
    while i < order.len() {
        let (path, e) = order[i].clone();
        i += 1;
        if e.kind != EntryKind::Dir {
            continue;
        }
        if path.depth() > b.opts.max_depth {
            // Não desce; o diretório não pode ser apagado com segurança.
            excluded.push(path.clone());
            continue;
        }
        let mut kids = dest.list_dir(&path).await?;
        kids.sort_by(|a, b| a.name.cmp(&b.name));
        b.count(kids.len())?;
        for k in kids {
            let Ok(kp) = path.join(&k.name) else {
                b.skip(
                    path.clone(),
                    k.kind,
                    SkipReason::InvalidName { name: k.name },
                    None,
                );
                excluded.push(path.clone());
                continue;
            };
            if b.opts.exclusions.is_excluded(&kp, k.kind == EntryKind::Dir) {
                excluded.push(kp);
                continue;
            }
            order.push((kp, k));
        }
    }
    for (path, e) in order.into_iter().rev() {
        let prefix = format!("{}/", path.as_str());
        let blocked = e.kind == EntryKind::Dir
            && excluded
                .iter()
                .any(|x| *x == path || x.as_str().starts_with(&prefix));
        if blocked {
            b.push(
                path,
                e.kind,
                Action::Skip {
                    reason: SkipReason::ContainsExcluded,
                },
                None,
                Some(&e),
            );
        } else {
            b.push(path, e.kind, Action::Delete, None, Some(&e));
        }
    }
    Ok(())
}
