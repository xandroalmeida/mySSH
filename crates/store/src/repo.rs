//! Operações de baixo nível sobre `&Connection` (reutilizáveis dentro de transações).

use crate::error::{Result, StoreError};
use myssh_core::{
    AuthMethod, Credential, CredentialId, CredentialKind, Environment, FolderId, Profile,
    ProfileId, SecretRef, ThemeOverride, Transport,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;
use std::str::FromStr;
use uuid::Uuid;

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid> {
    Uuid::from_str(s).map_err(|_| StoreError::Corrupt)
}

pub(crate) fn env_to_str(e: Environment) -> &'static str {
    match e {
        Environment::None => "none",
        Environment::Development => "development",
        Environment::Staging => "staging",
        Environment::Production => "production",
    }
}

fn env_from_str(s: &str) -> Result<Environment> {
    Ok(match s {
        "none" => Environment::None,
        "development" => Environment::Development,
        "staging" => Environment::Staging,
        "production" => Environment::Production,
        _ => return Err(StoreError::Corrupt),
    })
}

fn kind_to_str(k: CredentialKind) -> &'static str {
    match k {
        CredentialKind::Password => "password",
        CredentialKind::KeyPassphrase => "key_passphrase",
    }
}

fn kind_from_str(s: &str) -> Result<CredentialKind> {
    Ok(match s {
        "password" => CredentialKind::Password,
        "key_passphrase" => CredentialKind::KeyPassphrase,
        _ => return Err(StoreError::Corrupt),
    })
}

/// Normaliza tags: remove vazias, apara espaços e deduplica sem diferenciar caixa.
pub(crate) fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in tags {
        let t = t.trim();
        if !t.is_empty() && seen.insert(t.to_lowercase()) {
            out.push(t.to_string());
        }
    }
    out
}

fn auth_refs(auth: &AuthMethod) -> (Option<CredentialId>, Option<CredentialId>) {
    match auth {
        AuthMethod::Password { credential } => (*credential, None),
        AuthMethod::KeyFile { passphrase, .. } => (None, *passphrase),
        AuthMethod::Agent | AuthMethod::KeyboardInteractive => (None, None),
    }
}

// ---------- perfis ----------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteMode {
    Insert,
    Update,
}

