//! Executor de sincronização unidirecional do mySSH (sem UI): liga o planejador puro
//! (`myssh-sync`) a árvores reais (disco local e SFTP) e executa o plano com o motor de
//! transferência (`myssh-transfer`).
//!
//! # Garantias
//! - **Nunca apaga por padrão.** `Delete` só existe no plano com `mirror_deletes` e só executa com
//!   `Decisions::confirm_deletes == true`. Sem confirmação os deletes ficam `Pending` no journal.
//! - Antes de executar, o plano inteiro é revalidado (`Plan::revalidate`); qualquer mudança aborta
//!   com [`SyncError::PlanStale`] (gere um novo plano). Cada delete faz `lstat` imediatamente antes
//!   e, se o item mudou desde o plano, **não apaga** (`Failed("alterado após o preview")`).
//!   Create/Update também conferem o destino (deve estar como no plano).
//! - Diretórios só são removidos vazios (`rmdir`), depois dos filhos; nunca `rm -r`.
//! - Conflitos só são executados com decisão por item (`ConflictChoice::UseSource`) e apenas os de
//!   data/tamanho (`DestModifiedAfter`, `DestNewer`, `MtimeUnknown`) entre arquivos; colisões de
//!   nome e divergência de tipo nunca são resolvidas automaticamente. Padrão: pular.
//! - Todo caminho é `raiz.join(RelPath)` (componentes validados) e os ancestrais no destino são
//!   verificados com `lstat` (nunca symlink). Symlinks nunca são seguidos.
//! - Arquivos: temporário + rename com mtime preservado (motor do `myssh-transfer`).
//! - O journal é gravado incrementalmente (JSON, 0600 em Unix) com o plano; contém só caminhos
//!   relativos e um id de perfil — sem hostname, usuário ou segredos.
//!
//! # Limitações
//! - Comparação por conteúdo (`CompareMode::Content`) lê **todo** o arquivo dos dois lados via SFTP
//!   (nada é executado no servidor): o tráfego é do tamanho dos arquivos de mesmo tamanho.
//! - Um arquivo por requisição ao `TransferManager` (varredura própria por arquivo); com milhares
//!   de arquivos pequenos o custo por item é alto. Diretórios criados não preservam mtime.
//! - O destino remoto é assumido case-sensitive (configurável com
//!   [`SyncRunner::remote_case_insensitive`]).
//! - O rename remoto com destino existente não é atômico (ver `myssh-transfer`).
//! - A janela entre o `lstat` e a operação (TOCTOU) não pode ser fechada via SFTP.
//! - O plano não é cancelável; o cancelamento vale para a execução.

mod exec;
mod side;

use myssh_ssh::Connection;
use myssh_sync::{
    Action, Changed, Direction, Journal, Op, Plan, PlanError, PlanOptions, RelPath, Status,
    TreeError,
};
use myssh_transfer::{LocalOs, RemoteTree, TransferError};
use serde::{Deserialize, Serialize};
use side::{AnyTree, Hasher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use myssh_sync;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("{}", myssh_i18n::tr!(format!("Erro ao ler a árvore de arquivos: {}", .0), format!("Error reading the file tree: {}", .0), format!("Error al leer el árbol de archivos: {}", .0)))]
    Tree(#[from] TreeError),
    #[error("{0}")]
    Transfer(TransferError),
    #[error("{}", myssh_i18n::tr!(format!("O plano está desatualizado ({} item(ns) mudaram após o preview); gere um novo plano.", .0.len()), format!("The plan is out of date ({} item(s) changed after the preview); generate a new plan.", .0.len()), format!("El plan está desactualizado ({} elemento(s) cambiaron tras la vista previa); genera un plan nuevo.", .0.len())))]
    PlanStale(Vec<Changed>),
    #[error("{}", myssh_i18n::tr!("A direção do plano não corresponde à do executor.", "The plan direction does not match the executor's.", "La dirección del plan no coincide con la del ejecutor."))]
    DirectionMismatch,
    #[error("{}", myssh_i18n::tr!(format!("Caminho fora da raiz da sincronização: «{}».", .0), format!("Path outside the sync root: «{}».", .0), format!("Ruta fuera de la raíz de la sincronización: «{}».", .0)))]
    OutsideRoot(String),
    #[error("{}", myssh_i18n::tr!(format!("Erro de E/S: {}", .0), format!("I/O error: {}", .0), format!("Error de E/S: {}", .0)))]
    Io(String),
}

impl From<TransferError> for SyncError {
    fn from(e: TransferError) -> Self {
        SyncError::Transfer(e)
    }
}

