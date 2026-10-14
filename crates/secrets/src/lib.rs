//! Armazenamento de segredos (senhas/passphrases) do mySSH.
//!
//! * [`SecretStore`]: contrato de armazenamento, endereçado por [`SecretRef`].
//! * [`OsKeyring`]: cofre do SO (Keychain / Credential Manager / Secret Service).
//! * [`MemoryStore`]: só para a sessão, uso explícito (nunca é fallback automático).
//! * [`Vault`]: envolve um store e implementa bloqueio manual e por inatividade.
//!
//! Nenhum tipo aqui expõe valores via `Debug`/`Display`/erros.
//!
//! Chamadas ao [`OsKeyring`] podem bloquear (prompt do SO): não chamar da thread de UI.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub use myssh_core::SecretRef;
pub use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

/// Service padrão usado no cofre do SO.
pub const DEFAULT_SERVICE: &str = "com.myssh.app";

/// Limite portável: Windows guarda no máximo 2560 bytes em UTF-16 (1280 unidades).
pub const MAX_SECRET_UTF16: usize = 1280;

/// Gera uma referência aleatória (uuid v4), sem relação com host/usuário.
pub fn new_secret_ref() -> SecretRef {
    SecretRef(uuid::Uuid::new_v4().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// Cofre do SO inexistente/inacessível. A UI pode oferecer uso temporário em memória
    /// (`MemoryStore`), mas isso exige ação explícita do usuário.
    #[error("armazenamento seguro indisponível: {0}")]
    Unavailable(String),
    /// Cofre bloqueado (do SO ou do `Vault`).
    #[error("cofre bloqueado")]
    Locked,
    #[error("segredo não encontrado")]
    NotFound,
    #[error("segredo grande demais (máximo {max} unidades UTF-16)")]
    TooLong { max: usize },
    #[error("falha no armazenamento seguro: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, SecretError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Available,
    Locked,
    Unavailable(String),
}

pub trait SecretStore: Send + Sync {
    fn put(&self, key: &SecretRef, secret: SecretString) -> Result<()>;
    fn get(&self, key: &SecretRef) -> Result<SecretString>;
    /// Apagar algo inexistente retorna `NotFound`.
    fn delete(&self, key: &SecretRef) -> Result<()>;
    fn availability(&self) -> Availability;
}

fn check_len(secret: &SecretString) -> Result<()> {
    if secret.expose_secret().encode_utf16().count() > MAX_SECRET_UTF16 {
        return Err(SecretError::TooLong {
            max: MAX_SECRET_UTF16,
        });
    }
    Ok(())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------- MemoryStore

/// Armazenamento só em memória. Valores são zeroizados ao serem removidos/descartados
/// (`SecretString` zeroiza no `Drop`).
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, SecretString>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore")
            .field("entries", &lock(&self.map).len())
            .finish()
    }
}

impl SecretStore for MemoryStore {
    fn put(&self, key: &SecretRef, secret: SecretString) -> Result<()> {
        check_len(&secret)?;
        lock(&self.map).insert(key.0.clone(), secret);
        Ok(())
    }
    fn get(&self, key: &SecretRef) -> Result<SecretString> {
        lock(&self.map)
            .get(&key.0)
            .cloned()
            .ok_or(SecretError::NotFound)
    }
    fn delete(&self, key: &SecretRef) -> Result<()> {
        lock(&self.map)
            .remove(&key.0)
            .map(|_| ())
            .ok_or(SecretError::NotFound)
    }
    fn availability(&self) -> Availability {
        Availability::Available
    }
}

// ------------------------------------------------------------------ OsKeyring

/// Cofre do SO via `keyring` (sem fallback para mock/arquivo; ver `BACKENDS.md`).
#[derive(Debug, Clone)]
pub struct OsKeyring {
    service: String,
}

impl Default for OsKeyring {
    fn default() -> Self {
        Self::new()
    }
}

impl OsKeyring {
    pub fn new() -> Self {
        Self::with_service(DEFAULT_SERVICE)
    }

    /// Service configurável (testes usam `"com.myssh.test"`).
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &SecretRef) -> Result<keyring::Entry> {
        if let Err(e) = keyring::Entry::store_status() {
            return Err(SecretError::Unavailable(describe(e)));
        }
        keyring::Entry::new(&self.service, &key.0).map_err(map_err)
    }
}

/// Descrição sem dados sensíveis (nunca usa o `Debug` de variantes com bytes).
fn describe(e: &keyring::Error) -> String {
    use keyring::Error as E;
    match e {
        E::NoDefaultStore => "nenhum cofre do sistema inicializado".into(),
        E::Invalid(attr, why) => format!("parâmetro inválido {attr}: {why}"),
        E::PlatformFailure(err) | E::NoStorageAccess(err) => err.to_string(),
        _ => "erro do cofre do sistema".into(),
    }
}

