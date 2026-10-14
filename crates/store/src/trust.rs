//! `HostTrustStore` sobre SQLite.

use crate::{Store, StoreError};
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustStoreError, TrustVerdict};
use rusqlite::{Connection, params};
use std::time::{SystemTime, UNIX_EPOCH};

struct Row {
    key: HostKey,
    revoked: bool,
}

fn norm(e: &HostEndpoint) -> (String, u16) {
    (e.host.trim().to_ascii_lowercase(), e.port)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn rows_for(conn: &Connection, host: &str, port: u16) -> Result<Vec<Row>, StoreError> {
    let mut st = conn.prepare_cached(
        "SELECT algorithm, key_base64, fingerprint, revoked FROM host_keys \
         WHERE host = ?1 AND port = ?2 ORDER BY algorithm",
    )?;
    let rows = st
        .query_map(params![host, port], |r| {
            Ok(Row {
                key: HostKey {
                    algorithm: r.get(0)?,
                    key_base64: r.get(1)?,
                    fingerprint_sha256: r.get(2)?,
                },
                revoked: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn upsert(conn: &Connection, host: &str, port: u16, key: &HostKey) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO host_keys (host, port, algorithm, key_base64, fingerprint, created_at, revoked) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0) \
         ON CONFLICT(host, port, algorithm) DO UPDATE SET \
         key_base64 = excluded.key_base64, fingerprint = excluded.fingerprint, \
         created_at = excluded.created_at, revoked = 0",
        params![
            host,
            port,
            key.algorithm,
            key.key_base64,
            key.fingerprint_sha256,
            now()
        ],
    )?;
    Ok(())
}

impl Store {
    /// Marca como revogada a chave do algoritmo informado para o endpoint.
    /// Chaves revogadas nunca são aceitas; só `replace` as reabilita.
    pub fn revoke_host_key(
        &self,
        endpoint: &HostEndpoint,
        algorithm: &str,
    ) -> Result<(), StoreError> {
        let (host, port) = norm(endpoint);
        let n = self.lock()?.execute(
            "UPDATE host_keys SET revoked = 1 WHERE host = ?1 AND port = ?2 AND algorithm = ?3",
            params![host, port, algorithm],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

impl HostTrustStore for Store {
    fn verify(
        &self,
        endpoint: &HostEndpoint,
        key: &HostKey,
    ) -> Result<TrustVerdict, TrustStoreError> {
        let (host, port) = norm(endpoint);
        let rows = rows_for(&*self.lock()?, &host, port)?;
        if rows.is_empty() {
            return Ok(TrustVerdict::Unknown);
        }
        // Mesma chave (qualquer algoritmo registrado) revogada → Revoked.
        if rows
            .iter()
            .any(|r| r.revoked && r.key.key_base64 == key.key_base64)
        {
            return Ok(TrustVerdict::Revoked);
        }
        if let Some(same) = rows.iter().find(|r| r.key.algorithm == key.algorithm) {
            return Ok(if same.key.key_base64 == key.key_base64 {
                TrustVerdict::Trusted
            } else {
                TrustVerdict::Changed {
                    previous: same.key.clone(),
                }
            });
        }
        let known: Vec<HostKey> = rows
            .into_iter()
            .filter(|r| !r.revoked)
            .map(|r| r.key)
            .collect();
        Ok(if known.is_empty() {
            TrustVerdict::Unknown
        } else {
            TrustVerdict::OtherAlgorithmKnown { known }
        })
    }

    fn trust(&self, endpoint: &HostEndpoint, key: &HostKey) -> Result<(), TrustStoreError> {
        let (host, port) = norm(endpoint);
        let mut c = self.lock()?;
        let tx = c.transaction().map_err(StoreError::from)?;
        let rows = rows_for(&tx, &host, port)?;
        if let Some(same) = rows.iter().find(|r| r.key.algorithm == key.algorithm) {
            if same.key.key_base64 != key.key_base64 || same.revoked {
                return Err(StoreError::HostKeyConflict.into());
            }
            return Ok(()); // idempotente
        }
        upsert(&tx, &host, port, key)?;
        tx.commit().map_err(StoreError::from)?;
        Ok(())
    }

    fn replace(&self, endpoint: &HostEndpoint, key: &HostKey) -> Result<(), TrustStoreError> {
        let (host, port) = norm(endpoint);
        let c = self.lock()?;
        upsert(&c, &host, port, key)?;
        Ok(())
    }

    fn forget(&self, endpoint: &HostEndpoint) -> Result<(), TrustStoreError> {
        let (host, port) = norm(endpoint);
        self.lock()?
            .execute(
                "DELETE FROM host_keys WHERE host = ?1 AND port = ?2",
                params![host, port],
            )
            .map_err(StoreError::from)?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<(HostEndpoint, HostKey)>, TrustStoreError> {
        let c = self.lock()?;
        let mut st = c
            .prepare(
                "SELECT host, port, algorithm, key_base64, fingerprint FROM host_keys \
                 WHERE revoked = 0 ORDER BY host, port, algorithm",
            )
            .map_err(StoreError::from)?;
        let out = st
            .query_map([], |r| {
                Ok((
                    HostEndpoint {
                        host: r.get(0)?,
                        port: r.get(1)?,
                    },
                    HostKey {
                        algorithm: r.get(2)?,
                        key_base64: r.get(3)?,
                        fingerprint_sha256: r.get(4)?,
                    },
                ))
            })
            .map_err(StoreError::from)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::from)?;
        Ok(out)
    }
}
