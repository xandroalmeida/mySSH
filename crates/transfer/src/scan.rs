//! Varredura (contagem) antes de transferir: monta a lista de diretórios e arquivos, pulando
//! symlinks e validando nomes.

use crate::error::TransferError;
use crate::manager::JobShared;
use crate::names::{self, LocalOs};
use crate::side::{Meta, Side, meta_from_attrs};
use crate::{Direction, Kind, TransferRequest};
use russh_sftp::client::SftpSession;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
pub enum Item {
    Dir {
        rel: String,
        mtime: Option<SystemTime>,
        perms: Option<u32>,
    },
    File {
        rel: String,
        size: u64,
    },
}

#[derive(Debug, Default)]
pub struct Plan {
    pub src_root: String,
    pub dst_root: String,
    /// Nome do item raiz (para mensagens).
    pub name: String,
    pub items: Vec<Item>,
    pub files: u64,
    pub bytes: u64,
    /// Itens que falharam já na varredura (nome inválido, colisão...).
    pub failed: u64,
    /// Itens pulados na varredura (symlinks, tipos não suportados).
    pub skipped: u64,
}

pub struct ScanOpts {
    pub os: LocalOs,
    pub case_insensitive: bool,
    pub limits: ScanLimits,
}

/// Limites da varredura (servidor hostil pode devolver árvores profundas/enormes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanLimits {
    /// Profundidade máxima de subdiretórios abaixo da raiz.
    pub max_depth: usize,
    /// Total máximo de entradas examinadas.
    pub max_entries: u64,
}

pub const DEFAULT_MAX_DEPTH: usize = 64;
pub const DEFAULT_MAX_ENTRIES: u64 = 1_000_000;

impl Default for ScanLimits {
    fn default() -> Self {
        ScanLimits {
            max_depth: DEFAULT_MAX_DEPTH,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

impl ScanLimits {
    /// `rel` (relativo à raiz) é um diretório que a varredura vai abrir.
    pub fn check_depth(&self, rel: &str) -> Result<(), TransferError> {
        let depth = if rel.is_empty() {
            0
        } else {
            rel.split('/').count()
        };
        if depth > self.max_depth {
            return Err(TransferError::ScanLimit(format!(
                "profundidade máxima de {} níveis excedida",
                self.max_depth
            )));
        }
        Ok(())
    }

    /// `seen` entradas examinadas até agora.
    pub fn check_entries(&self, seen: u64) -> Result<(), TransferError> {
        if seen > self.max_entries {
            return Err(TransferError::ScanLimit(format!(
                "mais de {} entradas",
                self.max_entries
            )));
        }
        Ok(())
    }
}

struct Ticker {
    last: Instant,
}

impl Ticker {
    fn new() -> Self {
        Ticker {
            last: Instant::now(),
        }
    }

    fn due(&mut self) -> bool {
        if self.last.elapsed() >= Duration::from_millis(250) {
            self.last = Instant::now();
            true
        } else {
            false
        }
    }
}

fn child_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}/{name}")
    }
}

fn disp(plan: &Plan, rel: &str) -> String {
    if rel.is_empty() {
        plan.name.clone()
    } else {
        format!("{}/{}", plan.name, rel)
    }
}

pub async fn scan(
    job: &JobShared,
    req: &TransferRequest,
    sftp: &SftpSession,
    opts: &ScanOpts,
) -> Result<Plan, TransferError> {
    job.scanning(0, 0);
    let plan = match req.direction {
        Direction::Upload => scan_upload(job, req, opts).await?,
        Direction::Download => scan_download(job, req, sftp, opts).await?,
    };
    job.scanning(plan.files, plan.bytes);
    Ok(plan)
}

