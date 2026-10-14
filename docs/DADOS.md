# Onde o mySSH guarda seus dados

## Diretório de dados

Resolvido por `crates/app/src/paths.rs` com `directories::ProjectDirs::from("com", "myssh", "mySSH")`:

| SO | Caminho |
|---|---|
| macOS | `~/Library/Application Support/com.myssh.mySSH/` |
| Windows | `%APPDATA%\myssh\mySSH\data\` |
| Linux | `$XDG_DATA_HOME/myssh/` (padrão `~/.local/share/myssh/`) |

Variáveis de ambiente (não reduzem a segurança):
- `MYSSH_DATA_DIR=dir` sobrescreve o diretório de dados e, por segurança, também força o cofre **somente em memória** (nunca usa o cofre real do SO).
- `MYSSH_MEMORY_VAULT=1` força o cofre em memória sem trocar o diretório.

O diretório é criado com permissão 0700 e o banco com 0600 em sistemas unix. Ao abrir, permissões mais frouxas pré-existentes do diretório, do banco e de `-wal`/`-shm`/`-journal` são corrigidas para 0700/0600. A exportação de configuração também grava 0600 (arquivo temporário novo + rename).

Ganchos de demonstração (`MYSSH_DEMO_SHOTS`, `MYSSH_DEMO_SSH`, `MYSSH_DEMO_KEY`) só existem em builds de debug, em testes ou com `--features demo`; o release padrão não os contém.

Arquivo principal: `myssh.db` (SQLite). Se o banco não puder ser aberto, o app usa um banco em memória (nada é gravado).

## O que fica no SQLite

Tabelas: `folders`, `profiles` (host, porta, usuário, método de autenticação, tema, jump host, keepalive etc.), `profile_tags`, `credentials` (apenas rótulo, tipo e a referência `secret_ref` — nunca o segredo), `host_keys` (chaves de host confiáveis: algoritmo, chave pública, fingerprint) e `settings` (preferências).

Nenhuma senha ou passphrase é gravada no banco.

## O que fica no keyring do SO

Senhas e passphrases de chave, no cofre do SO (Keychain no macOS, Credential Manager no Windows, Secret Service no Linux):

- service: `com.myssh.app`
- account: referência aleatória (uuid v4) guardada em `credentials.secret_ref`, sem relação com host ou usuário.

Se o cofre do SO estiver indisponível, o app só usa armazenamento em memória (válido para a sessão) mediante ação explícita do usuário; nunca grava segredos em arquivo.

Chaves privadas SSH permanecem onde estão (por exemplo `~/.ssh`); o app guarda apenas o caminho.

## Telemetria

Não há telemetria, análise de uso, relatório de falhas nem verificação automática de atualização. A única rede usada é a das conexões SSH que você iniciar.

## Backup

Use a exportação de configuração (JSON) do app: inclui pastas e perfis (com tags), **sem segredos** (senhas e passphrases não são exportadas). Preferências e chaves de host confiáveis não entram no JSON. Para restaurar, importe o JSON; as senhas precisam ser digitadas de novo. Para backup completo do estado local, copie também `myssh.db` com o app fechado; os segredos continuam no cofre do SO e não vão junto.
