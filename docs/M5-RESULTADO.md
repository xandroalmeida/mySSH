# M5 — distribuição (resultado parcial: macOS arm64)

Feito tudo o que é possível nesta máquina (macOS arm64). Windows e Linux **não foram executados**; ver `docs/VALIDACAO.md`.

## Revisão de segurança (crates/app)
- **Ganchos de demo/teste** (`MYSSH_DEMO_SHOTS`, `MYSSH_DEMO_SSH`, `MYSSH_DEMO_KEY`, `demo_accept_host_key`, `simulate_busy`, `App::is_idle`) só compilam com `cfg(any(test, debug_assertions, feature = "demo"))`. A feature `demo` (`default = []`) existe em `crates/app/Cargo.toml`. Sem ela, o release usa `demo_off.rs` (stub sem leitura de env). Verificação: `strings target/release/myssh | grep -c MYSSH_DEMO` → **0** (e 0 também dentro do `.app`); com `--features demo` aparecem. A mensagem "exige MYSSH_DATA_DIR" foi movida para dentro do módulo demo (não fica no release).
- `MYSSH_DATA_DIR` e `MYSSH_MEMORY_VAULT` continuam e estão documentados (`README.md`, `docs/DADOS.md`).
- `paths.rs::prepare_db` aperta diretório (0700), banco e `-wal`/`-shm`/`-journal` (0600) pré-existentes em unix (teste `aperta_permissoes_frouxas_preexistentes`).
- `config_io.rs::export_to` grava arquivo temporário novo (`create_new`, 0600) e faz rename; sem sobra do temporário (teste `exporta_com_0600_mesmo_sobre_arquivo_frouxo`).
- Os testes de UI continuam rodando: debug e `cfg(test)` incluem os ganchos.

## Pacote macOS (executado)
`cargo build --release -p myssh-app` seguido de `cargo packager --release -p myssh-app --formats app,dmg` (a partir da raiz; também funciona de dentro de `crates/app`).
- **Descoberta**: o cargo-packager não compila; empacota o `target/release` existente. `release.yml` não compilava antes: foi adicionado o passo `cargo build --release -p myssh-app --locked` e uma checagem `strings | grep MYSSH_DEMO` (unix) antes do empacotamento. Nomes de formatos (`app,dmg`) e flags (`-p`, `--formats`, `--out-dir`) estão corretos; `nsis,wix` e `deb,appimage` não puderam ser testados.
- `mySSH.app` 28 MB (binário arm64 thin), `mySSH_0.1.0_aarch64.dmg` **12 449 930 bytes (~11,9 MiB)**.
- `Info.plist`: `CFBundleName`/`DisplayName` mySSH, `CFBundleIdentifier` com.myssh.app, `CFBundleShortVersionString` 0.1.0, `CFBundleIconFile` icon.icns (1024 px, presente em `Contents/Resources`), `NSHighResolutionCapable` true, `LSApplicationCategoryType` public.app-category.developer-tools. Observações: `CFBundleVersion` é um carimbo de data (20261014.xxxxxx); `LSRequiresCarbon` é emitido pelo packager.
- Abertura: `open -n --env MYSSH_DATA_DIR=<tmp> mySSH.app`; processo vivo durante 9 s, RSS ~101 MB e CPU 0–4 % (arranque); fechado com `osascript … quit`. Banco criado 0600/0700 com WAL/SHM 0600.
- **NÃO assinado nem notarizado** (assinatura ad-hoc de linker apenas). O Gatekeeper avisará; instruções "Abrir mesmo assim" em `docs/USO.md`.

## Janela real (executado)
Build release com `--features demo`, `MYSSH_DATA_DIR` temporário, contra o testenv (`127.0.0.1:2222`, chave ed25519 sem passphrase): a janela pintou (tela desbloqueada) e `ViewportCommand::Screenshot` salvou 4 capturas da janela do app (sem `screencapture`): host desconhecido, conectado, **10 abas conectadas** e **painel de arquivos aberto**. Novos passos de demo: `32-dez-abas`, `33-painel-de-arquivos`; ao fim da fase ociosa a demo confirma o encerramento sozinha (antes ficava em laço quando havia sessões ativas). Capturas: `scratchpad/m5-shots/` (a `33` mostra o home real do usuário e **não** foi copiada para `docs/img`).

Medidas do processo real (release, 10 sessões conectadas, ociosas; `ps`, 4 amostras em 8 s):
| | RSS | CPU |
|---|---|---|
| app sem sessões (pacote, ~9 s) | ~101 MB | 0,0–4,1 % (arranque) |
| 10 sessões, painel de arquivos aberto, ocioso | ~147 MB | 0,0–2,4 % (média ~0,9 %) |
RSS inclui GPU/wgpu e a janela; a medida de 10 abas com flood (headless) está em `docs/M1-RESULTADO.md`.

## Documentação
`README.md`, `docs/USO.md`, `docs/VALIDACAO.md`, este arquivo, `docs/DADOS.md` atualizado e 4 renders em `docs/img/` (`dez-abas`, `paineis-divididos`, `arquivos-conflito`, `sync-previa`; dados fictícios, sem informação pessoal).

## Verificações
- `cargo fmt --all`: ok.
- `cargo clippy --workspace --all-targets -- -D warnings`: ok. `cargo clippy -p myssh-app --release` (sem features): ok, sem avisos. `--release --features demo`: ok.
- `cargo test --workspace`: verde (app: 161 passam, 19 ignorados).
- `MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=…/testenv/keys cargo test -p myssh-app -- --ignored --test-threads=1 --skip real_saida`: **18 passaram, 0 falharam** (110 s).

## Não validado
Windows e Linux (CI nunca rodou; sem remote GitHub), assinatura/notarização, Keychain real na janela empacotada (usou cofre em memória), IME/teclas mortas, VoiceOver, formatos `nsis,wix,deb,appimage`. Detalhes e checklist: `docs/VALIDACAO.md`.