fn map_err(e: keyring::Error) -> SecretError {
    use keyring::Error as E;
    match e {
        E::NoEntry => SecretError::NotFound,
        E::NoStorageAccess(_) => SecretError::Locked,
        E::TooLong(..) => SecretError::TooLong {
            max: MAX_SECRET_UTF16,
        },
        E::NoDefaultStore => SecretError::Unavailable(describe(&e)),
        other => SecretError::Backend(describe(&other)),
    }
}

impl SecretStore for OsKeyring {
    fn put(&self, key: &SecretRef, secret: SecretString) -> Result<()> {
        check_len(&secret)?;
        self.entry(key)?
            .set_password(secret.expose_secret())
            .map_err(map_err)
    }

    fn get(&self, key: &SecretRef) -> Result<SecretString> {
        match self.entry(key)?.get_password() {
            Ok(s) => Ok(SecretString::from(s)),
            Err(keyring::Error::BadEncoding(mut bytes)) => {
                bytes.zeroize();
                Err(SecretError::Backend(
                    "segredo com codificação inválida".into(),
                ))
            }
            Err(e) => Err(map_err(e)),
        }
    }

    fn delete(&self, key: &SecretRef) -> Result<()> {
        self.entry(key)?.delete_credential().map_err(map_err)
    }

    fn availability(&self) -> Availability {
        if let Err(e) = keyring::Entry::store_status() {
            return Availability::Unavailable(describe(e));
        }
        // Sondagem: ler uma entrada inexistente. NoEntry => cofre acessível.
        let probe = SecretRef("availability-probe".into());
        match self.entry(&probe).map(|e| e.get_password()) {
            Err(SecretError::Unavailable(r)) => Availability::Unavailable(r),
            Err(_) => Availability::Unavailable("não foi possível criar entrada".into()),
            Ok(Ok(mut s)) => {
                s.zeroize();
                Availability::Available
            }
            Ok(Err(keyring::Error::NoEntry)) => Availability::Available,
            Ok(Err(keyring::Error::NoStorageAccess(_))) => Availability::Locked,
            Ok(Err(e)) => Availability::Unavailable(describe(&e)),
        }
    }
}

// ---------------------------------------------------------------------- Vault

struct VaultState {
    locked: bool,
    last_activity: Instant,
    idle_timeout: Option<Duration>,
    cache: HashMap<String, SecretString>,
}

/// Cofre com bloqueio manual e por inatividade, sobre qualquer [`SecretStore`].
///
/// **Bloquear o cofre NÃO encerra sessões SSH já autenticadas**: ele só impede novas
/// recuperações de segredos (novas conexões que precisem de senha/passphrase salva).
/// Quem mantém a sessão é o crate SSH; ela segue viva até ser fechada explicitamente.
///
/// Sem threads próprias: o chamador invoca [`Vault::tick`] periodicamente.
///
/// Se o cofre do SO estiver indisponível, as operações retornam
/// [`SecretError::Unavailable`]; a UI decide se oferece `Vault::new(Arc::new(MemoryStore::new()))`
/// (uso temporário). Nunca há fallback automático.
pub struct Vault {
    store: Arc<dyn SecretStore>,
    state: Mutex<VaultState>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = lock(&self.state);
        f.debug_struct("Vault")
            .field("locked", &s.locked)
            .field("idle_timeout", &s.idle_timeout)
            .field("cached", &s.cache.len())
            .finish()
    }
}

