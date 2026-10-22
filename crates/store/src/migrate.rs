//! Migrações versionadas via `PRAGMA user_version`, cada uma em transação.

use crate::error::{Result, StoreError};
use rusqlite::Connection;

pub(crate) const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE folders (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL
    );
    CREATE TABLE credentials (
        id TEXT PRIMARY KEY NOT NULL,
        label TEXT NOT NULL,
        kind TEXT NOT NULL,
        secret_ref TEXT NOT NULL
    );
    CREATE TABLE profiles (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        host TEXT NOT NULL,
        port INTEGER NOT NULL,
        username TEXT NOT NULL,
        auth_json TEXT NOT NULL,
        credential_id TEXT REFERENCES credentials(id) ON DELETE RESTRICT,
        passphrase_id TEXT REFERENCES credentials(id) ON DELETE RESTRICT,
        folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
        favorite INTEGER NOT NULL,
        environment TEXT NOT NULL,
        theme_json TEXT NOT NULL,
        jump_id TEXT REFERENCES profiles(id) ON DELETE SET NULL,
        agent_forwarding INTEGER NOT NULL,
        keepalive_secs INTEGER,
        connect_timeout_secs INTEGER NOT NULL
    );
    CREATE INDEX idx_profiles_folder ON profiles(folder_id);
    CREATE INDEX idx_profiles_credential ON profiles(credential_id);
    CREATE INDEX idx_profiles_passphrase ON profiles(passphrase_id);
    CREATE TABLE profile_tags (
        profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
        tag TEXT NOT NULL COLLATE NOCASE,
        position INTEGER NOT NULL,
        PRIMARY KEY (profile_id, tag)
    );
    CREATE INDEX idx_profile_tags_tag ON profile_tags(tag);
    CREATE TABLE host_keys (
        host TEXT NOT NULL,
        port INTEGER NOT NULL,
        algorithm TEXT NOT NULL,
        key_base64 TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        revoked INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (host, port, algorithm)
    );
    CREATE TABLE settings (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    "#,
    // v2: transporte (direto ou AWS SSM), JSON de `myssh_core::Transport`.
    r#"
    ALTER TABLE profiles ADD COLUMN transport_json TEXT NOT NULL DEFAULT '"Direct"';
    "#,
];

pub(crate) fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let current = usize::try_from(current).map_err(|_| StoreError::Corrupt)?;
    if current > migrations.len() {
        return Err(StoreError::DatabaseTooNew);
    }
    for (i, sql) in migrations.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