/// Decisão do usuário para um item em conflito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictChoice {
    /// Sobrescreve o destino com a origem.
    UseSource,
    /// Não faz nada (padrão para itens sem decisão).
    Skip,
}

/// Decisões explícitas do usuário. O padrão não apaga nem resolve nenhum conflito.
#[derive(Debug, Clone, Default)]
pub struct Decisions {
    /// Autoriza os `Delete` do plano (só vale se o plano foi feito com `mirror_deletes`).
    pub confirm_deletes: bool,
    pub conflicts: HashMap<RelPath, ConflictChoice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunSummary {
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Itens não executados (cancelamento ou deletes sem confirmação).
    pub pending: usize,
    pub bytes: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Hash de conteúdo durante o planejamento.
    Hashing {
        done: usize,
        total: usize,
        current: String,
    },
    Progress {
        items_done: usize,
        items_total: usize,
        bytes_done: u64,
        bytes_total: u64,
        current: String,
    },
    ItemDone {
        path: RelPath,
        op: Op,
    },
    ItemFailed {
        path: RelPath,
        op: Op,
        message: String,
    },
    ItemSkipped {
        path: RelPath,
        op: Op,
        reason: String,
    },
    Warning {
        message: String,
    },
    Finished(RunSummary),
}

/// Estado persistido: plano + journal (necessário para revalidar na retomada).
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncState {
    pub version: u32,
    pub profile_id: String,
    pub plan: Plan,
    pub journal: Journal,
}

#[derive(Serialize)]
struct SyncStateRef<'a> {
    version: u32,
    profile_id: &'a str,
    plan: &'a Plan,
    journal: &'a Journal,
}

pub(crate) async fn save_state(
    path: &Path,
    profile_id: &str,
    plan: &Plan,
    journal: &Journal,
) -> Result<(), SyncError> {
    let io = |e: std::io::Error| SyncError::Io(e.to_string());
    let json = serde_json::to_vec_pretty(&SyncStateRef {
        version: 1,
        profile_id,
        plan,
        journal,
    })
    .map_err(|e| SyncError::Io(e.to_string()))?;
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(dir).await.map_err(io)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // Um `.tmp` pré-existente (sobra de queda ou plantado, inclusive symlink) é removido sem
    // seguir o link e recriado com `create_new` (O_EXCL), 0600: nunca escreve através dele.
    match tokio::fs::symlink_metadata(&tmp).await {
        Ok(_) => tokio::fs::remove_file(&tmp).await.map_err(io)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(e)),
    }
    {
        use tokio::io::AsyncWriteExt;
        let mut oo = tokio::fs::OpenOptions::new();
        oo.write(true).create_new(true);
        #[cfg(unix)]
        oo.mode(0o600);
        let mut f = oo.open(&tmp).await.map_err(io)?;
        let w = async {
            f.write_all(&json).await?;
            f.sync_all().await
        }
        .await;
        if let Err(e) = w {
            drop(f);
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(io(e));
        }
    }
    // O rename substitui o nome final (mesmo que seja um symlink), sem segui-lo.
    tokio::fs::rename(&tmp, path).await.map_err(io)
}

/// Lê o estado gravado por uma execução anterior.
pub async fn load_state(path: &Path) -> Result<SyncState, SyncError> {
    let raw = tokio::fs::read(path)
        .await
        .map_err(|e| SyncError::Io(e.to_string()))?;
    let mut st: SyncState =
        serde_json::from_slice(&raw).map_err(|e| SyncError::Io(e.to_string()))?;
    // O índice interno do journal não é serializado.
    st.journal = Journal::from_json(
        &st.journal
            .to_json()
            .map_err(|e| SyncError::Io(e.to_string()))?,
    )
    .map_err(|e| SyncError::Io(e.to_string()))?;
    Ok(st)
}

pub(crate) type DeleteHook = Arc<dyn Fn(&RelPath) + Send + Sync>;

pub struct SyncRunner {
    pub(crate) conn: Arc<Connection>,
    pub(crate) local_root: PathBuf,
    pub(crate) remote_root: String,
    pub(crate) profile_id: String,
    pub(crate) journal_path: PathBuf,
    opts: PlanOptions,
    remote_ci: bool,
    pub(crate) concurrency: usize,
    pub(crate) preserve_perms: bool,
    pub(crate) before_delete: Option<DeleteHook>,
}

