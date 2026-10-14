//! Persistência local do mySSH em SQLite. Somente metadados: nenhum segredo é gravado.

mod config;
mod error;
mod migrate;
mod repo;
mod trust;

pub use config::{
    ExportAuth, ExportFolder, ExportProfile, ExportedConfig, ImportReport, SCHEMA_VERSION,
};
pub use error::{Result, StoreError};

use myssh_core::{Credential, CredentialId, Environment, FolderId, Profile, ProfileId};
use repo::{WriteMode, parse_uuid};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Pasta de perfis (aninhável).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub parent: Option<FolderId>,
}

impl Folder {
    pub fn new(name: impl Into<String>, parent: Option<FolderId>) -> Self {
        Self {
            id: FolderId::new(),
            name: name.into(),
            parent,
        }
    }
}

/// Filtros de listagem; todos combinados com E.
#[derive(Debug, Clone, Default)]
pub struct ProfileFilter {
    /// Busca textual (nome, host, usuário ou tag), sem diferenciar caixa.
    pub search: Option<String>,
    pub folder: Option<FolderId>,
    pub favorite: Option<bool>,
    pub environment: Option<Environment>,
    /// Tag exata (sem diferenciar caixa).
    pub tag: Option<String>,
}

/// Armazenamento SQLite thread-safe.
pub struct Store {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    /// Abre (ou cria) o banco em `path`, aplicando migrações pendentes.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).map_err(|_| StoreError::Open)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| StoreError::Open)?;
        Self::init(conn, migrate::MIGRATIONS)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|_| StoreError::Open)?;
        Self::init(conn, migrate::MIGRATIONS)
    }

    fn init(mut conn: Connection, migrations: &[&str]) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate::migrate(&mut conn, migrations)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| StoreError::Poisoned)
    }

    // ---------- perfis ----------

    pub fn create_profile(&self, p: &Profile) -> Result<()> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        repo::save_profile(&tx, p, WriteMode::Insert)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_profile(&self, p: &Profile) -> Result<()> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        repo::save_profile(&tx, p, WriteMode::Update)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_profile(&self, id: ProfileId) -> Result<Option<Profile>> {
        repo::get_profile(&*self.lock()?, id)
    }

    /// Remove o perfil; perfis que o usavam como jump host ficam sem jump.
    pub fn delete_profile(&self, id: ProfileId) -> Result<()> {
        let c = self.lock()?;
        let n = c.execute("DELETE FROM profiles WHERE id = ?1", [id.to_string()])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Lista perfis ordenados por nome, aplicando busca e filtros.
    pub fn list_profiles(&self, filter: &ProfileFilter) -> Result<Vec<Profile>> {
        let all = repo::all_profiles(&*self.lock()?)?;
        let needle = filter
            .search
            .as_deref()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let tag = filter.tag.as_deref().map(|t| t.trim().to_lowercase());
        Ok(all
            .into_iter()
            .filter(|p| filter.folder.is_none_or(|f| p.folder == Some(f)))
            .filter(|p| filter.favorite.is_none_or(|f| p.favorite == f))
            .filter(|p| filter.environment.is_none_or(|e| p.environment == e))
            .filter(|p| {
                tag.as_ref()
                    .is_none_or(|t| p.tags.iter().any(|x| x.to_lowercase() == *t))
            })
            .filter(|p| {
                needle.as_ref().is_none_or(|n| {
                    p.name.to_lowercase().contains(n.as_str())
                        || p.host.to_lowercase().contains(n.as_str())
                        || p.username.to_lowercase().contains(n.as_str())
                        || p.tags.iter().any(|t| t.to_lowercase().contains(n.as_str()))
                })
            })
            .collect())
    }

    // ---------- pastas ----------

    pub fn create_folder(&self, f: &Folder) -> Result<()> {
        if f.name.trim().is_empty() {
            return Err(StoreError::EmptyFolderName);
        }
        let c = self.lock()?;
        c.execute(
            "INSERT INTO folders (id, name, parent_id) VALUES (?1, ?2, ?3)",
            params![
                f.id.to_string(),
                f.name.trim(),
                f.parent.map(|p| p.to_string())
            ],
        )?;
        Ok(())
    }

    pub fn update_folder(&self, f: &Folder) -> Result<()> {
        if f.name.trim().is_empty() {
            return Err(StoreError::EmptyFolderName);
        }
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let mut visited = HashSet::new();
        let mut cur = f.parent;
        while let Some(p) = cur {
            if p == f.id || !visited.insert(p) {
                return Err(StoreError::FolderCycle);
            }
            let next: Option<Option<String>> = tx
                .query_row(
                    "SELECT parent_id FROM folders WHERE id = ?1",
                    [p.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            cur = match next.flatten() {
                Some(s) => Some(FolderId(parse_uuid(&s)?)),
                None => None,
            };
        }
        let n = tx.execute(
            "UPDATE folders SET name=?2, parent_id=?3 WHERE id=?1",
            params![
                f.id.to_string(),
                f.name.trim(),
                f.parent.map(|p| p.to_string())
            ],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove a pasta; perfis e subpastas ficam sem pasta (não são apagados).
    pub fn delete_folder(&self, id: FolderId) -> Result<()> {
        let c = self.lock()?;
        let n = c.execute("DELETE FROM folders WHERE id = ?1", [id.to_string()])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let c = self.lock()?;
        repo::list_folders(&c)
    }

    // ---------- credenciais (metadados) ----------

    pub fn create_credential(&self, cred: &Credential) -> Result<()> {
        repo::insert_credential(&*self.lock()?, cred)
    }

    pub fn update_credential(&self, cred: &Credential) -> Result<()> {
        repo::update_credential(&*self.lock()?, cred)
    }

    pub fn get_credential(&self, id: CredentialId) -> Result<Option<Credential>> {
        repo::get_credential(&*self.lock()?, id)
    }

    pub fn list_credentials(&self) -> Result<Vec<Credential>> {
        repo::list_credentials(&*self.lock()?)
    }

    pub fn profiles_using_credential(&self, id: CredentialId) -> Result<Vec<ProfileId>> {
        repo::profiles_using_credential(&*self.lock()?, id)
    }

    /// Remove a credencial; falha com [`StoreError::CredentialInUse`] se algum perfil a usa.
    /// (O segredo em si é removido do cofre pelo chamador.)
    pub fn delete_credential(&self, id: CredentialId) -> Result<()> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let used = repo::profiles_using_credential(&tx, id)?;
        if !used.is_empty() {
            return Err(StoreError::CredentialInUse(used));
        }
        let n = tx.execute("DELETE FROM credentials WHERE id = ?1", [id.to_string()])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit()?;
        Ok(())
    }

    // ---------- configurações ----------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .lock()?
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.lock()?.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.lock()?
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
