//! Importação de `~/.ssh/config`: leitura (somente leitura, nunca escreve no arquivo do usuário),
//! seleção de candidatos, detecção de conflitos e gravação no store.

use std::io::Read;
use std::path::{Path, PathBuf};

use myssh_core::{FolderId, Profile, ProfileId};
use myssh_sshconfig::{
    Candidate, DiskFs, ImportOptions, ImportPreview, MAX_FILE_BYTES, Warning, parse_config,
    resolve_jumps,
};
use myssh_store::{Folder, Store};

use crate::i18n;

/// Nome da pasta de destino.

#[derive(Debug, Clone)]
pub struct Item {
    pub cand: Candidate,
    pub selected: bool,
    /// Perfil existente com mesmo host + usuário + porta.
    pub conflict: Option<ProfileId>,
    /// Perfil inválido (ex.: host vazio): não pode ser importado.
    pub invalid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Preview {
    pub items: Vec<Item>,
    pub global_warnings: Vec<Warning>,
}

/// O aviso trata de diretiva que executaria comando local (ignorada pelo importador).
pub fn is_local_command_warning(w: &Warning) -> bool {
    w.msg.contains("executa comando local")
}

/// Perfil existente equivalente (mesmo host, usuário e porta; host sem diferenciar maiúsculas).
pub fn find_conflict(existing: &[Profile], p: &Profile) -> Option<ProfileId> {
    existing
        .iter()
        .find(|e| {
            e.port == p.port
                && e.username == p.username
                && e.host.trim().eq_ignore_ascii_case(p.host.trim())
        })
        .map(|e| e.id)
}

/// Monta a prévia: resolve saltos, marca conflitos (desmarcados por padrão) e inválidos.
pub fn build_preview(mut raw: ImportPreview, existing: &[Profile]) -> Preview {
    resolve_jumps(&mut raw.candidates);
    let items = raw
        .candidates
        .into_iter()
        .map(|cand| {
            let conflict = find_conflict(existing, &cand.profile);
            let invalid = cand.profile.validate().is_err();
            Item {
                selected: conflict.is_none() && !invalid,
                conflict,
                invalid,
                cand,
            }
        })
        .collect();
    Preview {
        items,
        global_warnings: raw.global_warnings,
    }
}

impl Preview {
    pub fn selected_count(&self) -> usize {
        self.items.iter().filter(|i| i.selected).count()
    }

    pub fn conflict_count(&self) -> usize {
        self.items.iter().filter(|i| i.conflict.is_some()).count()
    }

    /// Marca todos os importáveis (`include_conflicts` = inclui os "já existe").
    pub fn select_all(&mut self, include_conflicts: bool) {
        for i in &mut self.items {
            i.selected = !i.invalid && (include_conflicts || i.conflict.is_none());
        }
    }

    pub fn select_none(&mut self) {
        for i in &mut self.items {
            i.selected = false;
        }
    }

    /// Perfis a gravar, em ordem segura para chaves estrangeiras (saltos antes de quem os usa).
    /// Salto para candidato não selecionado: aponta para o perfil existente equivalente, se
    /// houver; senão é removido.
    pub fn prepare(&self) -> Vec<Profile> {
        let chosen: Vec<&Item> = self
            .items
            .iter()
            .filter(|i| i.selected && !i.invalid)
            .collect();
        let mut profiles: Vec<Profile> = chosen.iter().map(|i| i.cand.profile.clone()).collect();
        for p in &mut profiles {
            let Some(j) = p.jump else { continue };
            if chosen.iter().any(|c| c.cand.profile.id == j) {
                continue;
            }
            p.jump = self
                .items
                .iter()
                .find(|i| i.cand.profile.id == j)
                .and_then(|i| i.conflict);
        }
        // Ordem topológica: quem tem salto vem depois do salto (dentro do lote).
        let mut ordered: Vec<Profile> = Vec::with_capacity(profiles.len());
        let mut pending = profiles;
        while !pending.is_empty() {
            let ready: Vec<usize> = (0..pending.len())
                .filter(|&i| match pending[i].jump {
                    None => true,
                    Some(j) => {
                        ordered.iter().any(|o| o.id == j) || !pending.iter().any(|p| p.id == j)
                    }
                })
                .collect();
            if ready.is_empty() {
                // ciclo (não deveria ocorrer: `resolve_jumps` já o remove): corta o salto
                pending[0].jump = None;
                continue;
            }
            for &i in ready.iter().rev() {
                ordered.insert(ordered.len(), pending.remove(i));
            }
            // `rev` inverteu a ordem relativa dos prontos; restaura a do arquivo
            let n = ready.len();
            let l = ordered.len();
            ordered[l - n..].reverse();
        }
        ordered
    }
}

fn home_and_user() -> Option<(String, String)> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("HOME").or_else(|| env("USERPROFILE"))?;
    let user = env("USER").or_else(|| env("USERNAME")).unwrap_or_default();
    Some((home, user))
}

