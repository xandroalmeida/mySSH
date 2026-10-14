# M3 — SFTP: navegação, transferência, fila (resultado)

## O que funciona
- **Painel de arquivos** em cada aba de terminal conectada (botão "Arquivos" ou Cmd/Ctrl+Shift+F): colunas local e remota, caminho editável, voltar/subir/atualizar/início, ocultos, ordenação, seleção múltipla, teclado (setas, Enter, Backspace, Delete, F2, Cmd+A), menu de contexto, nova pasta, renomear, excluir com confirmação (pasta exige "entendo").
- **Transferências** (motor `myssh-transfer`): upload/download de arquivos e pastas recursivas por botões ⬆/⬇, arrastar-soltar entre colunas e de arquivos do SO para a coluna remota. Política de sobrescrita (Perguntar/Sobrescrever/Pular/Renomear): padrão em Configurações, ajustável no painel.
- **Fila**: progresso, bytes, arquivos x/y, taxa, ETA, estado, cancelar por item/todas, erros por arquivo, diálogo de conflito (com tamanho/data dos dois lados e "Aplicar a todos").
- **Fechar** aba, painel dividido ou app com fila ativa pede confirmação dizendo quantas transferências serão canceladas. Reconectar/desconectar cancela a fila e recria o SFTP.
- O terminal segue interativo: listagens e operações rodam no runtime; os eventos da fila são drenados no máximo 128 por frame.

## Como executar
- Testes comuns: `cargo test --workspace`.
- Contra o servidor real (`./testenv/up.sh`; `/home/tester/small` é um tmpfs de 2 MiB):
  `MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys cargo test -p myssh-app real_arquivos -- --ignored --nocapture --test-threads=1`
  (`MYSSH_DEMO_SHOTS=dir` também grava PNGs; precisa de GPU).
- Motor: `cargo test -p myssh-transfer -- --ignored --test-threads=1` com as mesmas variáveis.

## Testado
- UI headless com backend simulado (`MemBackend`): navegação, seleção, teclado, envio/baixa, progresso, conflito, cancelamento, exclusão, nova pasta/renomear, arrastar-soltar, confirmações ao fechar.
- UI contra o servidor real (`real_arquivos_fluxo_completo`): abrir o painel, listar o home remoto, enviar arquivo e pasta com Unicode e espaços, baixar de volta e comparar, conflito (Pular e Sobrescrever), cancelar upload de 768 MiB (sem arquivo nem temporário no remoto), disco cheio no tmpfs de 2 MiB ("Sem espaço disponível no destino." na fila), excluir remoto com confirmação.
- Tempo de frame da UI com fila ativa (upload grande em loopback, 66 frames, sem render de GPU): p50 ≈ 2 ms, p95 ≈ 3–4 ms, máx ≈ 5 ms.
- Intermitência de `real_tests` no `cargo test --workspace` (2 de 5 execuções falhavam): causa era corrida do teste, que lia a mensagem de erro assim que o estado virava "erro", mas o evento `Closed` com a mensagem chega depois. Os testes agora esperam também o fechamento. 5 execuções seguidas sem falha.

## Números do motor (loopback)
- Upload ≈ 182 MB/s, download ≈ 173 MB/s.
- Latência de eco do terminal durante transferência: p95 17–20 ms.

## Limitações
- Rename remoto **não é atômico** quando o destino existe (a biblioteca SFTP não expõe `posix-rename@openssh.com`): o antigo vai para um backup, o novo é renomeado e o backup removido.
- Retomada só para arquivos ≥ 4 MiB, com sidecar e validação do trecho final.
- ENOSPC claro depende de `statvfs@openssh.com` no servidor.
- Symlinks e tipos especiais não são transferidos (viram `FileError` contado em "pulados").
- Exclusão local é permanente (sem lixeira). Nomes locais não UTF-8 aparecem com caractere de substituição e a operação falha.
- Na janela de conflito, os metadados do arquivo que chegaria só aparecem quando a origem é um arquivo único da listagem (dentro de pastas aparece "desconhecido").
- O duplo clique não é exercitado por teste (limitação do `egui_kittest`); a ação que ele dispara é a mesma do Enter, testada.
- `real_saida_dos_testes_sem_segredos` exige `MYSSH_TEST_LOG` (ver `docs/M1-RESULTADO.md`).
