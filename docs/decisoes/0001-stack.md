# 0001 — Stack de UI e terminal

**Decisão:** eframe/egui 0.36 + alacritty_terminal 0.26 (widget próprio de renderização) + russh 0.64 + tokio.

## Evidência (macOS 26.5 arm64, servidor Docker `testenv/`)
Duas provas feitas em paralelo, ambas com russh + alacritty_terminal e widget próprio:

| | egui (`poc/egui`) | iced 0.14 (branch `worktree-agent-a9e5fbf701e14d264`, não integrada) |
|---|---|---|
| Host key SHA-256, aceitar/rejeitar, bloqueio de chave alterada | ok (6/6 testes integração) | ok (5/5) |
| vim/top/resize (`stty size`) na GUI real | ok, screenshots | ok, screenshots |
| Acessibilidade | AccessKit nativo; widget expõe `Role::Terminal` (não validado com VoiceOver) | **sem AccessKit** no 0.14 |
| Acentos (`Event::Text`) | ok (á é ç ã ecoados) | não validado |
| CPU ociosa | 0,0% | 0–0,3% |
| RSS por sessão (headless) | ~2,3 MB | ~10,5 MB (debug) |
| Binário release | 18,6 MB | 16,3 MB |

iced_term 0.8 não aceita fonte de bytes SSH (spawna PTY local) e depende de `open`; egui_term 0.1 é incompatível com egui 0.36. Em ambos os casos o emulador continua sendo o alacritty_terminal — só a renderização é nossa.

Tauri foi descartado: exigiria WebView + frontend JS (xterm.js), contra a prioridade de UI em Rust.

## Controles nativos vs desenhados
egui desenha todos os controles (botões, campos, diálogos internos). Nativos: janela (winit), clipboard, IME, diálogo de arquivo (`rfd`, a adotar). Menu nativo: avaliar `muda` no M2; até lá, menu desenhado.

## Riscos aceitos / a tratar
- Dead keys/IME do macOS não provados em teclado real; `Preedit` não desenhado → validar manualmente no M1, implementar preedit no M2.
- egui-winit converte Ctrl+C/V/X em Copy/Paste/Cut no Windows/Linux: Ctrl+C sem seleção → enviar `^C`; Ctrl+V cola (como Windows Terminal). `^V` literal fica sem atalho direto — documentar.
- Buffer de `TextEdit` não tem zeroização garantida (senha digitada).
- Windows e Linux: não testados nesta máquina; CI de build no M5 e validação manual pendente.
