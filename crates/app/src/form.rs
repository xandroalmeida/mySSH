//! Formulário de perfil ↔ `Profile` (sem UI) e regras de criação de credenciais.

use myssh_core::{
    AuthMethod, Credential, CredentialId, CredentialKind, Environment, FolderId, Profile,
    ProfileError, ProfileId, ThemeOverride,
};
use myssh_secrets::{SecretString, new_secret_ref};
use myssh_store::Folder;
use zeroize::Zeroize;

use crate::i18n;
use crate::logic::{obsolete_credentials, parse_tags};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    Password,
    Key,
    Agent,
    KeyboardInteractive,
}

/// Onde fica o segredo (senha/passphrase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretMode {
    /// Não persiste; o app pergunta ao conectar.
    Ask,
    /// Guarda no cofre.
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormError {
    Profile(ProfileError),
    Port,
    Keepalive,
    Timeout,
    KeyPath,
    /// Modo "salvar" sem valor digitado e sem segredo já salvo.
    SecretRequired,
}

impl std::fmt::Display for FormError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Profile(e) => e.fmt(f),
            Self::Port => f.write_str(i18n::ERR_PORT),
            Self::Keepalive => f.write_str(i18n::ERR_KEEPALIVE),
            Self::Timeout => f.write_str(i18n::ERR_TIMEOUT),
            Self::KeyPath => f.write_str(i18n::ERR_KEY_PATH),
            Self::SecretRequired => f.write_str(i18n::ERR_SECRET_REQUIRED),
        }
    }
}

impl From<ProfileError> for FormError {
    fn from(e: ProfileError) -> Self {
        Self::Profile(e)
    }
}

/// Segredo novo a gravar no cofre, com os metadados da credencial correspondente.
#[derive(Clone)]
pub struct NewSecret {
    pub credential: Credential,
    pub secret: SecretString,
}

impl std::fmt::Debug for NewSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewSecret")
            .field("credential", &self.credential.id)
            .finish_non_exhaustive()
    }
}

/// Resultado de validar o formulário: tudo o que precisa ser gravado.
#[derive(Debug, Clone)]
pub struct Draft {
    pub profile: Profile,
    pub new_secret: Option<NewSecret>,
    /// Credenciais antigas que o perfil deixou de usar (liberar se ninguém mais usa).
    pub obsolete: Vec<CredentialId>,
    /// Pasta a criar antes do perfil.
    pub new_folder: Option<Folder>,
}

#[derive(Clone)]
pub struct ProfileForm {
    /// Perfil original (edição); preserva id, tema e campos não editáveis aqui.
    pub existing: Option<Profile>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth: AuthKind,
    pub password_mode: SecretMode,
    pub password: String,
    pub key_path: String,
    pub passphrase_mode: SecretMode,
    pub passphrase: String,
    pub folder: Option<FolderId>,
    pub new_folder: String,
    pub tags: String,
    pub favorite: bool,
    pub environment: Environment,
    pub jump: Option<ProfileId>,
    /// Tema do perfil (cada campo `None` herda o global).
    pub theme: ThemeOverride,
    pub keepalive: String,
    pub timeout: String,
}

impl std::fmt::Debug for ProfileForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileForm")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Drop for ProfileForm {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl Default for ProfileForm {
    fn default() -> Self {
        Self::blank()
    }
}

impl ProfileForm {
    pub fn blank() -> Self {
        let d = Profile::new("", "", "");
        Self {
            existing: None,
            name: String::new(),
            host: String::new(),
            port: d.port.to_string(),
            username: String::new(),
            auth: AuthKind::Password,
            password_mode: SecretMode::Ask,
            password: String::new(),
            key_path: String::new(),
            passphrase_mode: SecretMode::Ask,
            passphrase: String::new(),
            folder: None,
            new_folder: String::new(),
            tags: String::new(),
            favorite: false,
            environment: Environment::None,
            jump: None,
            theme: ThemeOverride::default(),
            keepalive: d.keepalive_secs.map(|k| k.to_string()).unwrap_or_default(),
            timeout: d.connect_timeout_secs.to_string(),
        }
    }

