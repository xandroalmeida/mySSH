//! Operações no disco local, sempre assíncronas (`tokio::fs`) e fora da thread de UI.

use std::path::{Path, PathBuf};

use super::contract::Kind;
use super::model::Entry;
use crate::i18n;

/// Pasta inicial local: a home do usuário (ou `/`).
pub fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR_STR))
}

fn io_text(e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound => i18n::FILES_ERR_NOT_FOUND().into(),
        K::PermissionDenied => i18n::FILES_ERR_PERMISSION().into(),
        K::AlreadyExists => i18n::FILES_ERR_EXISTS().into(),
        _ => i18n::files_err_io(&e.to_string()),
    }
}

/// Lista um diretório. Não segue symlinks (o tipo vem do `lstat`).
pub async fn list(dir: PathBuf) -> Result<Vec<Entry>, String> {
    let mut rd = tokio::fs::read_dir(&dir).await.map_err(|e| io_text(&e))?;
    let mut out = Vec::new();
    while let Some(ent) = rd.next_entry().await.map_err(|e| io_text(&e))? {
        let Ok(md) = ent.metadata().await else {
            continue; // sumiu entre a leitura e o stat
        };
        let ft = md.file_type();
        let kind = if ft.is_symlink() {
            Kind::Symlink
        } else if ft.is_dir() {
            Kind::Dir
        } else if ft.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        out.push(Entry {
            name: ent.file_name().to_string_lossy().into_owned(),
            path: ent.path().to_string_lossy().into_owned(),
            kind,
            size: if kind == Kind::File { md.len() } else { 0 },
            mtime: md.modified().ok(),
        });
    }
    Ok(out)
}

pub async fn mkdir(path: PathBuf) -> Result<(), String> {
    tokio::fs::create_dir(&path).await.map_err(|e| io_text(&e))
}

/// Renomeia sem sobrescrever: o rename do SO substituiria o destino em silêncio.
pub async fn rename(from: PathBuf, to: PathBuf) -> Result<(), String> {
    if tokio::fs::symlink_metadata(&to).await.is_ok() {
        return Err(i18n::FILES_ERR_EXISTS().into());
    }
    tokio::fs::rename(&from, &to).await.map_err(|e| io_text(&e))
}

/// Exclusão permanente. Pastas são removidas recursivamente; symlinks são removidos sem
/// seguir o destino.
pub async fn remove(path: PathBuf) -> Result<(), String> {
    let md = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|e| io_text(&e))?;
    let r = if md.file_type().is_dir() {
        tokio::fs::remove_dir_all(&path).await
    } else {
        tokio::fs::remove_file(&path).await
    };
    r.map_err(|e| io_text(&e))
}

pub fn parent(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
}

pub fn join(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lista_cria_renomeia_e_exclui() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        std::fs::write(d.join("a.txt"), b"12345").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join(".oculto"), b"").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(d.join("a.txt"), d.join("ln")).unwrap();

        let mut v = list(d.to_path_buf()).await.unwrap();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        #[cfg(unix)]
        assert_eq!(names, [".oculto", "a.txt", "ln", "sub"]);
        let a = v.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!((a.kind, a.size), (Kind::File, 5));
        assert!(a.mtime.is_some());
        assert_eq!(v.iter().find(|e| e.name == "sub").unwrap().kind, Kind::Dir);
        #[cfg(unix)]
        assert_eq!(
            v.iter().find(|e| e.name == "ln").unwrap().kind,
            Kind::Symlink
        );

        mkdir(d.join("nova")).await.unwrap();
        assert!(mkdir(d.join("nova")).await.is_err());
        rename(d.join("nova"), d.join("outra")).await.unwrap();
        // não sobrescreve um existente
        assert_eq!(
            rename(d.join("a.txt"), d.join("sub")).await.unwrap_err(),
            i18n::FILES_ERR_EXISTS()
        );
        std::fs::write(d.join("sub").join("x"), b"x").unwrap();
        remove(d.join("sub")).await.unwrap();
        remove(d.join("a.txt")).await.unwrap();
        assert!(!d.join("sub").exists() && !d.join("a.txt").exists());
        #[cfg(unix)]
        {
            // remover o link não apaga o alvo
            std::fs::write(d.join("alvo"), b"1").unwrap();
            std::os::unix::fs::symlink(d.join("alvo"), d.join("ln2")).unwrap();
            remove(d.join("ln2")).await.unwrap();
            assert!(d.join("alvo").exists());
        }
        assert!(list(d.join("naoexiste")).await.is_err());
    }

    #[test]
    fn pai_e_juncao() {
        assert_eq!(parent("/a/b").as_deref(), Some("/a"));
        assert_eq!(parent("/").as_deref(), None);
        assert_eq!(
            join("/a", "b c"),
            "/a/b c".replace('/', std::path::MAIN_SEPARATOR_STR)
        );
    }
}
