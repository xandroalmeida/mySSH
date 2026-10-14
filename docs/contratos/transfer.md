# Contrato M3 — crate `myssh-transfer` (definido pelo Opus)

Crate `crates/transfer` sobre `myssh_ssh::Connection::open_sftp()`. Sem egui. Mudanças só via Opus.

```rust
pub enum Kind { File, Dir, Symlink, Other }
pub struct RemoteEntry { pub name: String, pub path: String, pub kind: Kind, pub size: u64,
                         pub mtime: Option<SystemTime>, pub perms: Option<u32> }

/// Navegação remota (cada chamada usa o canal SFTP do browser; nunca o da fila).
pub struct RemoteBrowser;
impl RemoteBrowser {
    pub async fn open(conn: Arc<Connection>) -> Result<Self, TransferError>;
    pub async fn home(&self) -> Result<String, TransferError>;          // realpath(".")
    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, TransferError>; // sem "." e ".."
    pub async fn mkdir(&self, path: &str) -> Result<(), TransferError>;
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), TransferError>;
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), TransferError>; // UI confirma antes
}

pub enum Direction { Upload, Download }
pub enum Overwrite { Ask, Overwrite, Skip, Rename }   // Rename = "nome (1).ext"
pub enum ConflictDecision { Overwrite, Skip, Rename, OverwriteAll, SkipAll, Cancel }

pub struct TransferRequest {
    pub direction: Direction,
    pub local: PathBuf,        // arquivo ou diretório local (origem no upload, destino-pai no download)
    pub remote: String,        // idem no remoto
    pub overwrite: Overwrite,
    pub preserve_mtime: bool,  // padrão true
    pub preserve_perms: bool,  // padrão false (só bits rwx do usuário, nunca setuid/setgid)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct TransferId(pub u64);

pub enum TransferEvent {
    Queued { id: TransferId, label: String },
    Scanning { id: TransferId, files: u64, bytes: u64 },            // contagem antes de começar
    Progress { id: TransferId, current: String, bytes_done: u64, bytes_total: u64,
               files_done: u64, files_total: u64, rate_bps: u64 },
    Conflict { id: TransferId, path: String, existing: RemoteOrLocalMeta, responder: ConflictResponder },
    FileError { id: TransferId, path: String, message: String },     // pt-BR, sem segredos
    Finished { id: TransferId, ok: u64, failed: u64, skipped: u64 },
    Cancelled { id: TransferId },
}

pub struct TransferManager;
impl TransferManager {
    /// `max_concurrent` arquivos simultâneos (padrão 3), canais SFTP próprios (não o do browser).
    pub fn new(rt: &Handle, conn: Arc<Connection>, max_concurrent: usize, notify: Notify) -> Self;
    pub fn enqueue(&self, req: TransferRequest) -> TransferId;
    pub fn cancel(&self, id: TransferId);
    pub fn cancel_all(&self);
    pub fn try_recv(&mut self) -> Option<TransferEvent>;   // UI drena por frame
    pub fn active(&self) -> usize;                          // para confirmar fechamento
}
```

Regras: escrita em temporário `.<nome>.myssh-part-<rand>` no mesmo diretório e rename ao fim (no remoto: `posix-rename@openssh.com` se anunciado; senão remove+rename, documentado como NÃO atômico); cancelamento remove o temporário; não seguir symlinks na recursão (reportar como pulado); nomes remotos validados contra traversal (`/`, `..`, NUL) antes de virar caminho local; nomes inválidos no SO local → `FileError` (sem renomear silenciosamente); retomada só quando o temporário existe E origem tem mesmo tamanho/mtime registrados num sidecar — senão recomeça. Leitura/escrita em blocos com backpressure; transferência não pode atrasar o terminal (canal SFTP separado, prioridade natural do SSH; limitar janelas).