/// Caminho padrão do arquivo (`~/.ssh/config`).
pub fn default_path() -> Option<PathBuf> {
    home_and_user().map(|(h, _)| Path::new(&h).join(".ssh").join("config"))
}

/// Lê e interpreta o arquivo (somente leitura; até 1 MiB). `home`/`user` servem a `~`, `%d`,
/// `%u` e `Include` relativos.
pub fn load(path: &Path, home: &str, user: &str) -> Result<ImportPreview, String> {
    let mut content = String::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES).read_to_string(&mut content))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => i18n::IMPORT_ERR_NOT_FOUND().to_string(),
            std::io::ErrorKind::PermissionDenied => i18n::IMPORT_ERR_DENIED().to_string(),
            std::io::ErrorKind::InvalidData => i18n::IMPORT_ERR_NOT_TEXT().to_string(),
            _ => i18n::IMPORT_ERR_READ().to_string(),
        })?;
    let mut opts = ImportOptions::new(home, user);
    opts.root_name = path.to_string_lossy().into_owned();
    Ok(parse_config(&content, &opts, &DiskFs))
}

/// Como [`load`], com o home e o usuário do ambiente.
pub fn load_with_env(path: &Path) -> Result<ImportPreview, String> {
    let (home, user) = home_and_user().ok_or_else(|| i18n::IMPORT_ERR_NO_HOME().to_string())?;
    load(path, &home, &user)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    pub imported: usize,
    pub folder: FolderId,
    pub created_folder: bool,
}

