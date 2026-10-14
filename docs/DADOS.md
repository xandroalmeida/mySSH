# Onde o mySSH guarda seus dados

## Diretório de dados

Resolvido por `crates/app/src/paths.rs` com `directories::ProjectDirs::from("com", "myssh", "mySSH")`:

| SO | Caminho |
|---|---|
| macOS | `~/Library/Application Support/com.myssh.mySSH/` |
| Windows | `%APPDATA%\myssh\mySSH\data\` |
| Linux | `$XDG_DATA_HOME/myssh/` (padrão `~/.local/share/myssh/`) |

A variável `MYSSH_DATA_DIR` sobrescreve o diretório (demo/testes). O diretório é criado com permissão 0700 e o banco com 0600 em sistemas unix.

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