async fn scan_upload(
    job: &JobShared,
    req: &TransferRequest,
    opts: &ScanOpts,
) -> Result<Plan, TransferError> {
    let local = Side::Local;
    let root = req
        .local
        .to_str()
        .ok_or_else(|| {
            TransferError::InvalidLocalName(names::display_name(&req.local.to_string_lossy()))
        })?
        .to_string();
    let name = req
        .local
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| TransferError::InvalidLocalName(names::display_name(&root)))?
        .to_string();
    let meta = local.lstat(&root).await?.ok_or(TransferError::NotFound)?;
    let mut plan = Plan {
        src_root: root.clone(),
        dst_root: names::remote_join(&req.remote, &name),
        name,
        ..Plan::default()
    };
    match meta.kind {
        Kind::Symlink => return Err(TransferError::SymlinkSkipped),
        Kind::Other => return Err(TransferError::UnsupportedKind),
        Kind::File => {
            plan.items.push(Item::File {
                rel: String::new(),
                size: meta.size,
            });
            plan.files = 1;
            plan.bytes = meta.size;
            return Ok(plan);
        }
        Kind::Dir => {}
    }
    plan.items.push(Item::Dir {
        rel: String::new(),
        mtime: meta.mtime,
        perms: meta.perms,
    });
    let mut tick = Ticker::new();
    let mut seen = 0u64;
    let mut stack: Vec<(String, String)> = vec![(root, String::new())];
    while let Some((dir, rel)) = stack.pop() {
        if job.cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        opts.limits.check_depth(&rel)?;
        let mut rd = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| TransferError::from_io(&e))?;
        let mut entries = Vec::new();
        while let Some(de) = rd
            .next_entry()
            .await
            .map_err(|e| TransferError::from_io(&e))?
        {
            entries.push(de);
        }
        seen = seen.saturating_add(entries.len() as u64);
        opts.limits.check_entries(seen)?;
        entries.sort_by_key(|e| e.file_name());
        let mut subdirs = Vec::new();
        for de in entries {
            let fname = de.file_name();
            let Some(name) = fname.to_str() else {
                plan.failed += 1;
                job.file_error(
                    &disp(&plan, &child_rel(&rel, &fname.to_string_lossy())),
                    &TransferError::InvalidLocalName(names::display_name(&fname.to_string_lossy())),
                );
                continue;
            };
            if names::is_artifact(name) {
                continue;
            }
            let crel = child_rel(&rel, name);
            let md = match tokio::fs::symlink_metadata(de.path()).await {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    plan.failed += 1;
                    job.file_error(&disp(&plan, &crel), &TransferError::from_io(&e));
                    continue;
                }
            };
            let ft = md.file_type();
            if ft.is_symlink() {
                plan.skipped += 1;
                job.file_error(&disp(&plan, &crel), &TransferError::SymlinkSkipped);
            } else if ft.is_dir() {
                #[cfg(unix)]
                let perms = {
                    use std::os::unix::fs::PermissionsExt;
                    Some(md.permissions().mode() & 0o7777)
                };
                #[cfg(not(unix))]
                let perms = None;
                plan.items.push(Item::Dir {
                    rel: crel.clone(),
                    mtime: md.modified().ok(),
                    perms,
                });
                subdirs.push((de.path().to_string_lossy().into_owned(), crel));
            } else if ft.is_file() {
                plan.items.push(Item::File {
                    rel: crel,
                    size: md.len(),
                });
                plan.files += 1;
                plan.bytes = plan.bytes.saturating_add(md.len());
            } else {
                plan.skipped += 1;
                job.file_error(&disp(&plan, &crel), &TransferError::UnsupportedKind);
            }
        }
        // Ordem estável: subdiretórios na ordem alfabética.
        stack.extend(subdirs.into_iter().rev());
        if tick.due() {
            job.scanning(plan.files, plan.bytes);
        }
    }
    Ok(plan)
}