    pub fn from_profile(p: &Profile) -> Self {
        let mut f = Self::blank();
        f.existing = Some(p.clone());
        f.name = p.name.clone();
        f.host = p.host.clone();
        f.port = p.port.to_string();
        f.username = p.username.clone();
        match &p.auth {
            AuthMethod::Password { credential } => {
                f.auth = AuthKind::Password;
                if credential.is_some() {
                    f.password_mode = SecretMode::Save;
                }
            }
            AuthMethod::KeyFile { path, passphrase } => {
                f.auth = AuthKind::Key;
                f.key_path = path.to_string_lossy().into_owned();
                if passphrase.is_some() {
                    f.passphrase_mode = SecretMode::Save;
                }
            }
            AuthMethod::Agent => f.auth = AuthKind::Agent,
            AuthMethod::KeyboardInteractive => f.auth = AuthKind::KeyboardInteractive,
        }
        f.folder = p.folder;
        f.tags = p.tags.join(", ");
        f.favorite = p.favorite;
        f.environment = p.environment;
        f.jump = p.jump;
        f.theme = p.theme.clone();
        f.keepalive = p.keepalive_secs.map(|k| k.to_string()).unwrap_or_default();
        f.timeout = p.connect_timeout_secs.to_string();
        f
    }

    /// Zera os buffers de segredo (chamado ao fechar o diálogo e no `Drop`).
    pub fn wipe(&mut self) {
        self.password.zeroize();
        self.passphrase.zeroize();
    }

    /// Credencial já salva do tipo atual (edição), se houver.
    fn existing_credential(&self) -> Option<CredentialId> {
        match self.existing.as_ref().map(|p| &p.auth) {
            Some(AuthMethod::Password { credential }) if self.auth == AuthKind::Password => {
                *credential
            }
            Some(AuthMethod::KeyFile { passphrase, .. }) if self.auth == AuthKind::Key => {
                *passphrase
            }
            _ => None,
        }
    }

    /// Valida e converte em [`Draft`]. `folders` serve para reaproveitar/criar a pasta
    /// digitada em "nova pasta".
    pub fn build(&self, folders: &[Folder]) -> Result<Draft, FormError> {
        let port: u16 = self.port.trim().parse().map_err(|_| FormError::Port)?;
        let keepalive = match self.keepalive.trim() {
            "" => None,
            s => Some(s.parse::<u32>().map_err(|_| FormError::Keepalive)?),
        };
        let timeout: u32 = self
            .timeout
            .trim()
            .parse()
            .ok()
            .filter(|t| *t > 0)
            .ok_or(FormError::Timeout)?;

        let mut new_secret = None;
        let auth = match self.auth {
            AuthKind::Password => {
                let credential = self.plan_secret(
                    self.password_mode,
                    &self.password,
                    CredentialKind::Password,
                    &mut new_secret,
                )?;
                AuthMethod::Password { credential }
            }
            AuthKind::Key => {
                let path = self.key_path.trim();
                if path.is_empty() {
                    return Err(FormError::KeyPath);
                }
                let passphrase = self.plan_secret(
                    self.passphrase_mode,
                    &self.passphrase,
                    CredentialKind::KeyPassphrase,
                    &mut new_secret,
                )?;
                AuthMethod::KeyFile {
                    path: path.into(),
                    passphrase,
                }
            }
            AuthKind::Agent => AuthMethod::Agent,
            AuthKind::KeyboardInteractive => AuthMethod::KeyboardInteractive,
        };

        let mut profile = match &self.existing {
            Some(p) => p.clone(),
            None => Profile::new("", "", ""),
        };
        profile.name = self.name.trim().to_string();
        profile.host = self.host.trim().to_string();
        profile.port = port;
        profile.username = self.username.trim().to_string();
        profile.auth = auth;
        profile.tags = parse_tags(&self.tags);
        profile.favorite = self.favorite;
        profile.environment = self.environment;
        profile.jump = self.jump;
        profile.theme = self.theme.clone();
        profile.keepalive_secs = keepalive;
        profile.connect_timeout_secs = timeout;

        let mut new_folder = None;
        profile.folder = self.folder;
        let nf = self.new_folder.trim();
        if !nf.is_empty() {
            match folders
                .iter()
                .find(|f| f.parent.is_none() && f.name.trim().eq_ignore_ascii_case(nf))
            {
                Some(f) => profile.folder = Some(f.id),
                None => {
                    let f = Folder::new(nf, None);
                    profile.folder = Some(f.id);
                    new_folder = Some(f);
                }
            }
        }

        profile.validate()?;

        let obsolete = match &self.existing {
            Some(old) => obsolete_credentials(&old.auth, &profile.auth),
            None => Vec::new(),
        };
        Ok(Draft {
            profile,
            new_secret,
            obsolete,
            new_folder,
        })
    }

