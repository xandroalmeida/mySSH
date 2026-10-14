# Backends de segredos (keyring 4.2.0)

Fontes: `~/.cargo/registry/src/*/` após `cargo fetch` (keyring 4.2.0, keyring-core 1.0.1,
apple-native-keyring-store 1.0.2, windows-native-keyring-store 1.1.0,
zbus-secret-service-keyring-store 1.0.1).

## Seleção de backend
- `keyring` 4.x é só fachada; a feature padrão `v1` ativa Keychain (macOS), Credential Manager
  (Windows) e Secret Service via zbus (Linux/BSD) — `keyring-4.2.0/Cargo.toml`, seção `[features]`.
  Usamos `keyring = "4.2.0"` com features padrão.
- Escolha por `cfg(target_os)` em `keyring-4.2.0/src/v1.rs:109-129` (`set_credential_store`).
- Linux **não** usa keyutils na feature `v1` (keyutils é só da feature `cli`; é keyring do kernel,
  não persistente em disco): evitado de propósito.

## Sem fallback silencioso
- `v1.rs` não referencia `mock`/`sample`; se a inicialização falha, `Entry::new` devolve
  `Error::NoDefaultStore` (`v1.rs:47-50`) e `Entry::store_status()` expõe o erro (`v1.rs:67`).
- Plataformas fora de macOS/Windows/unix-não-mobile: `Error::Invalid("platform", ...)` (`v1.rs:124-128`).
- O store de exemplo/mock (`keyring-core::sample`) só entra com a feature `cli`, que NÃO habilitamos.
- Conclusão: `OsKeyring` consulta `store_status()` antes de cada operação; erro => `Unavailable`.
  Nunca caímos para arquivo/memória sozinhos.

## Limites de tamanho
- Windows: blob máx. `CRED_MAX_CREDENTIAL_BLOB_SIZE = 2560` bytes (windows-sys 0.59,
  `Security/Credentials/mod.rs:191`). A senha é gravada em UTF-16, então o máximo real é **1280 unidades
  UTF-16** (`windows-native-keyring-store-1.1.0/src/utils.rs:69-94`, erro `Error::TooLong`).
  Validamos esse limite em todos os stores (`MAX_SECRET_UTF16`) para o dado ser portável.
- Usuário (nosso `SecretRef`, uuid de 36 chars) e service ficam bem abaixo dos limites de nome.
- Persistência Windows: padrão `Enterprise` (`windows-native-keyring-store/src/lib.rs:38-42`).

## Secret Service ausente ou bloqueado (Linux)
- Sem D-Bus/serviço: `SecretService::connect` falha => `Store::new()` retorna `PlatformFailure`
  (`zbus-secret-service-keyring-store/src/service.rs:24-28`) => `store_status()` Err => `Unavailable`.
- Coleção/item bloqueado: o store tenta destravar, o que **abre prompt do sistema e bloqueia a thread**
  (`service.rs:216-217`, `cred.rs:205+`); se negado/falha: `ServiceError::Locked/NoResult/Prompt`
  => `Error::NoStorageAccess` (`errors.rs:10-15`) => nosso `SecretError::Locked`.
  Chamadas podem bloquear: **nunca na thread de UI**.
- Headless: precisa de gnome-keyring/kwallet destravado (`lib.rs:63-80` do store).

## macOS
- `apple_native_keyring_store::keychain::Store` (keychain de login). Erros -61, -25244, -25291,
  -25292, -25294, -25295 => `NoStorageAccess`; -25300 => `NoEntry` (`keychain.rs:369-375`).
  O Keychain pode exibir prompt de acesso ao item (ACL por app) com binários diferentes/não assinados.

## Estado global
- `v1` registra o store como *default store* global do processo (`keyring_core::set_default_store`);
  o `service` é por `Entry`, então app e teste coexistem.

## Não testado
Windows e Linux: apenas leitura de fonte; só macOS foi executado de fato.