impl Vault {
    /// Cria o cofre desbloqueado, sem timeout de inatividade.
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self {
            store,
            state: Mutex::new(VaultState {
                locked: false,
                last_activity: Instant::now(),
                idle_timeout: None,
                cache: HashMap::new(),
            }),
        }
    }

    pub fn set_idle_timeout(&self, timeout: Option<Duration>) {
        lock(&self.state).idle_timeout = timeout;
    }

    pub fn is_locked(&self) -> bool {
        lock(&self.state).locked
    }

    pub fn availability(&self) -> Availability {
        self.store.availability()
    }

    /// Bloqueia e zeroiza o cache em memória.
    pub fn lock(&self) {
        let mut s = lock(&self.state);
        s.locked = true;
        s.cache.clear(); // SecretString zeroiza no Drop
    }

    /// Desbloqueia se o store subjacente estiver acessível.
    pub fn unlock(&self) -> Result<()> {
        self.unlock_at(Instant::now())
    }

    pub fn unlock_at(&self, now: Instant) -> Result<()> {
        match self.store.availability() {
            Availability::Available => {}
            Availability::Locked => return Err(SecretError::Locked),
            Availability::Unavailable(r) => return Err(SecretError::Unavailable(r)),
        }
        let mut s = lock(&self.state);
        s.locked = false;
        s.last_activity = now;
        Ok(())
    }

    /// Registra atividade (adia o bloqueio por inatividade).
    pub fn touch(&self) {
        self.touch_at(Instant::now());
    }

    pub fn touch_at(&self, now: Instant) {
        lock(&self.state).last_activity = now;
    }

    /// Verifica a inatividade; bloqueia se passou do timeout. Retorna `true` se o cofre
    /// está bloqueado após a chamada.
    pub fn tick(&self, now: Instant) -> bool {
        let expired = {
            let s = lock(&self.state);
            !s.locked
                && s.idle_timeout
                    .is_some_and(|t| now.saturating_duration_since(s.last_activity) >= t)
        };
        if expired {
            self.lock();
        }
        self.is_locked()
    }

    fn ensure_unlocked(&self) -> Result<()> {
        if lock(&self.state).locked {
            Err(SecretError::Locked)
        } else {
            Ok(())
        }
    }

    pub fn put(&self, key: &SecretRef, secret: SecretString) -> Result<()> {
        self.ensure_unlocked()?;
        self.touch();
        lock(&self.state).cache.remove(&key.0);
        self.store.put(key, secret)
    }

    pub fn get(&self, key: &SecretRef) -> Result<SecretString> {
        self.ensure_unlocked()?;
        self.touch();
        if let Some(v) = lock(&self.state).cache.get(&key.0) {
            return Ok(v.clone());
        }
        let v = self.store.get(key)?;
        let mut s = lock(&self.state);
        if s.locked {
            // bloqueado durante a leitura: não guardar nada
            return Err(SecretError::Locked);
        }
        s.cache.insert(key.0.clone(), v.clone());
        Ok(v)
    }

    pub fn delete(&self, key: &SecretRef) -> Result<()> {
        self.ensure_unlocked()?;
        self.touch();
        lock(&self.state).cache.remove(&key.0);
        self.store.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(s: &str) -> SecretString {
        SecretString::from(s.to_string())
    }

    fn vault() -> Vault {
        Vault::new(Arc::new(MemoryStore::new()))
    }

    #[test]
    fn refs_sao_aleatorias() {
        assert_ne!(new_secret_ref(), new_secret_ref());
    }

    #[test]
    fn memory_roundtrip_e_erros() {
        let m = MemoryStore::new();
        let r = new_secret_ref();
        assert_eq!(m.get(&r).unwrap_err(), SecretError::NotFound);
        m.put(&r, secret("abc")).unwrap();
        assert_eq!(m.get(&r).unwrap().expose_secret(), "abc");
        m.delete(&r).unwrap();
        assert_eq!(m.delete(&r).unwrap_err(), SecretError::NotFound);
    }

    #[test]
    fn limite_de_tamanho() {
        let m = MemoryStore::new();
        let r = new_secret_ref();
        m.put(&r, secret(&"a".repeat(MAX_SECRET_UTF16))).unwrap();
        let err = m
            .put(&r, secret(&"a".repeat(MAX_SECRET_UTF16 + 1)))
            .unwrap_err();
        assert_eq!(
            err,
            SecretError::TooLong {
                max: MAX_SECRET_UTF16
            }
        );
        // emoji conta 2 unidades UTF-16
        let err = m
            .put(&r, secret(&"😀".repeat(MAX_SECRET_UTF16 / 2 + 1)))
            .unwrap_err();
        assert!(matches!(err, SecretError::TooLong { .. }));
    }

    #[test]
    fn lock_impede_operacoes_e_unlock_restaura() {
        let v = vault();
        let r = new_secret_ref();
        v.put(&r, secret("s")).unwrap();
        assert_eq!(v.get(&r).unwrap().expose_secret(), "s");
        v.lock();
        assert!(v.is_locked());
        assert_eq!(v.get(&r).unwrap_err(), SecretError::Locked);
        assert_eq!(v.put(&r, secret("x")).unwrap_err(), SecretError::Locked);
        assert_eq!(v.delete(&r).unwrap_err(), SecretError::Locked);
        v.unlock().unwrap();
        assert_eq!(v.get(&r).unwrap().expose_secret(), "s");
    }

    #[test]
    fn lock_zeroiza_cache() {
        let v = vault();
        let r = new_secret_ref();
        v.put(&r, secret("s")).unwrap();
        v.get(&r).unwrap();
        assert_eq!(lock(&v.state).cache.len(), 1);
        v.lock();
        assert!(lock(&v.state).cache.is_empty());
    }

    #[test]
    fn timeout_de_inatividade() {
        let v = vault();
        v.set_idle_timeout(Some(Duration::from_secs(60)));
        let t0 = Instant::now();
        v.touch_at(t0);
        assert!(!v.tick(t0 + Duration::from_secs(59)));
        v.touch_at(t0 + Duration::from_secs(59)); // atividade adia
        assert!(!v.tick(t0 + Duration::from_secs(100)));
        assert!(v.tick(t0 + Duration::from_secs(119)));
        assert_eq!(v.get(&new_secret_ref()).unwrap_err(), SecretError::Locked);
        v.unlock_at(t0 + Duration::from_secs(120)).unwrap();
        assert!(!v.tick(t0 + Duration::from_secs(130)));
    }

    #[test]
    fn sem_timeout_nunca_bloqueia_sozinho() {
        let v = vault();
        assert!(!v.tick(Instant::now() + Duration::from_secs(86_400)));
    }

    #[test]
    fn delete_remove_do_cache() {
        let v = vault();
        let r = new_secret_ref();
        v.put(&r, secret("s")).unwrap();
        v.get(&r).unwrap();
        v.delete(&r).unwrap();
        assert_eq!(v.get(&r).unwrap_err(), SecretError::NotFound);
    }

    struct Down;
    impl SecretStore for Down {
        fn put(&self, _: &SecretRef, _: SecretString) -> Result<()> {
            Err(SecretError::Unavailable("sem serviço".into()))
        }
        fn get(&self, _: &SecretRef) -> Result<SecretString> {
            Err(SecretError::Unavailable("sem serviço".into()))
        }
        fn delete(&self, _: &SecretRef) -> Result<()> {
            Err(SecretError::Unavailable("sem serviço".into()))
        }
        fn availability(&self) -> Availability {
            Availability::Unavailable("sem serviço".into())
        }
    }

    #[test]
    fn store_indisponivel_gera_erro_tipado_sem_fallback() {
        let v = Vault::new(Arc::new(Down));
        let r = new_secret_ref();
        assert!(matches!(
            v.put(&r, secret("s")),
            Err(SecretError::Unavailable(_))
        ));
        assert!(matches!(v.get(&r), Err(SecretError::Unavailable(_))));
        v.lock();
        assert!(matches!(v.unlock(), Err(SecretError::Unavailable(_))));
        assert!(v.is_locked());
    }

    #[test]
    fn debug_nao_vaza_valor() {
        let m = MemoryStore::new();
        let r = new_secret_ref();
        m.put(&r, secret("TOP-SEGREDO")).unwrap();
        let v = Vault::new(Arc::new(m));
        v.get(&r).unwrap();
        assert!(!format!("{v:?}").contains("TOP-SEGREDO"));
        assert!(!format!("{:?}", secret("TOP-SEGREDO")).contains("TOP-SEGREDO"));
    }

    /// Teste real no cofre do SO. Rodar com:
    /// `MYSSH_TEST_KEYRING=1 cargo test -p myssh-secrets -- --ignored`
    #[test]
    #[ignore = "usa o cofre real do SO; exige MYSSH_TEST_KEYRING=1"]
    fn os_keyring_real() {
        if std::env::var("MYSSH_TEST_KEYRING").as_deref() != Ok("1") {
            eprintln!("MYSSH_TEST_KEYRING!=1: ignorado");
            return;
        }
        struct Cleanup(OsKeyring, SecretRef);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete(&self.1);
            }
        }
        let store = OsKeyring::with_service("com.myssh.test");
        let r = new_secret_ref();
        let _guard = Cleanup(store.clone(), r.clone());

        assert_eq!(store.availability(), Availability::Available);
        assert_eq!(store.get(&r).unwrap_err(), SecretError::NotFound);
        store.put(&r, secret("valor-de-teste-ç😀")).unwrap();
        assert_eq!(store.get(&r).unwrap().expose_secret(), "valor-de-teste-ç😀");
        store.put(&r, secret("novo")).unwrap(); // sobrescreve
        assert_eq!(store.get(&r).unwrap().expose_secret(), "novo");
        store.delete(&r).unwrap();
        assert_eq!(store.get(&r).unwrap_err(), SecretError::NotFound);
        assert_eq!(store.delete(&r).unwrap_err(), SecretError::NotFound);

        // via Vault
        let v = Vault::new(Arc::new(store.clone()));
        v.put(&r, secret("via-vault")).unwrap();
        assert_eq!(v.get(&r).unwrap().expose_secret(), "via-vault");
        v.delete(&r).unwrap();
        assert_eq!(v.get(&r).unwrap_err(), SecretError::NotFound);
    }
}
