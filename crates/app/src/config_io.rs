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
    std::fs::write(path, json).map_err(|_| i18n::CFG_ERR_WRITE.to_string())?;
    Ok(i18n::cfg_exported(&path.display().to_string()))
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
