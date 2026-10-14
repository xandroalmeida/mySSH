# Dependências diretas

Gerado a partir de `cargo metadata` (versões = requisitos do `Cargo.toml`; as versões exatas estão em `Cargo.lock`). Política de licenças e auditoria: `deny.toml` (`cargo deny check`).

Dependências diretas de crates do workspace (`myssh-*`) entre si não são listadas.

## myssh-app

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| eframe | 0.36 | MIT OR Apache-2.0 | Janela e loop da UI (winit + wgpu) |
| egui | 0.36 | MIT OR Apache-2.0 | UI imediata |
| directories | 6 | MIT OR Apache-2.0 | Diretórios de dados por SO |
| rfd | 0.17 | MIT | Diálogos de arquivo nativos |
| tokio | 1 | MIT | Runtime assíncrono |
| secrecy | 0.10 | Apache-2.0 OR MIT | Tipos para segredos |
| zeroize | 1 | Apache-2.0 OR MIT | Limpeza de memória de segredos |
| png | 0.18 | MIT OR Apache-2.0 | Decodificar o ícone da janela |
| winresource (build, Windows) | 0.1 | MIT | Ícone/metadados do .exe |
| egui_kittest (dev) | 0.36 | MIT OR Apache-2.0 | Testes de UI |
| tempfile (dev) | 3 | MIT OR Apache-2.0 | Arquivos temporários em testes |

## myssh-core

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| serde | 1 | MIT OR Apache-2.0 | Serialização |
| thiserror | 2 | MIT OR Apache-2.0 | Tipos de erro |
| uuid | 1 | Apache-2.0 OR MIT | Identificadores |

## myssh-secrets

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| keyring | 4.2 | MIT OR Apache-2.0 | Cofre do SO (Keychain / Credential Manager / Secret Service) |
| secrecy | 0.10 | Apache-2.0 OR MIT | Tipos para segredos |
| zeroize | 1 | Apache-2.0 OR MIT | Limpeza de memória |
| thiserror | 2 | MIT OR Apache-2.0 | Erros |
| uuid | 1 | Apache-2.0 OR MIT | Referências aleatórias de segredos |

## myssh-ssh

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| russh | 0.64 | Apache-2.0 | Cliente SSH |
| tokio | 1 | MIT | Runtime assíncrono |
| secrecy / zeroize | 0.10 / 1 | Apache-2.0 OR MIT | Segredos |

## myssh-store

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| rusqlite (feature `bundled`) | 0.37 | MIT | SQLite embutido |
| serde / serde_json | 1 | MIT OR Apache-2.0 | Export/import de configuração em JSON |
| thiserror / uuid | 2 / 1 | MIT OR Apache-2.0 | Erros / ids |

## myssh-sync

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| globset | 0.4 | Unlicense OR MIT | Padrões de exclusão |
| unicode-normalization | 0.1 | MIT OR Apache-2.0 | Normalização de nomes (NFC/NFD entre SOs) |
| serde / serde_json / thiserror / tokio | — | MIT OR Apache-2.0 / MIT | Serialização, erros, assíncrono |

## myssh-term

| Crate | Versão | Licença | Propósito |
|---|---|---|---|
| alacritty_terminal | 0.26 | Apache-2.0 | Emulação de terminal |
| egui | 0.36 | MIT OR Apache-2.0 | Renderização |

## myssh-sshconfig

Sem dependências externas.

## Ferramentas (fora do binário)

| Crate | Licença | Propósito |
|---|---|---|
| resvg 0.45 (`tools/icon`) | Apache-2.0 OR MIT | Gerar PNG/ICO/ICNS do ícone SVG |
| `poc/egui` | — | Protótipo, não distribuído |

## Licenças no grafo completo

Permitidas em `deny.toml`: MIT, Apache-2.0 (incl. LLVM-exception), BSD-2/3, ISC, Zlib, Unicode-3.0, MPL-2.0, CC0-1.0, 0BSD, BSL-1.0, OFL-1.1 e Ubuntu-font-1.0 (estas duas só nas fontes embutidas do egui, `epaint_default_fonts`). Nenhuma licença copyleft forte (GPL/AGPL) no grafo.

## Auditoria (cargo deny / cargo audit, 2026-10-14)

Ignorados em `deny.toml` com justificativa, a revisar a cada release:

| ID | Crate | Severidade | Situação |
|---|---|---|---|
| RUSTSEC-2023-0071 | rsa 0.10.0-rc (via russh/ssh-key) | média (5.9) | Marvin Attack (timing em RSA). Sem correção upstream. Está no caminho de execução do app via russh. |
| RUSTSEC-2026-0192 | ttf-parser 0.25 | aviso (unmaintained) | Transitiva de winit/sctk-adwaita e resvg. |
| RUSTSEC-2026-0206 | rustybuzz 0.20 | aviso (unmaintained) | Apenas `tools/icon` (resvg); fora do binário. |