pub(crate) fn check_jump_cycle(
    conn: &Connection,
    id: ProfileId,
    jump: Option<ProfileId>,
) -> Result<()> {
    let mut visited = HashSet::new();
    let mut cur = jump;
    while let Some(c) = cur {
        if c == id || !visited.insert(c) {
            return Err(StoreError::JumpCycle);
        }
        let next: Option<Option<String>> = conn
            .query_row(
                "SELECT jump_id FROM profiles WHERE id = ?1",
                [c.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        cur = match next.flatten() {
            Some(s) => Some(ProfileId(parse_uuid(&s)?)),
            None => None,
        };
    }
    Ok(())
}

pub(crate) fn save_profile(conn: &Connection, p: &Profile, mode: WriteMode) -> Result<()> {
    p.validate()?;
    check_jump_cycle(conn, p.id, p.jump)?;
    let (cred, pass) = auth_refs(&p.auth);
    let auth_json = serde_json::to_string(&p.auth)?;
    let theme_json = serde_json::to_string(&p.theme)?;
    let transport_json = serde_json::to_string(&p.transport)?;
    let args = params![
        p.id.to_string(),
        p.name.trim(),
        p.host.trim(),
        p.port,
        p.username.trim(),
        auth_json,
        cred.map(|c| c.to_string()),
        pass.map(|c| c.to_string()),
        p.folder.map(|f| f.to_string()),
        p.favorite,
        env_to_str(p.environment),
        theme_json,
        p.jump.map(|j| j.to_string()),
        p.agent_forwarding,
        p.keepalive_secs,
        p.connect_timeout_secs,
        transport_json,
    ];
    match mode {
        WriteMode::Insert => {
            conn.execute(
                "INSERT INTO profiles (id, name, host, port, username, auth_json, credential_id, \
                 passphrase_id, folder_id, favorite, environment, theme_json, jump_id, \
                 agent_forwarding, keepalive_secs, connect_timeout_secs, transport_json) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                args,
            )?;
        }
        WriteMode::Update => {
            let n = conn.execute(
                "UPDATE profiles SET name=?2, host=?3, port=?4, username=?5, auth_json=?6, \
                 credential_id=?7, passphrase_id=?8, folder_id=?9, favorite=?10, environment=?11, \
                 theme_json=?12, jump_id=?13, agent_forwarding=?14, keepalive_secs=?15, \
                 connect_timeout_secs=?16, transport_json=?17 WHERE id=?1",
                args,
            )?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
        }
    }
    conn.execute(
        "DELETE FROM profile_tags WHERE profile_id = ?1",
        [p.id.to_string()],
    )?;
    for (i, tag) in normalize_tags(&p.tags).iter().enumerate() {
        conn.execute(
            "INSERT INTO profile_tags (profile_id, tag, position) VALUES (?1, ?2, ?3)",
            params![p.id.to_string(), tag, i as i64],
        )?;
    }
    Ok(())
}

const PROFILE_COLS: &str = "id, name, host, port, username, auth_json, folder_id, favorite, \
     environment, theme_json, jump_id, agent_forwarding, keepalive_secs, connect_timeout_secs, \
     transport_json";

struct RawProfile {
    id: String,
    name: String,
    host: String,
    port: u16,
    username: String,
    auth_json: String,
    folder: Option<String>,
    favorite: bool,
    environment: String,
    theme_json: String,
    jump: Option<String>,
    agent_forwarding: bool,
    keepalive_secs: Option<u32>,
    connect_timeout_secs: u32,
    transport_json: String,
}

fn raw_profile(r: &Row<'_>) -> rusqlite::Result<RawProfile> {
    Ok(RawProfile {
        id: r.get(0)?,
        name: r.get(1)?,
        host: r.get(2)?,
        port: r.get(3)?,
        username: r.get(4)?,
        auth_json: r.get(5)?,
        folder: r.get(6)?,
        favorite: r.get(7)?,
        environment: r.get(8)?,
        theme_json: r.get(9)?,
        jump: r.get(10)?,
        agent_forwarding: r.get(11)?,
        keepalive_secs: r.get(12)?,
        connect_timeout_secs: r.get(13)?,
        transport_json: r.get(14)?,
    })
}

fn tags_of(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut st = conn
        .prepare_cached("SELECT tag FROM profile_tags WHERE profile_id = ?1 ORDER BY position")?;
    let tags = st
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tags)
}

fn build_profile(conn: &Connection, raw: RawProfile) -> Result<Profile> {
    let tags = tags_of(conn, &raw.id)?;
    Ok(Profile {
        id: ProfileId(parse_uuid(&raw.id)?),
        name: raw.name,
        host: raw.host,
        port: raw.port,
        username: raw.username,
        auth: serde_json::from_str::<AuthMethod>(&raw.auth_json)?,
        folder: raw
            .folder
            .map(|s| parse_uuid(&s).map(FolderId))
            .transpose()?,
        tags,
        favorite: raw.favorite,
        environment: env_from_str(&raw.environment)?,
        theme: serde_json::from_str::<ThemeOverride>(&raw.theme_json)?,
        jump: raw
            .jump
            .map(|s| parse_uuid(&s).map(ProfileId))
            .transpose()?,
        agent_forwarding: raw.agent_forwarding,
        keepalive_secs: raw.keepalive_secs,
        connect_timeout_secs: raw.connect_timeout_secs,
        transport: serde_json::from_str::<Transport>(&raw.transport_json)?,
    })
}

pub(crate) fn get_profile(conn: &Connection, id: ProfileId) -> Result<Option<Profile>> {
    let raw = conn
        .query_row(
            &format!("SELECT {PROFILE_COLS} FROM profiles WHERE id = ?1"),
            [id.to_string()],
            raw_profile,
        )
        .optional()?;
    raw.map(|r| build_profile(conn, r)).transpose()
}

pub(crate) fn all_profiles(conn: &Connection) -> Result<Vec<Profile>> {
    let mut st = conn.prepare(&format!(
        "SELECT {PROFILE_COLS} FROM profiles ORDER BY name COLLATE NOCASE, id"
    ))?;
    let raws = st
        .query_map([], raw_profile)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(|r| build_profile(conn, r)).collect()
}

// ---------- pastas ----------

pub(crate) fn list_folders(conn: &Connection) -> Result<Vec<crate::Folder>> {
    let mut st =
        conn.prepare("SELECT id, name, parent_id FROM folders ORDER BY name COLLATE NOCASE, id")?;
    let raws = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter()
        .map(|(id, name, parent)| {
            Ok(crate::Folder {
                id: FolderId(parse_uuid(&id)?),
                name,
                parent: parent.map(|s| parse_uuid(&s).map(FolderId)).transpose()?,
            })
        })
        .collect()
}

// ---------- credenciais ----------

fn check_credential(c: &Credential) -> Result<()> {
    if c.label.trim().is_empty() {
        return Err(StoreError::EmptyCredentialLabel);
    }
    Ok(())
}

pub(crate) fn insert_credential(conn: &Connection, c: &Credential) -> Result<()> {
    check_credential(c)?;
    conn.execute(
        "INSERT INTO credentials (id, label, kind, secret_ref) VALUES (?1,?2,?3,?4)",
        params![
            c.id.to_string(),
            c.label.trim(),
            kind_to_str(c.kind),
            c.secret.0
        ],
    )?;
    Ok(())
}

pub(crate) fn update_credential(conn: &Connection, c: &Credential) -> Result<()> {
    check_credential(c)?;
    let n = conn.execute(
        "UPDATE credentials SET label=?2, kind=?3, secret_ref=?4 WHERE id=?1",
        params![
            c.id.to_string(),
            c.label.trim(),
            kind_to_str(c.kind),
            c.secret.0
        ],
    )?;
    if n == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

fn credential_from_row(r: &Row<'_>) -> rusqlite::Result<(String, String, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn build_credential(raw: (String, String, String, String)) -> Result<Credential> {
    Ok(Credential {
        id: CredentialId(parse_uuid(&raw.0)?),
        label: raw.1,
        kind: kind_from_str(&raw.2)?,
        secret: SecretRef(raw.3),
    })
}

pub(crate) fn get_credential(conn: &Connection, id: CredentialId) -> Result<Option<Credential>> {
    conn.query_row(
        "SELECT id, label, kind, secret_ref FROM credentials WHERE id = ?1",
        [id.to_string()],
        credential_from_row,
    )
    .optional()?
    .map(build_credential)
    .transpose()
}

pub(crate) fn list_credentials(conn: &Connection) -> Result<Vec<Credential>> {
    let mut st = conn.prepare(
        "SELECT id, label, kind, secret_ref FROM credentials ORDER BY label COLLATE NOCASE, id",
    )?;
    let raws = st
        .query_map([], credential_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(build_credential).collect()
}

pub(crate) fn profiles_using_credential(
    conn: &Connection,
    id: CredentialId,
) -> Result<Vec<ProfileId>> {
    let mut st = conn.prepare(
        "SELECT id FROM profiles WHERE credential_id = ?1 OR passphrase_id = ?1 \
         ORDER BY name COLLATE NOCASE, id",
    )?;
    let ids = st
        .query_map([id.to_string()], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.iter().map(|s| parse_uuid(s).map(ProfileId)).collect()
}