    /// Regra de credencial para um segredo:
    /// * `Ask` → nenhuma credencial (a antiga, se existir, fica obsoleta);
    /// * `Save` com valor digitado → nova credencial + segredo (a antiga fica obsoleta);
    /// * `Save` sem valor → mantém a credencial atual; sem atual, erro.
    fn plan_secret(
        &self,
        mode: SecretMode,
        typed: &str,
        kind: CredentialKind,
        out: &mut Option<NewSecret>,
    ) -> Result<Option<CredentialId>, FormError> {
        match mode {
            SecretMode::Ask => Ok(None),
            SecretMode::Save if !typed.is_empty() => {
                let label = match kind {
                    CredentialKind::Password => i18n::cred_label_password(self.name.trim()),
                    CredentialKind::KeyPassphrase => i18n::cred_label_passphrase(self.name.trim()),
                };
                let credential = Credential {
                    id: CredentialId::new(),
                    label,
                    kind,
                    secret: new_secret_ref(),
                };
                let id = credential.id;
                *out = Some(NewSecret {
                    credential,
                    secret: SecretString::from(typed.to_string()),
                });
                Ok(Some(id))
            }
            SecretMode::Save => self
                .existing_credential()
                .map(Some)
                .ok_or(FormError::SecretRequired),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myssh_secrets::ExposeSecret;

    fn filled() -> ProfileForm {
        let mut f = ProfileForm::blank();
        f.name = " Web ".into();
        f.host = " web.local ".into();
        f.username = " deploy ".into();
        f
    }

    #[test]
    fn novo_perfil_basico() {
        let d = filled().build(&[]).unwrap();
        assert_eq!(d.profile.name, "Web");
        assert_eq!(d.profile.host, "web.local");
        assert_eq!(d.profile.username, "deploy");
        assert_eq!(d.profile.port, 22);
        assert_eq!(d.profile.keepalive_secs, Some(30));
        assert_eq!(d.profile.connect_timeout_secs, 15);
        assert_eq!(d.profile.auth, AuthMethod::Password { credential: None });
        assert!(d.new_secret.is_none() && d.obsolete.is_empty() && d.new_folder.is_none());
    }

    #[test]
    fn erros_de_campos() {
        let mut f = filled();
        f.port = "abc".into();
        assert_eq!(f.build(&[]).unwrap_err(), FormError::Port);
        f.port = "70000".into();
        assert_eq!(f.build(&[]).unwrap_err(), FormError::Port);
        f = filled();
        f.timeout = "0".into();
        assert_eq!(f.build(&[]).unwrap_err(), FormError::Timeout);
        f = filled();
        f.keepalive = "x".into();
        assert_eq!(f.build(&[]).unwrap_err(), FormError::Keepalive);
        f.keepalive = "".into();
        assert_eq!(f.build(&[]).unwrap().profile.keepalive_secs, None);
        f = filled();
        f.name = "  ".into();
        assert_eq!(
            f.build(&[]).unwrap_err(),
            FormError::Profile(ProfileError::EmptyName)
        );
        f = filled();
        f.host = "a b".into();
        assert_eq!(
            f.build(&[]).unwrap_err(),
            FormError::Profile(ProfileError::InvalidHost)
        );
        f = filled();
        f.port = "0".into();
        assert_eq!(
            f.build(&[]).unwrap_err(),
            FormError::Profile(ProfileError::InvalidPort)
        );
    }

    #[test]
    fn senha_salva_cria_credencial() {
        let mut f = filled();
        f.password_mode = SecretMode::Save;
        assert_eq!(f.build(&[]).unwrap_err(), FormError::SecretRequired);
        f.password = "s3gredo".into();
        let d = f.build(&[]).unwrap();
        let ns = d.new_secret.unwrap();
        assert_eq!(ns.secret.expose_secret(), "s3gredo");
        assert_eq!(ns.credential.kind, CredentialKind::Password);
        assert_eq!(
            d.profile.auth,
            AuthMethod::Password {
                credential: Some(ns.credential.id)
            }
        );
        // nada de segredo no Debug
        assert!(!format!("{ns:?}").contains("s3gredo"));
    }

    #[test]
    fn senha_perguntar_nao_persiste_mesmo_digitada() {
        let mut f = filled();
        f.password = "digitada".into();
        let d = f.build(&[]).unwrap();
        assert!(d.new_secret.is_none());
        assert_eq!(d.profile.auth, AuthMethod::Password { credential: None });
    }

    #[test]
    fn edicao_mantem_credencial_se_vazio_e_troca_se_digitado() {
        let mut p = Profile::new("a", "h", "u");
        let old = CredentialId::new();
        p.auth = AuthMethod::Password {
            credential: Some(old),
        };
        let mut f = ProfileForm::from_profile(&p);
        assert_eq!(f.password_mode, SecretMode::Save);
        let d = f.build(&[]).unwrap();
        assert_eq!(d.profile.id, p.id);
        assert_eq!(d.profile.auth, p.auth);
        assert!(d.new_secret.is_none() && d.obsolete.is_empty());

        f.password = "nova".into();
        let d = f.build(&[]).unwrap();
        assert!(d.new_secret.is_some());
        assert_eq!(d.obsolete, vec![old]);

        f.password.clear();
        f.password_mode = SecretMode::Ask;
        let d = f.build(&[]).unwrap();
        assert_eq!(d.profile.auth, AuthMethod::Password { credential: None });
        assert_eq!(d.obsolete, vec![old]);

        f.auth = AuthKind::Agent;
        let d = f.build(&[]).unwrap();
        assert_eq!(d.profile.auth, AuthMethod::Agent);
        assert_eq!(d.obsolete, vec![old]);
    }

    #[test]
    fn chave_exige_caminho_e_passphrase_opcional() {
        let mut f = filled();
        f.auth = AuthKind::Key;
        assert_eq!(f.build(&[]).unwrap_err(), FormError::KeyPath);
        f.key_path = " /home/u/.ssh/id_ed25519 ".into();
        let d = f.build(&[]).unwrap();
        assert_eq!(
            d.profile.auth,
            AuthMethod::KeyFile {
                path: "/home/u/.ssh/id_ed25519".into(),
                passphrase: None
            }
        );
        f.passphrase_mode = SecretMode::Save;
        f.passphrase = "pp".into();
        let d = f.build(&[]).unwrap();
        assert_eq!(
            d.new_secret.unwrap().credential.kind,
            CredentialKind::KeyPassphrase
        );
        // a senha digitada não vaza para a chave
        let mut g = filled();
        g.auth = AuthKind::Key;
        g.key_path = "k".into();
        g.password_mode = SecretMode::Save;
        g.password = "x".into();
        assert!(g.build(&[]).unwrap().new_secret.is_none());
    }

    #[test]
    fn pasta_nova_reaproveita_existente() {
        let existing = Folder::new("Produção", None);
        let mut f = filled();
        f.new_folder = " produção ".into();
        let d = f.build(std::slice::from_ref(&existing)).unwrap();
        assert_eq!(d.profile.folder, Some(existing.id));
        assert!(d.new_folder.is_none());

        f.new_folder = "Lab".into();
        let d = f.build(&[existing]).unwrap();
        let nf = d.new_folder.unwrap();
        assert_eq!(nf.name, "Lab");
        assert_eq!(d.profile.folder, Some(nf.id));
    }

    #[test]
    fn jump_para_si_mesmo_e_rejeitado() {
        let p = Profile::new("a", "h", "u");
        let mut f = ProfileForm::from_profile(&p);
        f.jump = Some(p.id);
        assert_eq!(
            f.build(&[]).unwrap_err(),
            FormError::Profile(ProfileError::JumpLoop)
        );
    }

    #[test]
    fn roundtrip_perfil_formulario_perfil() {
        let mut p = Profile::new("a", "h", "u");
        p.port = 2200;
        p.tags = vec!["x".into(), "y".into()];
        p.favorite = true;
        p.environment = Environment::Staging;
        p.keepalive_secs = None;
        p.connect_timeout_secs = 9;
        p.auth = AuthMethod::KeyboardInteractive;
        p.agent_forwarding = true;
        let d = ProfileForm::from_profile(&p).build(&[]).unwrap();
        assert_eq!(d.profile, p);
    }

    #[test]
    fn wipe_zera_buffers() {
        let mut f = filled();
        f.password = "abc".into();
        f.passphrase = "def".into();
        f.wipe();
        assert!(f.password.is_empty() && f.passphrase.is_empty());
    }
}
