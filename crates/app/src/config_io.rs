//! Exportar/importar a configuração (perfis e pastas) em JSON, sem segredos.
//! Funções bloqueantes: chamar fora da thread de UI.

use std::io::Read;
use std::path::Path;

use myssh_store::{ImportReport, Store};

use crate::i18n;

/// Limite de leitura do arquivo de importação (igual ao do store).
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Grava a configuração em `path`. Devolve o aviso de sucesso.
pub fn export_to(store: &Store, path: &Path) -> Result<String, String> {
    let json = store.export_config_json().map_err(|e| e.to_string())?;
    write_private(path, json.as_bytes()).map_err(|_| i18n::CFG_ERR_WRITE.to_string())?;
    Ok(i18n::cfg_exported(&path.display().to_string()))
}

/// Grava em arquivo temporário novo (0600 em unix, `create_new`) na mesma pasta e renomeia
/// sobre o destino: o resultado final tem sempre 0600 e nunca fica meio escrito.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?;
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = (|| {
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Importa a configuração de `path` (nunca sobrescreve; IDs repetidos ganham novos IDs).
pub fn import_from(store: &Store, path: &Path) -> Result<ImportReport, String> {
    let mut text = String::new();
    let n = std::fs::File::open(path)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_string(&mut text))
        .map_err(|_| i18n::CFG_ERR_READ.to_string())?;
    if n as u64 > MAX_BYTES {
        return Err(i18n::CFG_ERR_TOO_BIG.to_string());
    }
    store.import_config_json(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_core::{AuthMethod, Profile};
    use myssh_store::{Folder, ProfileFilter};

    #[test]
    fn ida_e_volta_sem_segredos() {
        let tmp = tempfile::tempdir().unwrap();
        let a = Store::open_in_memory().unwrap();
        let folder = Folder::new("Infra", None);
        a.create_folder(&folder).unwrap();
        let mut p = Profile::new("srv", "srv.example.com", "ana");
        p.folder = Some(folder.id);
        p.auth = AuthMethod::Password { credential: None };
        p.theme.palette = Some("cyberpunk".into());
        a.create_profile(&p).unwrap();

        let path = tmp.path().join("cfg.json");
        let notice = export_to(&a, &path).unwrap();
        assert!(notice.contains("cfg.json"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("cyberpunk"));
        assert!(!text.to_lowercase().contains("secret"));

        let b = Store::open_in_memory().unwrap();
        let rep = import_from(&b, &path).unwrap();
        assert_eq!((rep.profiles, rep.folders), (1, 1));
        let got = b.list_profiles(&ProfileFilter::default()).unwrap();
        assert_eq!(got[0].theme.palette.as_deref(), Some("cyberpunk"));
        // importar de novo não sobrescreve: duplica com novos ids
        let again = import_from(&b, &path).unwrap();
        assert!(again.ids_regenerated > 0);
        assert_eq!(b.list_profiles(&ProfileFilter::default()).unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn exporta_com_0600_mesmo_sobre_arquivo_frouxo() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let path = tmp.path().join("cfg.json");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        export_to(&s, &path).unwrap();
        assert_eq!(mode(&path), 0o600);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        export_to(&s, &path).unwrap();
        assert_eq!(mode(&path), 0o600);
        // sem sobras do temporário
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn erros_em_pt_br() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        assert_eq!(
            import_from(&s, &tmp.path().join("nada.json")).unwrap_err(),
            i18n::CFG_ERR_READ
        );
        let bad = tmp.path().join("ruim.json");
        std::fs::write(&bad, "{ isto não é json").unwrap();
        assert!(import_from(&s, &bad).is_err());
        assert_eq!(
            export_to(&s, &tmp.path().join("sem/pasta/x.json")).unwrap_err(),
            i18n::CFG_ERR_WRITE
        );
    }
}
