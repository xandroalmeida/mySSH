# AGENTS.md — mySSH

Gerenciador desktop de sessões SSH em Rust (macOS, Windows, Linux). UI e mensagens em pt-BR.

## Papéis
- **Opus (claude-opus-5-5)**: arquitetura, contratos entre módulos, distribuição de tarefas, revisão e integração. Marca tarefas como concluídas.
- **Sonnet (claude-sonnet-5-5)**: implementa código, testes e correções em tarefas pequenas, com arquivos sob sua responsabilidade.
- Toda entrega informa: alterações, verificações executadas, resultado, limitações. Nunca incluir segredos em relatórios.

## Comandos
- Servidor de teste: `./testenv/up.sh` → `127.0.0.1:2222`, usuário `tester`, senha `tester-pass`, chaves em `testenv/keys/` (`id_ed25519` sem passphrase; `id_ed25519_pass` com passphrase `test-passphrase`). Só para desenvolvimento.
- `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Testes de integração que precisam do servidor: `MYSSH_TEST_SSH=127.0.0.1:2222 cargo test --workspace -- --ignored`

## Convenções
- Nunca desabilitar validação de host key, nem "aceitar qualquer chave", nem em testes (testes fixam a chave esperada).
- Nenhum segredo em logs, `Debug`, erros, argumentos de processo ou arquivos em texto puro. Use `secrecy`/`zeroize`.
- Nenhuma operação de rede/disco pesada na thread de UI.
- Textos de UI centralizados (módulo `i18n`), não espalhados.
- Saída remota nunca executa comandos locais, abre URLs ou escreve no clipboard sem ação explícita.
- Contratos centrais (`crates/core` tipos públicos) só mudam via Opus.
- Commits locais como checkpoints; sem push.
