# Changelog

## mySSH v0.1.0 — 2026-10-14

Primeira versão do mySSH, um gerenciador desktop de sessões SSH escrito em Rust. Roda localmente, sem conta e sem nuvem.

### Destaques
- Terminal SSH completo, com várias sessões em abas e painéis divididos. vim, top e tmux funcionam, e o terminal acompanha o redimensionamento da janela.
- A identidade do servidor é verificada sempre: na primeira conexão o app mostra o fingerprint SHA-256, e se a chave mudar a conexão é bloqueada.
- Senhas e passphrases ficam no Keychain do sistema, nunca em arquivo de texto.
- Painel de arquivos via SFTP, com fila de transferências, progresso e cancelamento.
- Sincronização de pastas com prévia. Nada é excluído por padrão.

### Novidades
- **Perfis:** cadastro com pastas, tags, favoritos, ambiente (DEV, HOMOLOGAÇÃO, PRODUÇÃO com aviso visual) e tema próprio. Busca, filtros e conexão rápida (`usuario@host:porta`).
- **Autenticação:**
  - senha, chave privada com ou sem passphrase e SSH agent;
  - MFA (keyboard-interactive);
  - jump host com um ou mais saltos, validando a chave de cada servidor.
- **Sessão:** estados claros (conectando, autenticando, conectado, erro), desconectar e reconectar. A reconexão abre uma sessão nova e não reenvia comandos. Keepalive, timeouts e detecção de queda de rede.
- **Terminal:** seleção, copiar e colar, histórico com rolagem e confirmação ao colar várias linhas. Seis temas, entre eles o Cyberpunk Neon.
- **Arquivos:**
  - navegação local e remota lado a lado, com arrastar e soltar (inclusive do Finder);
  - política de sobrescrita, conflitos, retomada de arquivos grandes e erros claros, como disco cheio ou permissão negada.
- **Sincronização:**
  - unidirecional nos dois sentidos, com prévia e conflitos decididos por você;
  - modo espelho só com confirmação explícita;
  - recusa executar uma prévia desatualizada e pode ser retomada.
- **Importação de `~/.ssh/config`:** com prévia. Diretivas que executariam comandos locais são ignoradas.
- **Cofre e dados:** bloqueio do cofre manual, por inatividade e ao suspender. Exportação e importação das configurações sem segredos.
- **Interface:** janela Sobre e ícone próprio.

### Segurança
- Algoritmos legados (`ssh-rsa`/SHA-1, CBC) desativados. Agent forwarding nunca é ativado automaticamente.
- Saída do servidor não acessa a área de transferência (OSC 52 desligado) nem abre links.
- **Proteções contra servidor malicioso:**
  - memória limitada para sequências de terminal gigantes;
  - downloads limitados ao tamanho declarado;
  - nomes de arquivo com caracteres de controle ou invisíveis recusados;
  - nenhum caminho escapa da pasta escolhida e symlinks não são seguidos.
- Banco de dados e arquivos temporários com permissões restritas (0600/0700). Nenhuma telemetria.

### Limitações conhecidas
- Validado somente no macOS (Apple Silicon). Windows e Linux compilam pelo projeto, mas não foram testados.
- O app não é assinado nem notarizado.
- Substituir um arquivo que já existe no servidor não é uma operação atômica. A retomada de transferências vale para arquivos a partir de 4 MiB.
- Teclas mortas e IME no teclado físico e o VoiceOver não foram validados. Ainda não há mouse dentro de apps como o tmux.
- A dependência `rsa`, usada pelo russh, tem um aviso de segurança sem correção (RUSTSEC-2023-0071). Prefira chaves ed25519.

### Instalação (macOS)
Rode `make install-mac` ou abra o `.dmg` anexado e arraste o mySSH para Aplicativos. Como o app não é assinado, na primeira abertura clique com o botão direito e escolha **Abrir**.
