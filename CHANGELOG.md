# Changelog

## mySSH v0.3.0 — 2026-10-22

### Destaques
- **Conexão via AWS SSM Session Manager**: acesse instâncias EC2 sem IP público e sem porta 22 aberta. Não precisa do AWS CLI nem do session-manager-plugin: o túnel é feito pelo próprio app, e a host key do servidor continua sendo verificada.
- **Sem chave para preparar**: com a autenticação **EC2 Instance Connect**, o app gera uma chave temporária a cada conexão e a publica por 60 s na instância. Nada é gravado no servidor nem no seu computador.
- **Login no IAM Identity Center (SSO) dentro do app**: código no app, confirmação no navegador; os tokens ficam só no cofre do sistema.
- **Ajuda na tela**: o botão "Como configurar?" explica requisitos, campos, permissões IAM (com política de exemplo para copiar) e problemas comuns, em português, inglês e espanhol.

### Novidades
- Novo campo **Conexão** no perfil: Direta (TCP) ou AWS SSM Session Manager. Com SSM, o host é o ID da instância (`i-…`).
- Credenciais AWS de duas formas: **perfil AWS** (usa o que já está em `~/.aws`, inclusive `aws login`, SSO do AWS CLI e `credential_process`, com lista dos perfis configurados) ou **IAM Identity Center** (URL de início, região do SSO, conta e role). A sessão SSO é renovada sozinha enquanto o refresh token valer.
- Autenticação **EC2 Instance Connect**, sugerida automaticamente ao escolher SSM num perfil novo.
- Exportação e importação de configuração levam o tipo de conexão (sem segredos).

### Correções
- Textos de ajuda longos no formulário de perfil agora quebram linha; o diálogo não sai mais da janela.

### Segurança
- Tokens do IAM Identity Center e o registro do cliente OAuth ficam apenas no cofre do sistema, nunca em arquivo nem em log.
- A chave temporária do EC2 Instance Connect existe só em memória e é apagada ao fim da autenticação.
- O navegador só é aberto por clique no botão do diálogo de login.
- O app não usa o cliente HTTP legado do AWS SDK, que trazia versões antigas de `h2` e `rustls` com alertas de segurança.

### Limitações conhecidas
- Sessões com criptografia KMS ligada nas preferências do Session Manager ainda não são suportadas (o app mostra um erro explicando).
- EC2 Instance Connect só funciona em instâncias EC2 (`i-…`) com o pacote `ec2-instance-connect` (já presente em Ubuntu e Amazon Linux 2023).
- O login no IAM Identity Center foi testado contra a AWS só até o registro do cliente; o fluxo completo com uma organização real ainda não foi validado.
- Um perfil SSM não pode usar jump host.
- Mensagens de erro da AWS aparecem na tela como a AWS as envia e podem incluir ARNs e IDs de instância.
- Verificado no macOS. Windows não foi validado; no Linux, só Ubuntu 24.04 com Wayland foi testado em versões anteriores.
- Os pacotes não são assinados nem notarizados.
- Dependência com alerta de segurança conhecido para chaves RSA (RUSTSEC-2023-0071): prefira chaves ed25519.

### Instalação (macOS)
`make install-mac`, ou abra o .dmg anexado. O app não é assinado: na primeira abertura, clique com o botão direito e escolha Abrir.

### Instalação (Linux)
Baixe o AppImage anexado quando estiver disponível, marque-o como executável (`chmod +x`) e abra, ou instale o `.deb` (`sudo apt install ./myssh_0.3.0_amd64.deb`). Também dá para usar `make install-linux`.

## mySSH v0.2.1 — 2026-10-17

### Destaques
- O terminal agora exibe corretamente os gráficos de programas como o `btop`: os caracteres Braille, que antes apareciam como quadrados vazios, são desenhados normalmente.
- Nova fonte do terminal: Cascadia Mono, que cobre bordas e caixas, blocos, formas geométricas, Braille, boa parte dos Symbols for Legacy Computing e os separadores do Powerline.

### Correções
- Caracteres usados por ferramentas de terminal (gráficos em Braille, sextantes, separadores Powerline) não aparecem mais como quadrados vazios. A fonte anterior continua como reserva para o que a nova não tiver.

### Limitações conhecidas
- O negrito continua simulado (o texto é desenhado duas vezes, levemente deslocado), sem uma variante bold da fonte.
- Ícones de Nerd Fonts (usados por temas como os do starship ou do lsd) ainda não são exibidos.
- A correção foi verificada só no macOS. Windows não foi validado. No Linux, só Ubuntu 24.04 com Wayland foi testado em versões anteriores.
- Os pacotes não são assinados nem notarizados.
- Dependência com alerta de segurança conhecido para chaves RSA (RUSTSEC-2023-0071): prefira chaves ed25519.

### Licença de terceiros
- A fonte Cascadia Mono (© Microsoft) é distribuída sob a SIL Open Font License 1.1.

