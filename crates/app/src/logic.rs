//! Lógica pura (sem UI): conexão rápida, filtros, agrupamento e regras de credencial.

use myssh_core::{AuthMethod, CredentialId, Environment, FolderId, Profile};
use myssh_secrets::SecretRef;
use myssh_store::{Folder, Store};

use crate::i18n;

// ------------------------------------------------------------ conexão rápida

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickError {
    Empty,
    MissingUser,
    MissingHost,
    InvalidPort,
}

impl std::fmt::Display for QuickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => i18n::QUICK_ERR_EMPTY(),
            Self::MissingUser => i18n::QUICK_ERR_USER(),
            Self::MissingHost => i18n::QUICK_ERR_HOST(),
            Self::InvalidPort => i18n::ERR_PORT(),
        })
    }
}

/// Interpreta `usuário@host[:porta]`. IPv6 deve vir entre colchetes para ter porta
/// (`u@[::1]:2222`); IPv6 sem colchetes é tomado inteiro como host, na porta 22.
pub fn parse_quick(input: &str) -> Result<QuickTarget, QuickError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(QuickError::Empty);
    }
    let (user, rest) = s.rsplit_once('@').ok_or(QuickError::MissingUser)?;
    let user = user.trim();
    if user.is_empty() || user.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(QuickError::MissingUser);
    }
    let rest = rest.trim();
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        let (h, after) = inner.split_once(']').ok_or(QuickError::MissingHost)?;
        match after {
            "" => (h, None),
            a => (h, Some(a.strip_prefix(':').ok_or(QuickError::MissingHost)?)),
        }
    } else if rest.matches(':').count() == 1 {
        let (h, p) = rest.split_once(':').unwrap_or((rest, ""));
        (h, Some(p))
    } else {
        (rest, None)
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(QuickError::MissingHost);
    }
    let port = match port {
        None => 22,
        Some(p) => p
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or(QuickError::InvalidPort)?,
    };
    Ok(QuickTarget {
        user: user.to_string(),
        host: host.to_string(),
        port,
    })
}

/// Perfil transitório (não salvo) para uma conexão rápida: senha sempre perguntada.
pub fn quick_profile(t: &QuickTarget) -> Profile {
    let mut p = Profile::new(format!("{}@{}", t.user, t.host), &t.host, &t.user);
    p.port = t.port;
    p
}

// -------------------------------------------------------------------- filtros

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderSel {
    #[default]
    All,
    /// Somente perfis sem pasta.
    Unfiled,
    Folder(FolderId),
}

#[derive(Debug, Clone, Default)]
pub struct ViewFilter {
    pub search: String,
    pub favorites_only: bool,
    pub environment: Option<Environment>,
    pub folder: FolderSel,
}

impl ViewFilter {
    pub fn is_active(&self) -> bool {
        !self.search.trim().is_empty()
            || self.favorites_only
            || self.environment.is_some()
            || self.folder != FolderSel::All
    }
}

/// Busca por nome, host, usuário ou tag (sem diferenciar caixa) + filtros combinados com E.
pub fn matches(p: &Profile, f: &ViewFilter) -> bool {
    if f.favorites_only && !p.favorite {
        return false;
    }
    if f.environment.is_some_and(|e| p.environment != e) {
        return false;
    }
    match f.folder {
        FolderSel::All => {}
        FolderSel::Unfiled => {
            if p.folder.is_some() {
                return false;
            }
        }
        FolderSel::Folder(id) => {
            if p.folder != Some(id) {
                return false;
            }
        }
    }
    let n = f.search.trim().to_lowercase();
    if n.is_empty() {
        return true;
    }
    p.name.to_lowercase().contains(&n)
        || p.host.to_lowercase().contains(&n)
        || p.username.to_lowercase().contains(&n)
        || p.tags.iter().any(|t| t.to_lowercase().contains(&n))
}

