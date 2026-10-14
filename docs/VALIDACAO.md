# Validação por plataforma

Data: 2026-10-14. Versão 0.1.0.

## Matriz

| Item | macOS arm64 | Windows | Linux |
|---|---|---|---|
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` | **Executado** (verde) | NÃO executado (só CI preparada) | NÃO executado (só CI preparada) |
| Testes com servidor SSH real (Docker) | **Executado** | NÃO executado | NÃO executado (CI `integration-linux` preparada) |
| Build release | **Executado** | NÃO executado | NÃO executado |
| Pacote | **Executado**: `.app` + `.dmg` | NÃO executado (`nsis,wix` na CI) | NÃO executado (`deb,appimage` na CI) |
| Abrir o app empacotado | **Executado** (processo vivo, fechado por AppleScript) | NÃO | NÃO |
| Janela real + screenshots da janela | **Executado** (build release com `--features demo`) | NÃO | NÃO |
| Keyring do SO | Keychain: só por testes de `crates/secrets` (a janela de validação usou cofre em memória) | NÃO | NÃO |
| UI/DPI, IME, teclas mortas, VoiceOver/leitores | NÃO validado em teclado real | NÃO | NÃO |
| Assinatura/notarização | NÃO (sem certificados) | NÃO | n/a |

A CI (`.github/workflows/ci.yml`, `release.yml`) **nunca foi executada**: o repositório não tem remote no GitHub. Os erros que ela pode revelar (dependências de sistema, nomes de formatos do cargo-packager no Windows/Linux) são desconhecidos.

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
- [ ] `.deb` (ícones hicolor e `.desktop`), AppImage (`libfuse2`).

## Advisory conhecido: RUSTSEC-2023-0071 (rsa, "Marvin Attack")
- **Origem**: `rsa` 0.10.0-rc, transitiva de `russh`/`ssh-key`. Sem correção upstream; ignorado em `deny.toml` com justificativa.
- **Risco**: canal lateral de tempo (severidade média, 5,9) em operações RSA com a chave privada (assinatura/decifragem) quando um atacante consegue medir muitos tempos de resposta. Chaves RSA estão no caminho de execução do app.
- **Mitigação recomendada**: **prefira chaves ed25519** (e agente SSH com ed25519) para autenticação; evite RSA para perfis expostos a redes não confiáveis; mantenha o app atualizado e reavalie o advisory a cada release (`cargo deny check advisories`).

## Ganchos de demonstração
`MYSSH_DEMO_SHOTS`/`MYSSH_DEMO_SSH`/`MYSSH_DEMO_KEY` e a aceitação automática de host key só existem em debug, em testes ou com `--features demo`. O release padrão foi verificado com `strings` (0 ocorrências de `MYSSH_DEMO`).