async fn scan_download(
    job: &JobShared,
    req: &TransferRequest,
    sftp: &SftpSession,
    opts: &ScanOpts,
) -> Result<Plan, TransferError> {
    let remote = Side::Remote(sftp);
    let local = Side::Local;
    let root = req.remote.trim_end_matches('/').to_string();
    let root = if root.is_empty() {
        "/".to_string()
    } else {
        root
    };
    let name = names::remote_basename(&root)
        .ok_or_else(|| TransferError::InvalidLocalName("/".into()))?
        .to_string();
    names::validate_remote_name(&name)?;
    names::validate_local_name(opts.os, &name)?;
    let dest_parent = req.local.to_str().ok_or_else(|| {
        TransferError::InvalidLocalName(names::display_name(&req.local.to_string_lossy()))
    })?;
    let meta: Meta = remote.lstat(&root).await?.ok_or(TransferError::NotFound)?;
    let mut plan = Plan {
        src_root: root.clone(),
        dst_root: local.join(dest_parent, &name),
        name,
        ..Plan::default()
    };
    match meta.kind {
        Kind::Symlink => return Err(TransferError::SymlinkSkipped),
        Kind::Other => return Err(TransferError::UnsupportedKind),
        Kind::File => {
            plan.items.push(Item::File {
                rel: String::new(),
                size: meta.size,
            });
            plan.files = 1;
            plan.bytes = meta.size;
            return Ok(plan);
        }
        Kind::Dir => {}
    }
    plan.items.push(Item::Dir {
        rel: String::new(),
        mtime: meta.mtime,
        perms: meta.perms,
    });
    let mut tick = Ticker::new();
    let mut seen = 0u64;
    let mut stack: Vec<(String, String)> = vec![(root, String::new())];
    while let Some((dir, rel)) = stack.pop() {
        if job.cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        opts.limits.check_depth(&rel)?;
        let rd = sftp
            .read_dir(dir.as_str())
            .await
            .map_err(|e| TransferError::from_sftp(&e))?;
        let mut entries: Vec<(String, Meta)> = rd
            .map(|e| (e.file_name(), meta_from_attrs(&e.metadata())))
            .collect();
        seen = seen.saturating_add(entries.len() as u64);
        opts.limits.check_entries(seen)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Nomes inválidos: traversal (servidor hostil) e inválidos no SO local.
        let mut valid: Vec<(String, Meta)> = Vec::new();
        for (name, m) in entries {
            let crel = child_rel(&rel, &name);
            if let Err(e) = names::validate_remote_name(&name)
                .and_then(|_| names::validate_local_name(opts.os, &name))
            {
                plan.failed += 1;
                // Nunca monta caminho a partir do nome inválido.
                job.file_error(
                    &disp(&plan, &child_rel(&rel, &names::display_name(&name))),
                    &e,
                );
                continue;
            }
            if names::is_artifact(&name) {
                continue;
            }
            let _ = crel;
            valid.push((name, m));
        }
        // Colisões no destino (caixa/normalização): erro explícito para todos do grupo.
        let all: Vec<String> = valid.iter().map(|(n, _)| n.clone()).collect();
        let groups = names::collisions(opts.os, opts.case_insensitive, &all);
        if !groups.is_empty() {
            for g in &groups {
                plan.failed = plan.failed.saturating_add(g.len() as u64);
                job.file_error(
                    &disp(&plan, &child_rel(&rel, &g[0])),
                    &TransferError::CaseCollision(g.clone()),
                );
            }
            let bad: std::collections::HashSet<&String> = groups.iter().flatten().collect();
            valid.retain(|(n, _)| !bad.contains(n));
        }

        let mut subdirs = Vec::new();
        for (name, m) in valid {
            let crel = child_rel(&rel, &name);
            match m.kind {
                Kind::Symlink => {
                    plan.skipped += 1;
                    job.file_error(&disp(&plan, &crel), &TransferError::SymlinkSkipped);
                }
                Kind::Dir => {
                    plan.items.push(Item::Dir {
                        rel: crel.clone(),
                        mtime: m.mtime,
                        perms: m.perms,
                    });
                    subdirs.push((names::remote_join(&dir, &name), crel));
                }
                Kind::File => {
                    plan.items.push(Item::File {
                        rel: crel,
                        size: m.size,
                    });
                    plan.files += 1;
                    plan.bytes = plan.bytes.saturating_add(m.size);
                }
                Kind::Other => {
                    plan.skipped += 1;
                    job.file_error(&disp(&plan, &crel), &TransferError::UnsupportedKind);
                }
            }
        }
        stack.extend(subdirs.into_iter().rev());
        if tick.due() {
            job.scanning(plan.files, plan.bytes);
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limites_de_profundidade_e_entradas() {
        let l = ScanLimits::default();
        assert_eq!((l.max_depth, l.max_entries), (64, 1_000_000));
        let deep = |n: usize| vec!["d"; n].join("/");
        assert!(l.check_depth("").is_ok());
        assert!(l.check_depth(&deep(64)).is_ok());
        let e = l.check_depth(&deep(65)).unwrap_err();
        assert!(matches!(e, TransferError::ScanLimit(_)), "{e}");
        assert!(e.to_string().contains("profundidade"), "{e}");
        assert!(l.check_entries(1_000_000).is_ok());
        let e = l.check_entries(1_000_001).unwrap_err();
        assert!(e.to_string().contains("entradas"), "{e}");
    }
}