/// Grava os perfis na pasta "Importados" (criada se necessário). Desfaz tudo se algum falhar.
pub fn save(store: &Store, profiles: Vec<Profile>) -> Result<SaveReport, String> {
    let fail = |e: myssh_store::StoreError| e.to_string();
    let folders = store.list_folders().map_err(fail)?;
    let (folder, created_folder) = match folders
        .iter()
        .find(|f| f.parent.is_none() && f.name.trim().eq_ignore_ascii_case(i18n::IMPORT_FOLDER()))
    {
        Some(f) => (f.id, false),
        None => {
            let f = Folder::new(i18n::IMPORT_FOLDER(), None);
            store.create_folder(&f).map_err(fail)?;
            (f.id, true)
        }
    };
    let mut done: Vec<ProfileId> = Vec::new();
    for mut p in profiles {
        p.folder = Some(folder);
        let id = p.id;
        if let Err(e) = store.create_profile(&p) {
            for id in done.iter().rev() {
                let _ = store.delete_profile(*id);
            }
            if created_folder {
                let _ = store.delete_folder(folder);
            }
            return Err(e.to_string());
        }
        done.push(id);
    }
    Ok(SaveReport {
        imported: done.len(),
        folder,
        created_folder,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_core::AuthMethod;
    use myssh_store::ProfileFilter;

    fn write(dir: &Path, rel: &str, content: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
        p
    }

    const CONFIG: &str = "\
Host *
    ServerAliveInterval 45

Host bastion
    HostName bastion.example.com
    User ops
    IdentityFile ~/.ssh/id_bastion

Host web
    HostName web.example.com
    User deploy
    Port 2200
    ProxyJump bastion

Host evil
    HostName evil.example.com
    ProxyCommand nc %h %p

Host dup
    HostName DUP.example.com
    User ana
";

    fn preview_from(dir: &Path, existing: &[Profile]) -> Preview {
        let path = write(dir, ".ssh/config", CONFIG);
        let raw = load(&path, &dir.to_string_lossy(), "ana").unwrap();
        build_preview(raw, existing)
    }

    #[test]
    fn candidatos_avisos_e_selecao_padrao() {
        let tmp = tempfile::tempdir().unwrap();
        let p = preview_from(tmp.path(), &[]);
        let names: Vec<_> = p.items.iter().map(|i| i.cand.alias.as_str()).collect();
        assert_eq!(names, ["bastion", "web", "evil", "dup"]);
        assert_eq!(p.selected_count(), 4);
        let evil = &p.items[2];
        assert!(evil.cand.warnings.iter().any(is_local_command_warning));
        let web = &p.items[1];
        assert_eq!(web.cand.profile.jump, Some(p.items[0].cand.profile.id));
        assert_eq!(web.cand.profile.port, 2200);
        assert_eq!(web.cand.profile.keepalive_secs, Some(45)); // herdado de Host *
        assert!(matches!(
            p.items[0].cand.profile.auth,
            AuthMethod::KeyFile { .. }
        ));
    }

    #[test]
    fn conflito_exige_host_usuario_e_porta() {
        let tmp = tempfile::tempdir().unwrap();
        let mut existing = Profile::new("meu dup", "dup.example.com", "ana");
        let p = preview_from(tmp.path(), std::slice::from_ref(&existing));
        let dup = &p.items[3];
        assert_eq!(dup.conflict, Some(existing.id));
        assert!(!dup.selected, "conflito vem desmarcado");
        assert_eq!(p.conflict_count(), 1);
        assert_eq!(p.selected_count(), 3);

        // outra porta ou outro usuário não é conflito
        existing.port = 2222;
        assert_eq!(find_conflict(&[existing.clone()], &dup.cand.profile), None);
        existing.port = 22;
        existing.username = "root".into();
        assert_eq!(find_conflict(&[existing], &dup.cand.profile), None);
    }

    #[test]
    fn selecao_em_massa_e_preparo_remove_saltos_orfaos() {
        let tmp = tempfile::tempdir().unwrap();
        let mut p = preview_from(tmp.path(), &[]);
        p.select_none();
        assert_eq!(p.selected_count(), 0);
        assert!(p.prepare().is_empty());
        // só "web" (o salto "bastion" fica de fora): salto removido
        p.items[1].selected = true;
        let out = p.prepare();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].jump, None);
        // com o salto selecionado: ele vem antes
        p.items[0].selected = true;
        p.items[1].selected = true;
        let out = p.prepare();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "bastion");
        assert_eq!(out[1].jump, Some(out[0].id));
    }

    #[test]
    fn salto_para_perfil_ja_existente_aponta_para_ele() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = Profile::new("bastion antigo", "bastion.example.com", "ops");
        let mut p = preview_from(tmp.path(), std::slice::from_ref(&existing));
        assert!(!p.items[0].selected);
        p.items[1].selected = true;
        let out = p.prepare();
        assert_eq!(out[0].jump, Some(existing.id));
        // "selecionar todos" sem conflitos mantém o conflito de fora; com conflitos inclui
        p.select_all(false);
        assert!(!p.items[0].selected);
        p.select_all(true);
        assert!(p.items[0].selected);
    }

    #[test]
    fn grava_na_pasta_importados_com_saltos_e_sem_tocar_no_arquivo() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write(tmp.path(), ".ssh/config", CONFIG);
        let before = std::fs::read(&cfg).unwrap();
        let store = Store::open_in_memory().unwrap();
        let p = preview_from(tmp.path(), &[]);
        let rep = save(&store, p.prepare()).unwrap();
        assert_eq!(rep.imported, 4);
        assert!(rep.created_folder);
        let saved = store.list_profiles(&ProfileFilter::default()).unwrap();
        assert_eq!(saved.len(), 4);
        assert!(saved.iter().all(|s| s.folder == Some(rep.folder)));
        let web = saved.iter().find(|s| s.name == "web").unwrap();
        let bastion = saved.iter().find(|s| s.name == "bastion").unwrap();
        assert_eq!(web.jump, Some(bastion.id));
        assert_eq!(store.list_folders().unwrap()[0].name, i18n::IMPORT_FOLDER());
        assert_eq!(
            std::fs::read(&cfg).unwrap(),
            before,
            "arquivo do usuário intacto"
        );

        // segunda importação reaproveita a pasta
        let again = save(&store, vec![Profile::new("outro", "o.example.com", "u")]).unwrap();
        assert!(!again.created_folder);
        assert_eq!(store.list_folders().unwrap().len(), 1);
    }

    #[test]
    fn falha_ao_gravar_desfaz_tudo() {
        let store = Store::open_in_memory().unwrap();
        let a = Profile::new("a", "a.example.com", "u");
        let mut b = Profile::new("b", "b.example.com", "u");
        b.id = a.id; // id repetido: a segunda gravação falha
        assert!(save(&store, vec![a, b]).is_err());
        assert!(
            store
                .list_profiles(&ProfileFilter::default())
                .unwrap()
                .is_empty()
        );
        assert!(store.list_folders().unwrap().is_empty());
    }

    #[test]
    fn erros_de_leitura_em_pt_br() {
        let tmp = tempfile::tempdir().unwrap();
        let e = load(&tmp.path().join("nao-existe"), "/h", "u").unwrap_err();
        assert_eq!(e, i18n::IMPORT_ERR_NOT_FOUND());
        let bin = tmp.path().join("bin");
        std::fs::write(&bin, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(
            load(&bin, "/h", "u").unwrap_err(),
            i18n::IMPORT_ERR_NOT_TEXT()
        );
    }
}
