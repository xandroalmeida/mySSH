# M1 — resultado

Aceite do M1: perfis persistidos, senha/chave, abas, sessões simultâneas, desconexão e erros claros.

## O que funciona
- **Aba de terminal real** (`crates/app/src/terminal_tab.rs`): `myssh-term` + `myssh-ssh` no runtime do app, `Arc<Store>` como `HostTrustStore`.
- **Credenciais** resolvidas fora da thread de UI (`spawn_blocking`): senha/passphrase salvas (cofre), "perguntar" (modal com campo de senha, buffer zeroizado ao fechar, "Salvar no cofre" desmarcado por padrão e só para perfis salvos), cofre bloqueado (desbloquear ou digitar na hora). Chave sem passphrase salva: o app detecta se ela é criptografada (`myssh_ssh::key_requires_passphrase`) e só então pergunta.
- **Host key**: desconhecida → modal (host:porta, algoritmo, SHA-256 mono selecionável, "Rejeitar" com foco, "Aceitar uma vez", "Aceitar e salvar"); alterada/revogada → painel vermelho bloqueante, **sem** botão de aceitar, com "Abrir hosts confiáveis". Configurações → **Hosts confiáveis** lista as chaves salvas e "Esquecer" (com confirmação); depois disso a conexão cai no fluxo de host desconhecido.
- **Estados** com ícone + texto na barra de abas e na barra da aba: preparando, conectando, aguardando confiança, autenticando, conectado, desconectado, erro, bloqueado. Botões Desconectar / Reconectar (ou Tentar novamente após senha/passphrase recusada, que pergunta de novo). Erros em pt-BR na própria aba.
- **Várias sessões**: todas as abas têm `pump` a cada frame (evita backpressure em segundo plano); só a ativa desenha. Repaint por `Notify` (CPU ociosa ~0).
- **Configurações consumidas**: tamanho da fonte, confirmação de colagem multilinha (prévia truncada + nº de linhas; "Cancelar" é o padrão), tema claro/escuro → `TermTheme`.
- **Fechar**: aba/janela com sessão ativa pede confirmação; ao sair, desconecta todas as sessões e espera até 3 s o desligamento. Cofre: `tick` por frame/`request_repaint_after(5 s)`.
- **Jump host** no perfil → erro "ainda não suportado nesta versão", sem conectar. **Keyboard-interactive** → erro claro (sem conectar). **Agente** usa o que `myssh-ssh` já suporta (Unix).

## Como executar
```
./testenv/up.sh
cargo run -p myssh-app                      # app (MYSSH_DATA_DIR=dir para dados isolados)
MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=$PWD/testenv/keys \
  cargo test -p myssh-app real_ -- --ignored --nocapture --test-threads=1
```
`--test-threads=1` é obrigatório (PerSourcePenalties do sshd: falhas de autenticação e conexões fechadas antes de autenticar somam penalidade; os testes que as provocam esperam 12 s no fim). Capturas: `MYSSH_DEMO_SHOTS=dir ... real_capturas`. Janela real com SSH: `MYSSH_DEMO_SHOTS=dir MYSSH_DEMO_SSH=127.0.0.1:2222 MYSSH_DEMO_KEY=.../id_ed25519 MYSSH_DATA_DIR=tmp myssh`.

