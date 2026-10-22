//! Exportação/importação de configuração sem segredos.
//!
//! Referências a credenciais nunca são exportadas: perfis com senha viram "perguntar senha";
//! perfis com chave mantêm só o caminho (a passphrase salva não é exportada).

use crate::Store;
use crate::error::{Result, StoreError};
use crate::repo::{self, WriteMode};
use myssh_core::{AuthMethod, Environment, FolderId, Profile, ProfileId, ThemeOverride, Transport};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedConfig {
    pub schema_version: u32,
    pub folders: Vec<ExportFolder>,
    pub profiles: Vec<ExportProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFolder {
    pub id: FolderId,
    pub name: String,
    pub parent: Option<FolderId>,
}

/// Autenticação sem nenhuma referência a segredo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportAuth {
    /// Perguntar a senha a cada conexão.
    AskPassword,
    KeyFile {
        path: PathBuf,
    },
    Agent,
    KeyboardInteractive,
}

impl From<&AuthMethod> for ExportAuth {
    fn from(a: &AuthMethod) -> Self {
        match a {
            AuthMethod::Password { .. } => ExportAuth::AskPassword,
            AuthMethod::KeyFile { path, .. } => ExportAuth::KeyFile { path: path.clone() },
            AuthMethod::Agent => ExportAuth::Agent,
            AuthMethod::KeyboardInteractive => ExportAuth::KeyboardInteractive,
        }
    }
}

impl From<ExportAuth> for AuthMethod {
    fn from(a: ExportAuth) -> Self {
        match a {
            ExportAuth::AskPassword => AuthMethod::Password { credential: None },
            ExportAuth::KeyFile { path } => AuthMethod::KeyFile {
                path,
                passphrase: None,
            },
            ExportAuth::Agent => AuthMethod::Agent,
            ExportAuth::KeyboardInteractive => AuthMethod::KeyboardInteractive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportProfile {
    pub id: ProfileId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: ExportAuth,
    pub folder: Option<FolderId>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub environment: Environment,
    pub theme: ThemeOverride,
    pub jump: Option<ProfileId>,
    pub agent_forwarding: bool,
    pub keepalive_secs: Option<u32>,
    pub connect_timeout_secs: u32,
    /// Sem segredos (tokens SSO ficam no cofre). Ausente em exportações antigas.
    #[serde(default)]
    pub transport: Transport,
}

impl ExportProfile {
    fn from_profile(p: &Profile) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            host: p.host.clone(),
            port: p.port,
            username: p.username.clone(),
            auth: ExportAuth::from(&p.auth),
            folder: p.folder,
            tags: p.tags.clone(),
            favorite: p.favorite,
            environment: p.environment,
            theme: p.theme.clone(),
            jump: p.jump,
            agent_forwarding: p.agent_forwarding,
            keepalive_secs: p.keepalive_secs,
            connect_timeout_secs: p.connect_timeout_secs,
            transport: p.transport.clone(),
        }
    }

    fn into_profile(self) -> Profile {
        Profile {
            id: self.id,
            name: self.name,
            host: self.host,
            port: self.port,
            username: self.username,
            auth: self.auth.into(),
            folder: self.folder,
            tags: self.tags,
            favorite: self.favorite,
            environment: self.environment,
            theme: self.theme,
            jump: self.jump,
            agent_forwarding: self.agent_forwarding,
            keepalive_secs: self.keepalive_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            transport: self.transport,
        }
    }
}

/// Resultado de uma importação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportReport {
    pub folders: usize,
    pub profiles: usize,
    /// Quantos IDs colidiram com itens existentes e receberam novo ID.
    pub ids_regenerated: usize,
}

fn bad(msg: &str) -> StoreError {
    StoreError::InvalidConfig(msg.to_string())
}

impl Store {
    /// Exporta pastas e perfis (com tags) sem qualquer referência a segredo.
    pub fn export_config(&self) -> Result<ExportedConfig> {
        let c = self.lock()?;
        let folders = repo::list_folders(&c)?
            .into_iter()
            .map(|f| ExportFolder {
                id: f.id,
                name: f.name,
                parent: f.parent,
            })
            .collect();
        let profiles = repo::all_profiles(&c)?
            .iter()
            .map(ExportProfile::from_profile)
            .collect();
        Ok(ExportedConfig {
            schema_version: SCHEMA_VERSION,
            folders,
            profiles,
        })
    }