/// Caminho legível da pasta ("Raiz / Filha"); protege contra ciclos.
pub fn folder_path(folders: &[Folder], id: FolderId) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        if parts.len() > folders.len() {
            break;
        }
        match folders.iter().find(|f| f.id == c) {
            Some(f) => {
                parts.push(f.name.clone());
                cur = f.parent;
            }
            None => break,
        }
    }
    parts.reverse();
    parts.join(" / ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub folder: Option<FolderId>,
    pub label: String,
    /// Índices em `profiles`.
    pub items: Vec<usize>,
}

/// Agrupa por pasta (ordem alfabética do caminho); "Sem pasta" por último.
/// Favoritos primeiro dentro de cada grupo; depois por nome. Grupos vazios são omitidos.
pub fn group_by_folder(profiles: &[&Profile], folders: &[Folder]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, p) in profiles.iter().enumerate() {
        let key = p.folder.filter(|f| folders.iter().any(|x| x.id == *f));
        match groups.iter_mut().find(|g| g.folder == key) {
            Some(g) => g.items.push(i),
            None => groups.push(Group {
                folder: key,
                label: key.map_or_else(
                    || i18n::NO_FOLDER().to_string(),
                    |id| folder_path(folders, id),
                ),
                items: vec![i],
            }),
        }
    }
    for g in &mut groups {
        g.items.sort_by(|&a, &b| {
            profiles[b]
                .favorite
                .cmp(&profiles[a].favorite)
                .then_with(|| {
                    profiles[a]
                        .name
                        .to_lowercase()
                        .cmp(&profiles[b].name.to_lowercase())
                })
        });
    }
    groups.sort_by(|a, b| {
        (a.folder.is_none(), a.label.to_lowercase())
            .cmp(&(b.folder.is_none(), b.label.to_lowercase()))
    });
    groups
}

// --------------------------------------------------------------- perfis/tags

pub fn parse_tags(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in s.split([',', ';']) {
        let t = t.trim();
        if !t.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            out.push(t.to_string());
        }
    }
    out
}

pub fn duplicate_profile(p: &Profile) -> Profile {
    let mut d = p.clone();
    d.id = myssh_core::ProfileId::new();
    d.name = format!("{} {}", p.name, i18n::COPY_SUFFIX());
    d
}

// ---------------------------------------------------------------- credenciais

/// Credenciais referenciadas por um método de autenticação.
pub fn credential_ids(auth: &AuthMethod) -> Vec<CredentialId> {
    match auth {
        AuthMethod::Password { credential } => credential.iter().copied().collect(),
        AuthMethod::KeyFile { passphrase, .. } => passphrase.iter().copied().collect(),
        AuthMethod::Agent | AuthMethod::KeyboardInteractive => Vec::new(),
    }
}

/// Credenciais do perfil antigo que o novo não referencia mais.
pub fn obsolete_credentials(old: &AuthMethod, new: &AuthMethod) -> Vec<CredentialId> {
    let keep = credential_ids(new);
    credential_ids(old)
        .into_iter()
        .filter(|c| !keep.contains(c))
        .collect()
}

