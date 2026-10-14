# AGENTS.md — mySSH

Gerenciador desktop de sessões SSH em Rust (macOS, Windows, Linux). UI e mensagens em pt-BR.

## Papéis
- **Opus (claude-opus-5-5)**: arquitetura, contratos entre módulos, distribuição de tarefas, revisão e integração. Marca tarefas como concluídas.
- **Sonnet (claude-sonnet-5-5)**: implementa código, testes e correções em tarefas pequenas, com arquivos sob sua responsabilidade.
- Toda entrega informa: alterações, verificações executadas, resultado, limitações. Nunca incluir segredos em relatórios.

## Comandos
- Servidor de teste: `./testenv/up.sh` → `127.0.0.1:2222`, usuário `tester`, senha `tester-pass`, chaves em `testenv/keys/` (`id_ed25519` sem passphrase; `id_ed25519_pass` com passphrase `test-passphrase`). Só para desenvolvimento.
- Testenv (M2): `up.sh` também sobe `myssh-testsshd-target` (**sem porta publicada**; só alcançável pela rede docker `myssh-testnet`, como `myssh-testsshd-target:22`, a partir do bastion `127.0.0.1:2222`; host keys próprias). Usuários nos dois: `tester` (senha/chave); `mfa` (chave pública **e** keyboard-interactive: prompt "Password:" respondido com o código fixo `mfa-code-123456`); `kionly` (só keyboard-interactive, resposta `ki-pass`). O sshd roda com PAM (`sshd.pam`).
- Testes de jump host/MFA/SFTP: mesmos comandos abaixo (o alvo do jump é `MYSSH_TEST_TARGET`, padrão `myssh-testsshd-target:22`; os testes leem as host keys dos containers via `docker exec`, fora do código sob teste). Agente do sistema (macOS): `MYSSH_TEST_SYSTEM_AGENT=1` habilita `autenticacao_pelo_agente_do_sistema` (adiciona e remove a chave de teste no agente).
- `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Testes de integração que precisam do servidor: `MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys cargo test --workspace -- --ignored --test-threads=1` (`real_saida_dos_testes_sem_segredos` precisa de `MYSSH_TEST_LOG`, ver docs/M1-RESULTADO.md)

## Convenções
- Nunca desabilitar validação de host key, nem "aceitar qualquer chave", nem em testes (testes fixam a chave esperada).
- Nenhum segredo em logs, `Debug`, erros, argumentos de processo ou arquivos em texto puro. Use `secrecy`/`zeroize`.
- Nenhuma operação de rede/disco pesada na thread de UI.
- Textos de UI centralizados (módulo `i18n`), não espalhados.
- Saída remota nunca executa comandos locais, abre URLs ou escreve no clipboard sem ação explícita.
- Contratos centrais (`crates/core` tipos públicos) só mudam via Opus.
- Commits locais como checkpoints; sem push.
