//! Navegação remota (canal SFTP próprio do navegador; nunca o da fila).

use crate::error::TransferError;
use crate::side::{entry_from_attrs, meta_from_attrs, sftp_err};
use crate::{Kind, RemoteEntry, names};
use myssh_ssh::{Connection, Sftp};
use std::sync::Arc;

pub struct RemoteBrowser {
    sftp: Sftp,
}

pub(crate) async fn open_sftp(conn: &Arc<Connection>) -> Result<Sftp, TransferError> {
    conn.open_sftp().await.map_err(|m| {
        if conn.is_closed() || m.contains("conexão") {
            TransferError::ConnectionLost
        } else {
            TransferError::Remote(crate::error::sanitize(&m))
        }
    })
}

impl RemoteBrowser {
    pub async fn open(conn: Arc<Connection>) -> Result<Self, TransferError> {
        Ok(RemoteBrowser {
            sftp: open_sftp(&conn).await?,
        })
    }

    /// `realpath(".")`.
    pub async fn home(&self) -> Result<String, TransferError> {
        self.sftp.canonicalize(".").await.map_err(sftp_err)
    }

    /// Lista o diretório (sem `.` e `..`; nomes inválidos/traversal são descartados). Diretórios
    /// primeiro, depois por nome sem diferenciar caixa.
    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, TransferError> {
        let rd = self.sftp.read_dir(dir).await.map_err(sftp_err)?;
        let mut out: Vec<RemoteEntry> = rd
            .filter(|e| names::validate_remote_name(&e.file_name()).is_ok())
            .map(|e| entry_from_attrs(dir, &e.file_name(), &e.metadata()))
            .collect();
        out.sort_by(|a, b| {
            (a.kind != Kind::Dir)
                .cmp(&(b.kind != Kind::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(out)
    }

    /// `lstat` de um caminho (`None` se não existir).
    pub async fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, TransferError> {
        match self.sftp.symlink_metadata(path).await {
            Ok(a) => {
                let name = names::remote_basename(path).unwrap_or("/");
                let mut e = entry_from_attrs(&names::remote_dirname(path), name, &a);
                e.path = path.to_string();
                Ok(Some(e))
            }
            Err(e) if crate::side::is_no_such_file(&e) => Ok(None),
            Err(e) => Err(sftp_err(e)),
        }
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), TransferError> {
        self.sftp.create_dir(path).await.map_err(sftp_err)
    }

    /// Renomeia/move. O servidor recusa se `to` já existir (não sobrescreve).
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), TransferError> {
        self.sftp.rename(from, to).await.map_err(sftp_err)
    }

    /// Remove arquivo/link ou diretório. Diretório não vazio exige `recursive`; a recursão não
    /// segue symlinks (remove o link). A UI deve confirmar antes. Recusa a raiz.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), TransferError> {
        if path.trim_matches('/').is_empty() || path == "." || path == ".." {
            return Err(TransferError::OutsideRoot(names::display_name(path)));
        }
        let meta = self.sftp.symlink_metadata(path).await.map_err(sftp_err)?;
        if meta_from_attrs(&meta).kind != Kind::Dir {
            return self.sftp.remove_file(path).await.map_err(sftp_err);
        }
        if !recursive {
            return self.sftp.remove_dir(path).await.map_err(sftp_err);
        }
        // Pré-ordem iterativa; remove em ordem inversa (filhos antes dos pais).
        let mut order: Vec<(String, bool)> = vec![(path.to_string(), true)];
        let mut stack = vec![path.to_string()];
        while let Some(dir) = stack.pop() {
            let rd = self.sftp.read_dir(dir.as_str()).await.map_err(sftp_err)?;
            for e in rd {
                let name = e.file_name();
                if names::validate_remote_name(&name).is_err() {
                    continue;
                }
                let child = names::remote_join(&dir, &name);
                let is_dir = meta_from_attrs(&e.metadata()).kind == Kind::Dir;
                order.push((child.clone(), is_dir));
                if is_dir {
                    stack.push(child);
                }
            }
        }
        for (p, is_dir) in order.into_iter().rev() {
            if is_dir {
                self.sftp.remove_dir(p).await.map_err(sftp_err)?;
            } else {
                self.sftp.remove_file(p).await.map_err(sftp_err)?;
            }
        }
        Ok(())
    }
}