/// Remove a credencial do store se nenhum perfil a usa. Devolve a referência do segredo
/// que o chamador deve apagar do cofre (fora da thread de UI); `None` se foi mantida
/// (`CredentialInUse`), não existe ou houve erro.
pub fn release_credential(store: &Store, id: CredentialId) -> Option<SecretRef> {
    let cred = store.get_credential(id).ok().flatten()?;
    match store.delete_credential(id) {
        Ok(()) => Some(cred.secret),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_core::{Credential, CredentialKind};

    fn q(s: &str) -> Result<(String, String, u16), QuickError> {
        parse_quick(s).map(|t| (t.user, t.host, t.port))
    }

    #[test]
    fn quick_basico() {
        assert_eq!(
            q("ana@srv.local"),
            Ok(("ana".into(), "srv.local".into(), 22))
        );
        assert_eq!(q(" ana@srv:2222 "), Ok(("ana".into(), "srv".into(), 2222)));
        assert_eq!(q("a@b@host:1"), Ok(("a@b".into(), "host".into(), 1)));
    }

    #[test]
    fn quick_ipv6() {
        assert_eq!(q("u@[::1]:2200"), Ok(("u".into(), "::1".into(), 2200)));
        assert_eq!(q("u@[fe80::1]"), Ok(("u".into(), "fe80::1".into(), 22)));
        assert_eq!(q("u@fe80::1"), Ok(("u".into(), "fe80::1".into(), 22)));
        assert_eq!(q("u@[::1"), Err(QuickError::MissingHost));
        assert_eq!(q("u@[::1]x"), Err(QuickError::MissingHost));
    }

    #[test]
    fn quick_erros() {
        assert_eq!(q(""), Err(QuickError::Empty));
        assert_eq!(q("   "), Err(QuickError::Empty));
        assert_eq!(q("host"), Err(QuickError::MissingUser));
        assert_eq!(q("@host"), Err(QuickError::MissingUser));
        assert_eq!(q("u@"), Err(QuickError::MissingHost));
        assert_eq!(q("u@host:"), Err(QuickError::InvalidPort));
        assert_eq!(q("u@host:0"), Err(QuickError::InvalidPort));
        assert_eq!(q("u@host:70000"), Err(QuickError::InvalidPort));
        assert_eq!(q("u@host:abc"), Err(QuickError::InvalidPort));
        assert_eq!(q("u x@host"), Err(QuickError::MissingUser));
    }

    #[test]
    fn quick_profile_valido_e_sem_segredo() {
        let p = quick_profile(&parse_quick("ana@srv:2222").unwrap());
        assert!(p.validate().is_ok());
        assert_eq!(p.port, 2222);
        assert_eq!(p.auth, AuthMethod::Password { credential: None });
    }

    fn prof(name: &str, host: &str, user: &str, tags: &[&str]) -> Profile {
        let mut p = Profile::new(name, host, user);
        p.tags = tags.iter().map(|t| t.to_string()).collect();
        p
    }

    #[test]
    fn filtros_combinam_com_e() {
        let folder = FolderId::new();
        let mut a = prof("Web Prod", "web.example.com", "deploy", &["Frontend"]);
        a.favorite = true;
        a.environment = Environment::Production;
        a.folder = Some(folder);
        let b = prof("DB", "10.0.0.5", "postgres", &[]);

        let f = |s: &str| ViewFilter {
            search: s.into(),
            ..Default::default()
        };
        assert!(matches(&a, &f("")));
        assert!(matches(&a, &f("WEB")));
        assert!(matches(&a, &f("example")));
        assert!(matches(&a, &f("DEPLOY")));
        assert!(matches(&a, &f("front")));
        assert!(!matches(&b, &f("front")));
        assert!(matches(&b, &f("10.0")));

        let fav = ViewFilter {
            favorites_only: true,
            ..Default::default()
        };
        assert!(matches(&a, &fav) && !matches(&b, &fav));
        let env = ViewFilter {
            environment: Some(Environment::Production),
            ..Default::default()
        };
        assert!(matches(&a, &env) && !matches(&b, &env));
        let fo = ViewFilter {
            folder: FolderSel::Folder(folder),
            ..Default::default()
        };
        assert!(matches(&a, &fo) && !matches(&b, &fo));
        let un = ViewFilter {
            folder: FolderSel::Unfiled,
            ..Default::default()
        };
        assert!(!matches(&a, &un) && matches(&b, &un));
        let comb = ViewFilter {
            search: "web".into(),
            favorites_only: true,
            environment: Some(Environment::Staging),
            folder: FolderSel::All,
        };
        assert!(!matches(&a, &comb));
        assert!(comb.is_active() && !ViewFilter::default().is_active());
    }

    #[test]
    fn agrupamento_por_pasta() {
        let root = Folder::new("Raiz", None);
        let child = Folder::new("Filha", Some(root.id));
        let folders = vec![child.clone(), root.clone()];
        assert_eq!(folder_path(&folders, child.id), "Raiz / Filha");

        let mut a = prof("zeta", "h", "u", &[]);
        a.folder = Some(root.id);
        let mut b = prof("alfa", "h", "u", &[]);
        b.folder = Some(root.id);
        let mut fav = prof("zz-fav", "h", "u", &[]);
        fav.folder = Some(root.id);
        fav.favorite = true;
        let mut c = prof("c", "h", "u", &[]);
        c.folder = Some(child.id);
        let loose = prof("solto", "h", "u", &[]);
        let mut orphan = prof("orfao", "h", "u", &[]);
        orphan.folder = Some(FolderId::new());

        let ps = [&a, &b, &fav, &c, &loose, &orphan];
        let g = group_by_folder(&ps, &folders);
        let labels: Vec<_> = g.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Raiz", "Raiz / Filha", i18n::NO_FOLDER()]);
        // favorito primeiro, depois por nome
        assert_eq!(g[0].items, vec![2, 1, 0]);
        // pasta inexistente cai em "sem pasta"
        assert_eq!(g[2].items, vec![5, 4]);
    }

    #[test]
    fn tags_normalizadas() {
        assert_eq!(parse_tags(" a, b ;A,, c "), ["a", "b", "c"]);
        assert!(parse_tags("  ").is_empty());
    }

    #[test]
    fn duplicar_gera_novo_id_e_nome() {
        let p = prof("X", "h", "u", &["t"]);
        let d = duplicate_profile(&p);
        assert_ne!(d.id, p.id);
        assert_eq!(d.name, format!("X {}", i18n::COPY_SUFFIX()));
        assert_eq!(d.tags, p.tags);
    }

    fn cred(kind: CredentialKind) -> Credential {
        Credential {
            id: CredentialId::new(),
            label: "c".into(),
            kind,
            secret: myssh_secrets::new_secret_ref(),
        }
    }

    #[test]
    fn credenciais_obsoletas() {
        let c1 = CredentialId::new();
        let c2 = CredentialId::new();
        let pw = |c| AuthMethod::Password { credential: c };
        assert_eq!(obsolete_credentials(&pw(Some(c1)), &pw(Some(c1))), vec![]);
        assert_eq!(obsolete_credentials(&pw(Some(c1)), &pw(Some(c2))), vec![c1]);
        assert_eq!(
            obsolete_credentials(&pw(Some(c1)), &AuthMethod::Agent),
            vec![c1]
        );
        assert_eq!(
            obsolete_credentials(&AuthMethod::Agent, &pw(Some(c1))),
            vec![]
        );
    }

    #[test]
    fn liberar_credencial_respeita_uso() {
        let store = Store::open_in_memory().unwrap();
        let c = cred(CredentialKind::Password);
        store.create_credential(&c).unwrap();
        let mut p1 = Profile::new("a", "h", "u");
        p1.auth = AuthMethod::Password {
            credential: Some(c.id),
        };
        let mut p2 = duplicate_profile(&p1);
        p2.name = "b".into();
        store.create_profile(&p1).unwrap();
        store.create_profile(&p2).unwrap();

        // em uso por outro perfil: mantém
        store.delete_profile(p1.id).unwrap();
        assert_eq!(release_credential(&store, c.id), None);
        assert!(store.get_credential(c.id).unwrap().is_some());

        // último uso: remove e devolve a referência do segredo
        store.delete_profile(p2.id).unwrap();
        assert_eq!(release_credential(&store, c.id), Some(c.secret.clone()));
        assert!(store.get_credential(c.id).unwrap().is_none());
        // inexistente
        assert_eq!(release_credential(&store, c.id), None);
    }
}
