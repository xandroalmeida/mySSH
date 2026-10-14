# PoC M0 — Alternativa A: eframe/egui 0.36 + alacritty_terminal 0.26 + russh 0.64

Testado só em **macOS (arm64, retina)**. Windows/Linux: não testados.

## O que funcionou (com evidência)

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` (6 unitários) passam.
- Integração (`MYSSH_TEST_SSH=127.0.0.1:2222 MYSSH_TEST_KEYS=<dir> cargo test -- --ignored`, 6 testes OK, debug e release), em `poc/egui/tests/integration.rs`, usando o mesmo `session`/`terminal` do app, sem UI:
  - host key errada (fingerprint pinado de outra chave) rejeitada; host key real vem de `ssh-keyscan`;
  - prompt de host desconhecido → rejeitar não grava; aceitar grava; segunda conexão sem prompt; arquivo com outra chave para o mesmo host:porta → bloqueio com "a chave do host mudou", sem oferecer aceitar;
  - senha (certa/errada), chave ed25519 sem e com passphrase (certa/errada);
  - PTY 80x24 (`stty size` = `24 80`, `TERM=xterm-256color`), `vim` entra em tela alternativa e a grade mostra `~`; `:q` sai; `top` interativo mostra "load average"; `top -b -n1 | head -3`; resize 120x40 → `stty size` = `40 120`; DSR (`ESC[6n`) respondido pelo `PtyWrite` do Term e chega ao servidor.
- GUI real (macOS) via `poc/egui/demo.sh` (`MYSSH_POC_DEMO=1`): injeta eventos egui reais (`Event::Text`, `Event::Key`), aceita o diálogo só se o fingerprint bater, `ViewportCommand::InnerSize` para redimensionar. 5/5 checks OK (debug e release). Screenshots (feitos pelo próprio egui via `ViewportCommand::Screenshot`, só a janela do app), em `poc/egui/evidence/`:
  - `01-dialogo-fingerprint.png` diálogo com `SHA256:...` (rodada sem fingerprint pré-aprovado; rejeita após a captura);
  - `02-shell-stty-antes.png` shell, `echo acentos: á é ç ã` (Event::Text com acentos), `stty size` = `34 105`;
  - `03-vim.png` vim em tela alternativa (`~`, cores, cursor);
  - `04-top.png` top (bold, linha de cabeçalho invertida);
  - `05-stty-apos-resize.png` após resize para 1280x800: grade 150x46, `stty size` = `46 150`.
- Observações: `top` do procps NÃO usa tela alternativa neste servidor (o teste não exige alt-screen para ele). `screencapture` do SO funciona sem prompt de permissão aqui, mas captura a tela inteira (inclui outras janelas/dados privados), por isso NÃO foi usado nas evidências nem versionado.
- Tamanhos/medidas (release, M-series, 10 núcleos):
  - build release limpo: ~35 s de relógio (337 s de CPU); binário 18,6 MB (sem strip/LTO);
  - GUI conectada, 1 sessão, ociosa: **CPU 0,0%** (≈0,01 s de CPU em 8 s; só repinta com evento/dados), RSS ~165–175 MB (janela retina 1280x800 + wgpu/Metal);
  - headless (sem GUI): processo de teste 12 MB base; +~2,3 MB por sessão (russh+Term, com `top` rodando): 1 sessão 14,3 MB, 10 sessões 35,2 MB RSS. Scrollback limitado a 5000 linhas por sessão.

## Arquitetura
`known_hosts.rs` (arquivo próprio `[host]:porta algoritmo chave`), `session.rs` (russh em runtime tokio de 2 threads; canais mpsc limitados: eventos 32 / comandos 256; `check_server_key` valida antes de autenticar; prompt via oneshot, timeout 120 s), `terminal.rs` (Term + `vte::ansi::Processor`, listener que só repassa `PtyWrite`/`ColorRequest`/`TextAreaSizeRequest`; OSC 52 `Disabled`; clipboard/título/URL da saída ignorados; processa ≤256 KiB por frame, o resto fica no canal limitado, o que dá backpressure ao SSH), `widget.rs` (grade, cursor, bold/dim/inverso/sublinhado/tachado, seleção, scroll, IME, AccessKit), `keys.rs` (xterm, app cursor), `demo.rs`, `i18n.rs`.

## Investigação
- **Acessibilidade**: eframe 0.36 tem `accesskit` por padrão. O widget publica um nó `Role::Terminal` com label e o texto visível como `value`. **Não verificado com VoiceOver/leitor real** (só compila e roda). egui expõe widgets padrão; para o terminal, a qualidade depende desse nó manual (sem navegação por linha/seleção/eventos de texto novo).
- **IME/dead keys**: o widget habilita IME (`output.ime`, `IMEPurpose::Terminal`) e trata `ImeEvent::Commit`; `Preedit` NÃO é desenhado (a letra intermediária de dead key/CJK não aparece). Provado: `Event::Text("á é ç ã")` chega ao servidor e ecoa certo. **Não provado**: digitação real de dead key no macOS (a demo injeta eventos egui, não passa por winit/NSTextInputClient). Risco real para pt-BR/CJK.
- **Copiar/colar/seleção**: seleção por mouse (Term `Selection`), `Event::Copy` → `ctx.copy_text` só por ação do usuário, `Event::Paste` com bracketed paste e remoção de ESC. Não testado manualmente. **Risco Windows/Linux (lido no código do egui-winit)**: Ctrl+C/V/X viram `Event::Copy/Paste/Cut` (Ctrl+Shift+C também), sem Key event: não dá para enviar ^C/^V/^X puros de forma limpa. O PoC envia ^C/^X quando não há seleção; ^V literal é impossível sem patch no egui-winit. Mouse reporting para apps (vim `mouse=a`) NÃO implementado.
- **DPI/retina**: métrica de célula arredondada ao pixel físico; screenshots a 2x nítidos. Monitores de DPI misto não testados.
- **Menus nativos**: egui não tem; opções: crate `muda` (menu bar nativo macOS/Win/GTK; no Linux exige GTK) integrado ao loop winit do eframe, ou menus egui próprios. Não testado. Diálogos: o modal é desenhado pelo egui (ok); diálogos de arquivo nativos exigiriam `rfd`.
- **Empacotamento previsto**: `cargo-bundle` (.app/.deb) ou `cargo-packager` (.app/.dmg/.msi/.AppImage/NSIS). Assinatura/notarização macOS a cargo do pipeline. Não testado.
- **Licenças diretas**: alacritty_terminal Apache-2.0; russh Apache-2.0; eframe/egui MIT OR Apache-2.0; tokio MIT; directories MIT OR Apache-2.0; secrecy/zeroize Apache-2.0 OR MIT; png MIT OR Apache-2.0. 547 pacotes no grafo total (não auditado).
- **Fontes**: fonte monoespaçada padrão do egui (Hack); sem bold/italic reais (bold = cor brilhante + dupla impressão; itálico ignorado); sem fallback CJK/box-drawing avançado verificado; células largas tratadas só pelas flags do Term.

## Limitações e riscos
- Só macOS testado. Linux (X11/Wayland) e Windows não testados; riscos: Ctrl+C acima, IME em Wayland, `wgpu` em VMs sem GPU (há backend `glow` como alternativa, não testado).
- Sem reconexão, sem keepalive configurável na UI, sem agente SSH, sem known_hosts do OpenSSH, sem hashing de hosts. Mudança de host key só bloqueia (sem fluxo de substituição).
- Segredos: `SecretString` + `Zeroizing` para o arquivo da chave; campos de formulário zerados após conectar. Limitações: `authenticate_password` do russh recebe `String` (cópia não zerável); o `TextEdit` do egui mantém histórico de undo/buffer interno dos campos de senha/passphrase sem garantia de zeroização.
- Se o canal de comandos (256) enche com o servidor inundando, teclas são descartadas (`try_send`); a UI nunca bloqueia.
- O timeout de conexão (15 s + 120 s do prompt) cobre TCP+handshake+decisão do usuário.
- Demo: roda com a janela sempre por cima e foco forçado (o macOS não redesenha janelas totalmente cobertas: sem isso o app parece travar). Teclas reais digitadas durante a demo vão para o terminal (aconteceu uma vez e falhou um check; reexecução passou).
- Cursor aparece oco quando a janela não tem foco.

## Compatibilidade de versões
- egui 0.36: `App::ui(&mut Ui, &mut Frame)` (não mais `update`), `Panel::top`/`CentralPanel::show(ui, ..)` no lugar de `TopBottomPanel`, `Modal`, `ctx.fonts_mut`, `Context::input_mut` para injetar eventos; `egui_term 0.1` (egui 0.31) é incompatível.
- alacritty_terminal 0.26: `term::test::TermSize` não é público fora de testes (implementar `Dimensions`); `Colors` é indexado por `usize` (< 269); processador vem de `alacritty_terminal::vte::ansi::Processor` (vte 0.15).
- russh 0.64: `PublicKeyOrCertificate` em `check_server_key` (certificados precisam de tratamento explícito); `russh::keys` reexporta `ssh-key 0.7.0-rc` (pré-release); `Handler::Error` precisa de `From<russh::Error>`.
- Sem conflitos de versão de resolução; `png` alinhado em 0.18 (mesma do egui).