### Instalação (macOS)
`make install-mac`, ou abra o .dmg anexado. O app não é assinado: na primeira abertura, clique com o botão direito e escolha Abrir.

### Instalação (Linux)
Baixe o AppImage anexado quando estiver disponível, marque-o como executável (`chmod +x`) e abra, ou instale o `.deb` (`sudo apt install ./myssh_0.2.1_amd64.deb`). Também dá para usar `make install-linux`.

## mySSH v0.2.0 — 2026-10-15

### Destaques
- Interface em inglês, português (Brasil) e espanhol. O app segue o idioma do sistema e usa inglês quando ele não é um dos três.
- Novo seletor em Configurações → Idioma, com troca na hora e escolha salva.
- Mensagens de erro de conexão, transferência, sincronização e cofre também saem no idioma escolhido.
- Projeto agora é público, com página própria nos três idiomas: https://xandroalmeida.github.io/mySSH/

### Novidades
- Configurações → Idioma: "Automático (sistema)", English, Português (Brasil) ou Español. Qualquer variante de português usa pt-BR, e qualquer variante de espanhol usa es.
- Arquivos de licença no repositório (MIT ou Apache-2.0, à sua escolha).

### Mudanças de comportamento
- Nomes que o app grava ficam em inglês, independentemente do idioma da interface:
  - a pasta de perfis importados do `~/.ssh/config` agora se chama "Imported" (antes, "Importados"). Uma pasta "Importados" já existente não é reaproveitada: a próxima importação cria "Imported";
  - credenciais novas são rotuladas "Password for …" e "Passphrase for …";
  - perfis duplicados recebem o sufixo "(copy)".
  
  Itens já gravados não são renomeados.

### Limitações conhecidas
- As mensagens geradas no momento de uma falha continuam no idioma antigo se você trocar de idioma com elas na tela.
- Em inglês e espanhol, alguns textos são mais longos e podem apertar o layout de diálogos. A interface traduzida não foi revisada visualmente em todas as telas.
- Windows não foi validado. No Linux, só Ubuntu 24.04 com Wayland foi testado.
- Os pacotes não são assinados nem notarizados.
- Dependência com alerta de segurança conhecido para chaves RSA (RUSTSEC-2023-0071): prefira chaves ed25519.

### Instalação (Linux)
Baixe o AppImage anexado, marque-o como executável (`chmod +x`) e abra, ou instale o `.deb` (`sudo apt install ./myssh_0.2.0_amd64.deb`). Também dá para usar `make install-linux`.

### Instalação (macOS)
`make install-mac`, ou abra o .dmg anexado quando estiver disponível (os pacotes de cada plataforma são anexados separadamente). O app não é assinado: na primeira abertura, clique com o botão direito e escolha Abrir.

## mySSH v0.1.1 — 2026-10-14

### Destaques
- Primeira versão validada no Linux (Ubuntu 24.04, Wayland): compila, passa nos testes, abre a janela e conecta.
- Pacotes para Linux: `.deb` e AppImage.
- Correção de transferências: com a opção "Renomear", nenhum arquivo é mais perdido em conflitos de nome.

### Novidades
- Atalhos de instalação no Linux: `make deps-linux` instala as bibliotecas do sistema (Debian/Ubuntu), `make package-linux` gera `.deb` e AppImage, e `make install-linux` instala o app no seu usuário (`~/.local`, sem sudo), com ícone e atalho no menu. `make uninstall-linux` desfaz a instalação e mantém seus dados.

### Correções
- **Transferências com "Renomear" podiam perder um arquivo.** Em uma transferência com vários arquivos, o nome livre escolhido para um conflito (por exemplo `a (1).txt`) podia ser ocupado ao mesmo tempo por outro arquivo da mesma transferência, e um dos dois era sobrescrito sem aviso. Agora o app nunca sobrescreve no modo "Renomear": se o nome for ocupado, ele usa o próximo livre. Afetava todas as plataformas.

### Limitações conhecidas
- Windows não foi validado.
- No Linux, só Ubuntu 24.04 com Wayland foi testado. X11, escala fracionária, IME e teclas mortas, arrastar e soltar e o cofre do sistema bloqueado não foram validados.
- O `.deb` foi gerado mas não instalado nos testes; o nome do pacote é `my-ssh` e o atalho do menu fica na categoria Desenvolvimento.
- Os pacotes não são assinados nem notarizados.
- Dependência com alerta de segurança conhecido para chaves RSA (RUSTSEC-2023-0071): prefira chaves ed25519.

### Instalação (macOS)
`make install-mac` (compila a partir do código). Esta versão não traz `.dmg` anexado; o da v0.1.0 continua disponível. O app não é assinado: na primeira abertura, clique com o botão direito e escolha Abrir.

### Instalação (Linux)
`make install-linux`, ou baixe o AppImage anexado, marque-o como executável (`chmod +x`) e abra. Também há um `.deb` (`sudo apt install ./myssh_0.1.1_amd64.deb`).

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
