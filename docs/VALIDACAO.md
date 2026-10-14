# Validação por plataforma

Data: 2026-10-14. Versão 0.1.0.

## Matriz

| Item | macOS arm64 | Windows | Linux |
|---|---|---|---|
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` | **Executado** (verde) | NÃO executado (só CI preparada) | **Executado** (verde; Ubuntu 24.04 x86_64, Rust 1.99; o clippy exigiu uma correção, ver abaixo) |
| Testes com servidor SSH real (Docker) | **Executado** | NÃO executado | **Executado** (revelou uma corrida no "Renomear", corrigida) |
| Build release | **Executado** | NÃO executado | **Executado** |
| Pacote | **Executado**: `.app` + `.dmg` | NÃO executado (`nsis,wix` na CI) | **Executado**: `.deb` + `.AppImage` (~15 MB cada; `make package-linux`) |
| Abrir o app empacotado | **Executado** (processo vivo, fechado por AppleScript) | NÃO | **AppImage executado** (processo vivo 10 s, banco 0600/0700); `.deb` não instalado |
| Janela real + screenshots da janela | **Executado** (build release com `--features demo`) | NÃO | **Executado** (build debug, Wayland/GNOME, Intel Ivy Bridge via Vulkan) |
| Keyring do SO | Keychain: só por testes de `crates/secrets` (a janela de validação usou cofre em memória) | NÃO | Secret Service (GNOME Keyring): só por `os_keyring_real` com `MYSSH_TEST_KEYRING=1` |
| UI/DPI, IME, teclas mortas, VoiceOver/leitores | NÃO validado em teclado real | NÃO | NÃO |
| Assinatura/notarização | NÃO (sem certificados) | NÃO | n/a |

A CI (`.github/workflows/ci.yml`, `release.yml`) **nunca foi executada**. Os erros que ela pode revelar no Windows (dependências de sistema, nomes de formatos do cargo-packager) são desconhecidos; no Linux os mesmos passos foram executados à mão (abaixo).

## Checklist manual por SO (a fazer antes de uma versão pública)

### Todos
- [ ] Instalar o pacote em máquina limpa; abrir; aviso de app não assinado conforme `docs/USO.md`.
- [ ] Criar perfil, conectar por senha, chave (com e sem passphrase), agente; host key desconhecida/alterada.
- [ ] 10 abas simultâneas, painel dividido, painel de arquivos, sincronização com prévia.
- [ ] Fechar com sessão ativa pede confirmação; reabrir mantém perfis; exportar/importar JSON.
- [ ] Atualizar substituindo o app: o banco abre e migra.

### macOS
- [ ] Keychain: salvar, reiniciar o app, conectar com senha salva; negar acesso no prompt do Keychain vira erro claro.
- [ ] Retina/monitor externo (DPI misto); teclas mortas (´ ~ ^) e IME (japonês/chinês) no terminal; Cmd+C/V/T/W/N/F.
- [ ] Gatekeeper: "Abrir mesmo assim" funciona; `.dmg` baixado (quarentena).
- [ ] VoiceOver lê abas, diálogo de host key e campo de senha mascarado.

### Windows
- [ ] Credential Manager: **limite de 2560 bytes por segredo** (chaves/passphrases enormes devem falhar com erro claro "Segredo grande demais", não silenciosamente).
- [ ] Escala de DPI 100/150/200 %, múltiplos monitores; IME e teclas mortas; Ctrl+C sem seleção envia `^C`, Ctrl+V cola, `Ctrl+Shift+…` com terminal focado.
- [ ] **Agent**: o agente do sistema (OpenSSH Agent via named pipe / Pageant) está implementado só para Unix; verificar o que o app mostra no Windows (esperado: indisponível, com mensagem clara) e documentar.
- [ ] Instalador NSIS e WiX: instalar, desinstalar (dados do usuário permanecem), ícone e SmartScreen.
- [ ] Caminhos com `C:\`, nomes com acentos/espaços no painel de arquivos; ssh config com caminhos Windows.

### Linux
- [ ] Secret Service **bloqueado** (keyring trancado ou sem daemon): o app deve mostrar "cofre indisponível" e oferecer o cofre em memória somente por ação explícita; sem segredo em arquivo.
- [ ] X11 e Wayland (escala fracionária, clipboard, drag-and-drop de arquivos), GPU sem Vulkan (fallback).
- [ ] IME (ibus/fcitx) e teclas mortas; atalhos `Ctrl+Shift+…`.
- [ ] `.deb` (ícones hicolor e `.desktop`), AppImage (`libfuse2`). AppImage abre (Ubuntu 24.04 com `libfuse2t64`); `.deb` gerado mas não instalado, e seus metadados vêm do cargo-packager (pacote `my-ssh`, sem Maintainer, `Categories=Development`, só ícone 256 px) em vez de `assets/myssh.desktop`.

## Linux: execução de 2026-10-14
Ubuntu 24.04.5 x86_64, kernel 6.17, sessão Wayland (GNOME), GPU Intel Ivy Bridge (Mesa avisa "Ivy Bridge Vulkan support is incomplete", sem efeito visível), Rust 1.99, Docker 29.
- **clippy**: `large_enum_variant` em `Reader`/`Writer` (`crates/transfer/src/side.rs`) só dispara no Linux (`tokio::fs::File` tem 104 bytes contra 312 do `SftpFile`). O lado remoto passou a ir numa `Box`.
- **Corrida no "Renomear"** (`conflitos_overwrite_skip_rename_ask`, falhava na suíte completa): o nome livre escolhido para um arquivo podia ser ocupado por outro arquivo do mesmo lote durante a cópia, e o `rename` local substituía o destino, sumindo com um dos arquivos sem erro. Afetava todas as plataformas. Agora o modo Renomear publica sem substituir (`Side::publish_new`: `hard_link` no local, `SSH_FXP_RENAME` padrão no remoto) e, se o nome tiver sido ocupado, usa o próximo livre.
- **Testes**: 430 unitários/UI, 74 de integração com o testenv, `os_keyring_real` no GNOME Keyring.
- **Janela real**: `MYSSH_DATA_DIR` temporário, `MYSSH_DEMO_SSH`/`MYSSH_DEMO_KEY` contra o testenv; as 4 capturas (host desconhecido, conectado, 10 abas, painel de arquivos) renderizaram corretamente.
- **Makefile**: `deps-linux`, `package-linux`, `install-linux`/`uninstall-linux` (em `PREFIX`, padrão `~/.local`, sem sudo) e `install-deb`. `install-linux` e `uninstall-linux` testados em `PREFIX` temporário; `deps-linux` e `install-deb` (sudo) não executados.
- **Não validado**: X11, escala fracionária, IME/teclas mortas, drag-and-drop, keyring bloqueado, instalação do `.deb`, outras distribuições.

## Advisory conhecido: RUSTSEC-2023-0071 (rsa, "Marvin Attack")
- **Origem**: `rsa` 0.10.0-rc, transitiva de `russh`/`ssh-key`. Sem correção upstream; ignorado em `deny.toml` com justificativa.
- **Risco**: canal lateral de tempo (severidade média, 5,9) em operações RSA com a chave privada (assinatura/decifragem) quando um atacante consegue medir muitos tempos de resposta. Chaves RSA estão no caminho de execução do app.
- **Mitigação recomendada**: **prefira chaves ed25519** (e agente SSH com ed25519) para autenticação; evite RSA para perfis expostos a redes não confiáveis; mantenha o app atualizado e reavalie o advisory a cada release (`cargo deny check advisories`).

## Ganchos de demonstração
`MYSSH_DEMO_SHOTS`/`MYSSH_DEMO_SSH`/`MYSSH_DEMO_KEY` e a aceitação automática de host key só existem em debug, em testes ou com `--features demo`. O release padrão foi verificado com `strings` (0 ocorrências de `MYSSH_DEMO`).