impl SyncRunner {
    /// A direção vem de `opts.direction`. `journal_path` é o arquivo JSON do estado (no diretório
    /// de dados do app); `profile_id` identifica o perfil sem expor hostname.
    pub fn new(
        conn: Arc<Connection>,
        local_root: impl Into<PathBuf>,
        remote_root: impl Into<String>,
        profile_id: impl Into<String>,
        journal_path: impl Into<PathBuf>,
        opts: PlanOptions,
    ) -> Self {
        SyncRunner {
            conn,
            local_root: local_root.into(),
            remote_root: remote_root.into(),
            profile_id: profile_id.into(),
            journal_path: journal_path.into(),
            opts,
            remote_ci: false,
            concurrency: 3,
            preserve_perms: false,
            before_delete: None,
        }
    }

    /// Destino remoto case-insensitive/normalizador (padrão: não).
    pub fn remote_case_insensitive(mut self, v: bool) -> Self {
        self.remote_ci = v;
        self
    }

    /// Arquivos simultâneos (padrão 3).
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
        self
    }

    /// Copia os bits rwx (nunca setuid/setgid). Padrão: não.
    pub fn preserve_perms(mut self, v: bool) -> Self {
        self.preserve_perms = v;
        self
    }

    /// Só para testes: chamado logo antes da revalidação de cada delete.
    #[doc(hidden)]
    pub fn set_before_delete_hook(&mut self, h: impl Fn(&RelPath) + Send + Sync + 'static) {
        self.before_delete = Some(Arc::new(h));
    }

    pub(crate) fn dest_is_remote(&self) -> bool {
        self.opts.direction == Direction::LocalToRemote
    }

    /// Opções efetivas: o destino local macOS/Windows é case-insensitive (macOS também
    /// normaliza Unicode; Windows valida nomes); o remoto segue `remote_case_insensitive`.
    fn effective_opts(&self) -> PlanOptions {
        let mut o = self.opts.clone();
        if self.dest_is_remote() {
            o.case_insensitive_dest |= self.remote_ci;
            o.unicode_normalizing_dest |= self.remote_ci;
        } else {
            let os = LocalOs::current();
            o.case_insensitive_dest |= os.default_case_insensitive();
            o.unicode_normalizing_dest |= os == LocalOs::MacOs;
            o.dest_windows |= os == LocalOs::Windows;
        }
        o
    }

    pub(crate) async fn trees(&self) -> Result<(AnyTree, AnyTree), SyncError> {
        let local = || AnyTree::Local(myssh_sync::LocalTree::new(self.local_root.clone()));
        let remote = async {
            RemoteTree::open(self.conn.clone(), self.remote_root.clone())
                .await
                .map(AnyTree::Remote)
                .map_err(SyncError::Transfer)
        };
        Ok(match self.opts.direction {
            Direction::LocalToRemote => (local(), remote.await?),
            Direction::RemoteToLocal => (remote.await?, local()),
        })
    }

    /// Gera o plano (dry-run). Com `CompareMode::Content`, resolve os hashes (ver limitações).
    pub async fn plan(&self) -> Result<Plan, SyncError> {
        self.plan_with_events(None).await
    }

    pub async fn plan_with_events(
        &self,
        events: Option<&mpsc::Sender<SyncEvent>>,
    ) -> Result<Plan, SyncError> {
        let (src, dst) = self.trees().await?;
        let opts = self.effective_opts();
        let mut plan = myssh_sync::plan(&src, &dst, &opts).await?;
        let total = plan.summary().needs_hash;
        if total > 0 {
            let remote_src = !self.dest_is_remote();
            let hs =
                Hasher::open(&self.conn, remote_src, &self.local_root, &self.remote_root).await?;
            let hd =
                Hasher::open(&self.conn, !remote_src, &self.local_root, &self.remote_root).await?;
            let todo = plan.items.iter_mut().filter(|i| i.action.needs_hash());
            for (n, it) in todo.enumerate() {
                let (a, b) = (hs.hash(&it.path).await?, hd.hash(&it.path).await?);
                it.action = if a == b {
                    Action::Skip {
                        reason: myssh_sync::SkipReason::Identical,
                    }
                } else {
                    Action::Update {
                        reason: myssh_sync::UpdateReason::ContentDiffers,
                    }
                };
                if let Some(tx) = events {
                    let _ = tx
                        .send(SyncEvent::Hashing {
                            done: n + 1,
                            total,
                            current: it.path.to_string(),
                        })
                        .await;
                }
            }
        }
        Ok(plan)
    }

    fn use_source(plan: &Plan, d: &Decisions) -> HashSet<RelPath> {
        plan.items
            .iter()
            .filter(|i| matches!(i.action, Action::Conflict { .. }))
            .filter(|i| d.conflicts.get(&i.path) == Some(&ConflictChoice::UseSource))
            .map(|i| i.path.clone())
            .collect()
    }

    /// Executa o plano. Revalida antes de tudo; se algo mudou, `Err(PlanStale)` sem alterar nada.
    pub async fn execute(
        &self,
        plan: &Plan,
        decisions: &Decisions,
        events: mpsc::Sender<SyncEvent>,
        cancel: CancellationToken,
    ) -> Result<Journal, SyncError> {
        if plan.direction != self.opts.direction {
            return Err(SyncError::DirectionMismatch);
        }
        let journal = Journal::from_plan_with_choices(plan, &Self::use_source(plan, decisions));
        let (src, dst) = self.trees().await?;
        let changed = plan.revalidate(&src, &dst).await?;
        if !changed.is_empty() {
            return Err(SyncError::PlanStale(changed));
        }
        drop((src, dst));
        exec::Run::new(self, plan, decisions, journal, events, cancel)
            .await?
            .run()
            .await
    }

    /// Retoma uma execução interrompida: revalida e refaz só os itens `Pending`/`Failed`.
    /// `plan` e `journal` vêm de [`load_state`]; as `decisions` precisam ser fornecidas de novo.
    pub async fn resume(
        &self,
        plan: &Plan,
        decisions: &Decisions,
        journal: Journal,
        events: mpsc::Sender<SyncEvent>,
        cancel: CancellationToken,
    ) -> Result<Journal, SyncError> {
        if plan.direction != self.opts.direction {
            return Err(SyncError::DirectionMismatch);
        }
        let remaining: HashSet<&RelPath> = journal.remaining().map(|i| &i.path).collect();
        let sub = Plan {
            direction: plan.direction,
            mirror_deletes: plan.mirror_deletes,
            items: plan
                .items
                .iter()
                .filter(|i| remaining.contains(&i.path) && !matches!(i.action, Action::Skip { .. }))
                .cloned()
                .collect(),
        };
        let (src, dst) = self.trees().await?;
        let changed = sub.revalidate(&src, &dst).await?;
        if !changed.is_empty() {
            return Err(SyncError::PlanStale(changed));
        }
        drop((src, dst));
        // Falhos voltam a Pending para nova tentativa.
        let mut journal = journal;
        let failed: Vec<_> = journal
            .items
            .iter()
            .filter(|i| matches!(i.status, Status::Failed(_)))
            .map(|i| (i.path.clone(), i.op))
            .collect();
        for (p, op) in failed {
            journal.mark(&p, op, Status::Pending);
        }
        exec::Run::new(self, plan, decisions, journal, events, cancel)
            .await?
            .run()
            .await
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use myssh_sync::{Direction, Plan};
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn vazio() -> (Plan, Journal) {
        let plan = Plan {
            direction: Direction::LocalToRemote,
            mirror_deletes: false,
            items: vec![],
        };
        let j = Journal::from_plan(&plan);
        (plan, j)
    }

    #[tokio::test]
    async fn journal_tmp_symlink_plantado_nao_e_seguido_e_arquivo_e_0600() {
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("vitima.txt");
        std::fs::write(&victim, "intacto").unwrap();
        let jp = d.path().join("estado.json");
        symlink(&victim, d.path().join("estado.json.tmp")).unwrap();
        let (plan, j) = vazio();
        save_state(&jp, "perfil", &plan, &j).await.unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "intacto");
        assert_eq!(
            std::fs::metadata(&jp).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!d.path().join("estado.json.tmp").exists());
        load_state(&jp).await.unwrap();

        // Symlink plantado no próprio destino: substituído, não seguido.
        std::fs::remove_file(&jp).unwrap();
        symlink(&victim, &jp).unwrap();
        save_state(&jp, "perfil", &plan, &j).await.unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "intacto");
        assert!(
            std::fs::symlink_metadata(&jp)
                .unwrap()
                .file_type()
                .is_file()
        );
    }
}

#[cfg(test)]
mod i18n_tests {
    use super::*;

    #[test]
    fn erros_traduzidos() {
        myssh_i18n::with_lang(myssh_i18n::Lang::En, || {
            assert_eq!(
                SyncError::OutsideRoot("x".into()).to_string(),
                "Path outside the sync root: «x»."
            );
            assert_eq!(exec::changed(), "changed after the preview");
        });
        myssh_i18n::with_lang(myssh_i18n::Lang::Es, || {
            assert_eq!(
                SyncError::DirectionMismatch.to_string(),
                "La dirección del plan no coincide con la del ejecutor."
            );
        });
    }
}