    pub fn export_config_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.export_config()?)?)
    }

    /// Importa configuração em JSON. Nunca sobrescreve: IDs colidentes recebem novos IDs.
    /// Tudo ou nada (transação única).
    pub fn import_config_json(&self, json: &str) -> Result<ImportReport> {
        if json.len() > MAX_IMPORT_BYTES {
            return Err(bad(myssh_i18n::tr!(
                "o arquivo é grande demais.",
                "the file is too large.",
                "el archivo es demasiado grande."
            )));
        }
        let cfg: ExportedConfig = serde_json::from_str(json).map_err(|e| {
            StoreError::InvalidConfig(myssh_i18n::tr!(
                format!("JSON inválido ({e})."),
                format!("Invalid JSON ({e})."),
                format!("JSON no válido ({e}).")
            ))
        })?;
        self.import_config(cfg)
    }

    pub fn import_config(&self, cfg: ExportedConfig) -> Result<ImportReport> {
        if cfg.schema_version != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema(cfg.schema_version));
        }
        validate(&cfg)?;

        let mut c = self.lock()?;
        let tx = c.transaction()?;

        let existing_folders: HashSet<FolderId> =
            repo::list_folders(&tx)?.into_iter().map(|f| f.id).collect();
        let existing_profiles: HashSet<ProfileId> =
            repo::all_profiles(&tx)?.into_iter().map(|p| p.id).collect();

        let mut regenerated = 0;
        let mut fmap: HashMap<FolderId, FolderId> = HashMap::new();
        for f in &cfg.folders {
            let id = if existing_folders.contains(&f.id) {
                regenerated += 1;
                FolderId(Uuid::new_v4())
            } else {
                f.id
            };
            fmap.insert(f.id, id);
        }
        let mut pmap: HashMap<ProfileId, ProfileId> = HashMap::new();
        for p in &cfg.profiles {
            let id = if existing_profiles.contains(&p.id) {
                regenerated += 1;
                ProfileId(Uuid::new_v4())
            } else {
                p.id
            };
            pmap.insert(p.id, id);
        }

        // Pastas: insere sem pai e depois liga (pais podem vir depois dos filhos).
        for f in &cfg.folders {
            tx.execute(
                "INSERT INTO folders (id, name, parent_id) VALUES (?1, ?2, NULL)",
                params![fmap[&f.id].to_string(), f.name.trim()],
            )?;
        }
        for f in &cfg.folders {
            if let Some(parent) = f.parent {
                tx.execute(
                    "UPDATE folders SET parent_id = ?2 WHERE id = ?1",
                    params![fmap[&f.id].to_string(), fmap[&parent].to_string()],
                )?;
            }
        }
        // Perfis: insere sem jump e depois liga.
        for ep in &cfg.profiles {
            let mut p = ep.clone().into_profile();
            p.id = pmap[&ep.id];
            p.folder = ep.folder.map(|f| fmap[&f]);
            p.jump = None;
            repo::save_profile(&tx, &p, WriteMode::Insert)?;
        }
        for ep in &cfg.profiles {
            if let Some(j) = ep.jump {
                tx.execute(
                    "UPDATE profiles SET jump_id = ?2 WHERE id = ?1",
                    params![pmap[&ep.id].to_string(), pmap[&j].to_string()],
                )?;
            }
        }
        tx.commit()?;
        Ok(ImportReport {
            folders: cfg.folders.len(),
            profiles: cfg.profiles.len(),
            ids_regenerated: regenerated,
        })
    }
}

/// Valida o documento por inteiro antes de tocar no banco.
fn validate(cfg: &ExportedConfig) -> Result<()> {
    let mut folder_ids = HashSet::new();
    for f in &cfg.folders {
        if !folder_ids.insert(f.id) {
            return Err(bad(myssh_i18n::tr!(
                "pastas com identificador duplicado.",
                "folders with duplicate identifiers.",
                "carpetas con identificador duplicado."
            )));
        }
        if f.name.trim().is_empty() {
            return Err(StoreError::EmptyFolderName);
        }
    }
    let parents: HashMap<FolderId, Option<FolderId>> =
        cfg.folders.iter().map(|f| (f.id, f.parent)).collect();
    for f in &cfg.folders {
        let mut seen = HashSet::from([f.id]);
        let mut cur = f.parent;
        while let Some(p) = cur {
            if !folder_ids.contains(&p) {
                return Err(bad(myssh_i18n::tr!(
                    "pasta pai inexistente.",
                    "parent folder does not exist.",
                    "la carpeta superior no existe."
                )));
            }
            if !seen.insert(p) {
                return Err(StoreError::FolderCycle);
            }
            cur = parents[&p];
        }
    }

    let mut profile_ids = HashSet::new();
    for p in &cfg.profiles {
        if !profile_ids.insert(p.id) {
            return Err(bad(myssh_i18n::tr!(
                "perfis com identificador duplicado.",
                "profiles with duplicate identifiers.",
                "perfiles con identificador duplicado."
            )));
        }
    }
    let jumps: HashMap<ProfileId, Option<ProfileId>> =
        cfg.profiles.iter().map(|p| (p.id, p.jump)).collect();
    for ep in &cfg.profiles {
        ep.clone().into_profile().validate()?;
        if ep.tags.iter().any(|t| t.chars().any(|c| c.is_control())) {
            return Err(bad(myssh_i18n::tr!(
                "tag com caracteres inválidos.",
                "tag with invalid characters.",
                "etiqueta con caracteres no válidos."
            )));
        }
        if let Some(f) = ep.folder
            && !folder_ids.contains(&f)
        {
            return Err(bad(myssh_i18n::tr!(
                "perfil referencia pasta inexistente.",
                "profile references a folder that does not exist.",
                "un perfil hace referencia a una carpeta que no existe."
            )));
        }
        let mut seen = HashSet::from([ep.id]);
        let mut cur = ep.jump;
        while let Some(j) = cur {
            if !profile_ids.contains(&j) {
                return Err(bad(myssh_i18n::tr!(
                    "perfil referencia jump host inexistente.",
                    "profile references a jump host that does not exist.",
                    "un perfil hace referencia a un jump host que no existe."
                )));
            }
            if !seen.insert(j) {
                return Err(StoreError::JumpCycle);
            }
            cur = jumps[&j];
        }
    }
    Ok(())
}
