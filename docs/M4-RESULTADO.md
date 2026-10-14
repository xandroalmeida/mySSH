# M4-B — UI de sincronização (resultado)

## O que funciona
- **"Sincronizar…"** no painel de arquivos (entre as colunas): raízes = pasta aberta da coluna local e da remota.
- **Opções**: direção explícita (Local ➡ Remoto / Remoto ➡ Local, com "o destino (X) será alterado"), comparação por tamanho+data (tolerância de 2 s) ou por conteúdo (com aviso do custo de rede), exclusões editáveis (`.git/` por padrão, um padrão por linha, validados), modo espelho **desligado** por padrão com aviso vermelho.
- **Prévia** em segundo plano (progresso do hash de conteúdo): resumo (criar, atualizar, conflitos, excluir, pular, bytes), lista filtrável por ação e por caminho, com o motivo em pt-BR de cada `UpdateReason`/`ConflictReason`/`SkipReason`. Conflitos de data/tamanho: "Pular" (padrão) ou "Usar origem" por item; colisão de nome e divergência de tipo não são resolvidas automaticamente.
- **Exclusões** (só no espelho): seção vermelha própria; "Executar" só habilita com "Confirmo excluir N itens no destino" e o botão passa a "Executar e excluir N". Sem espelho a seção e o filtro nem aparecem.
- **Execução**: itens x/y, bytes, item atual, cancelar, ocultar (continua em segundo plano), erros por item, resumo final. `PlanStale` mostra "A prévia ficou velha: nada foi executado" e o botão "Gerar nova prévia" (o plano velho nunca é executado).
- **Retomada**: ao abrir o diálogo, se há journal com itens pendentes (arquivo por perfil em `<dados>/sync/<id-do-perfil>.json`), oferece "Retomar" (mostra só o que falta; decisões são dadas de novo; o executor revalida) ou "Descartar".
- **Fechar aba, painel dividido ou app com sincronização em execução** pede confirmação (junto com o aviso das transferências); fechar cancela a sincronização.
- Barra de transferência com falha agora fica vermelha ("Falhou: há arquivos com erro") e a cancelada, cinza; nunca parece 100% concluída. Setas de ordenação das colunas passaram a usar glifos que existem na fonte.

## Como executar
- Testes comuns: `cargo test --workspace`.
- Servidor real (`./testenv/up.sh`): `MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys cargo test -p myssh-app real_sync -- --ignored --nocapture --test-threads=1` (também `-p myssh-syncrun -- --ignored --test-threads=1`).
- Renders: `MYSSH_DEMO_SHOTS=dir cargo test -p myssh-app m4b_screenshots -- --ignored` (GPU).

## Testado
- Unitários: textos dos motivos, filtros, montagem de `Decisions`, regra do botão de exclusão, exclusões do usuário.
- Headless com executor simulado: opções e avisos, validação de padrões, prévia, conflitos, espelho com checkbox, execução, resumo, `PlanStale` → nova prévia, ocultar/cancelar, cancelamento ao fechar, retomada e descarte.
- Real (`real_sync_fluxo_completo`): prévia local→remoto com Unicode e espaços (`.git/` fora), execução, extra no destino continua sem espelho, espelho sem o checkbox não executa, espelho confirmado apaga só o planejado (arquivo criado depois da prévia sobrevive), alteração no destino depois da prévia → `PlanStale` na UI.

## Limitações
- A prévia não é cancelável (fechar o diálogo a abandona e ignora o resultado).
- A retomada usa as pastas atuais das colunas e perde a decisão dos conflitos (precisa decidir de novo); a comparação por conteúdo não é refeita.
- Um journal por perfil: uma nova execução sobrescreve o anterior.
- Limites herdados do executor: ver `crates/syncrun/src/lib.rs` (conteúdo lê os arquivos inteiros via SFTP, rename remoto não atômico, TOCTOU).