## O que foi testado (servidor real 127.0.0.1:2222, egui_kittest headless)
| Teste (`real_tests.rs`) | Cobre |
|---|---|
| `real_senha_perguntar_host_desconhecido_comando_e_reconectar` | modal de senha, host desconhecido (aceitar e salvar), prompt, `echo ok-$((1+1))` → `ok-2`, desconectar, reconectar (tela limpa, comando antigo não reenviado), segredo ausente do disco (SQLite+WAL) |
| `real_host_key_alterada_bloqueia_e_esquecer_volta_ao_fluxo_desconhecido` | chave falsa pré-gravada bloqueia (sem botões de aceitar, store intacto), Abrir hosts confiáveis → Esquecer (confirmação) → novo fluxo de host desconhecido → Rejeitar não salva |
| `real_senha_errada_mostra_erro_e_tentar_novamente_pergunta_de_novo` | erro claro, "Tentar novamente" pergunta de novo, depois conecta |
| `real_senha_salva_no_cofre_e_opcao_salvar` | senha salva conecta sem modal; "perguntar" + "Salvar no cofre" (desmarcado por padrão) cria credencial e atualiza o perfil |
| `real_chave_com_passphrase_perguntar_e_errada` | chave criptografada pede passphrase, errada → erro, retry pergunta de novo |
| `real_dez_abas_simultaneas_com_saida_continua` | 10 abas (5 chave, 5 senha do cofre), flood `yes \| head -c 5000000`, medidas abaixo, desconectar uma não afeta as outras, encerrar fecha as 10 (servidor fica com 0 sessões) |
| `real_saida_dos_testes_sem_segredos` | grep automatizado de `tester-pass`/`test-passphrase` na saída completa (`MYSSH_TEST_LOG=arquivo`): 0 ocorrências (também nos Debug de perfis/credenciais/estados e nos bytes do banco) |
Sem rede: jump host, keyboard-interactive, cofre bloqueado → desbloquear → "conexão recusada", fechar aba ocupada. Suítes `ssh` (13) e `term` (3) com servidor continuam verdes.

### Medidas (10 abas conectadas, saída contínua de 5 MB em uma aba; `Harness::step()` = um frame headless completo, sem upload de GPU, com AccessKit ligado)
| | build debug | build release |
|---|---|---|
| flood na aba **ativa** (frames com saída) | p50 54–60 ms, p95 59–69 ms | p50 0,7–0,8 ms, p95 1,4–1,6 ms |
| flood em aba de **segundo plano** | p50 24–31 ms, p95 35–37 ms | p50 0,7–0,8 ms, p95 1,4–1,7 ms |
| RSS do processo: base → 10 sessões → após 2 floods | 31 → 37 → 75 MB | 16,5 → 26 → 59 MB |
| encerrar as 10 sessões | 0,03 s | 0,02 s |
Ocioso: `try_run()` não encontra repaint contínuo; app real sem sessões a 0,0 % de CPU. O RSS após flood reflete o scrollback cheio (5 000 linhas × sessões) e buffers; não foi medido o retorno após limpar.

### Janela real
`ViewportCommand::Screenshot` na janela real **não pôde ser validada**: a tela do Mac estava bloqueada (`CGSSessionScreenIsLocked`) e a janela não pinta, então o screenshot nunca chega (o modo demo agora aborta em vez de girar a 100 % de CPU). Evidência visual vem do render offscreen (`real_capturas`, wgpu): senha, host desconhecido, conectado em produção, colagem multilinha, chave alterada, hosts confiáveis.

## Limitações
- Reconectar **limpa** a tela (não há separador "— reconectado —"; o scrollback anterior fica visível até clicar).
- Jump host e keyboard-interactive/MFA: M2. Agente só em Unix.
- Salvar segredo no cofre só se oferece para perfis salvos sem credencial; trocar uma senha salva que foi recusada não atualiza o cofre (digita-se na hora).
- Modais de aba aparecem um por vez; com um diálogo do app aberto, o modal da aba espera (a host key expira em 120 s).
- O campo de senha é um `TextEdit` do egui (sem zeroização garantida do buffer interno; ver ADR 0001). O valor exposto à acessibilidade foi verificado como mascarado.
- Windows/Linux e VoiceOver não testados; dead keys/IME continuam pendentes (ADR 0001).
- Glifos de estado usam o conjunto embutido do egui (`●` não existe; usamos `✔ ○ ✖ ⏳ 🔒 🔑 ⛔`).

## Mudanças fora do `app`
- `crates/ssh`: `key_requires_passphrase(path)` (nova função pública) e export de `AUTH_REJECTED`/`KEY_DECODE`. Sem mudança de comportamento.
- `crates/core`: **nenhuma**. Sugestão: expor um tipo de erro estruturado em `SessionEvent::Closed` (hoje a UI compara o texto pt-BR com as constantes exportadas).
