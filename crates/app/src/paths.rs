//! Diretórios e arquivo do banco, com permissões restritas.

use std::path::{Path, PathBuf};

pub const DB_FILE: &str = "myssh.db";

/// `MYSSH_DATA_DIR` (demo/testes) ou o diretório de dados do usuário.
/// Retorna também se o diretório foi sobrescrito por env.
pub fn data_dir() -> Option<(PathBuf, bool)> {
    if let Some(d) = std::env::var_os("MYSSH_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some((PathBuf::from(d), true));
    }
    directories::ProjectDirs::from("com", "myssh", "mySSH")
        .map(|p| (p.data_dir().to_path_buf(), false))
}

/// `~/.ssh`, se existir (ponto de partida do seletor de chave).
pub fn ssh_dir() -> Option<PathBuf> {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".ssh"))
        .filter(|d| d.is_dir())
}

/// Cria o diretório (0700 em unix) e o arquivo do banco (0600 em unix) se não existirem.
/// Em unix, também aperta permissões frouxas pré-existentes do diretório, do banco e dos
/// arquivos auxiliares do SQLite (`-wal`, `-shm`, `-journal`).
pub fn prepare_db(dir: &Path) -> std::io::Result<PathBuf> {
    create_dir_private(dir)?;
    let db = dir.join(DB_FILE);
    create_file_private(&db)?;
    tighten_permissions(dir, &db)?;
    Ok(db)
}

#[cfg(unix)]
fn tighten_permissions(dir: &Path, db: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fn chmod_if_needed(path: &Path, mode: u32) -> std::io::Result<()> {
        let meta = std::fs::metadata(path)?;
        if meta.permissions().mode() & 0o777 != mode {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
        }
        Ok(())
    }
    chmod_if_needed(dir, 0o700)?;
    chmod_if_needed(db, 0o600)?;
    let name = db.file_name().unwrap_or_default().to_string_lossy();
    for suffix in ["-wal", "-shm", "-journal"] {
        let aux = dir.join(format!("{name}{suffix}"));
        if aux.exists() {
            chmod_if_needed(&aux, 0o600)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten_permissions(_dir: &Path, _db: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_file_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    // `create` (sem `truncate`) preserva um banco existente; o modo só vale na criação.
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map(|_| ())
}

#[cfg(not(unix))]
fn create_file_private(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cria_banco_e_preserva_existente() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("a/b");
        let db = prepare_db(&dir).unwrap();
        assert!(db.exists());
        std::fs::write(&db, b"x").unwrap();
        prepare_db(&dir).unwrap();
        assert_eq!(std::fs::read(&db).unwrap(), b"x");
    }

    #[cfg(unix)]
    #[test]
    fn aperta_permissoes_frouxas_preexistentes() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("dados");
        std::fs::create_dir(&dir).unwrap();
        let set = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        set(&dir, 0o755);
        let files: Vec<_> = [
            "myssh.db",
            "myssh.db-wal",
            "myssh.db-shm",
            "myssh.db-journal",
        ]
        .iter()
        .map(|n| {
            let f = dir.join(n);
            std::fs::write(&f, b"x").unwrap();
            set(&f, 0o644);
            f
        })
        .collect();
        prepare_db(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        for f in files {
            assert_eq!(mode(&f), 0o600, "{}", f.display());
        }
        // conteúdo preservado
        assert_eq!(std::fs::read(dir.join("myssh.db")).unwrap(), b"x");
    }

    #[cfg(unix)]
    #[test]
    fn permissoes_restritas() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("dados");
        let db = prepare_db(&dir).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&db), 0o600);
        assert_eq!(mode(&dir), 0o700);
    }
}
