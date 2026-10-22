# Guia de uso

## Primeira abertura (app não assinado)
Os pacotes não são assinados nem notarizados.
- **macOS**: arraste `mySSH.app` para Aplicativos. Ao abrir, o Gatekeeper dirá que não pôde verificar o app. Vá em Ajustes do Sistema → Privacidade e Segurança → role até a mensagem sobre "mySSH" e clique em **Abrir mesmo assim** (ou clique com o botão direito no app → Abrir → Abrir). Se o .dmg baixado ficar em quarentena, `xattr -dr com.apple.quarantine /Applications/mySSH.app` também resolve.
- **Windows**: o SmartScreen mostra "O Windows protegeu seu computador": "Mais informações" → "Executar assim mesmo".
- **Linux**: `.deb` pelo gerenciador de pacotes; AppImage precisa de `chmod +x`.

## Perfis e conexão rápida
**Novo perfil** pede nome, host, porta, usuário, método de autenticação, ambiente (DEV, HOMOLOGAÇÃO, PRODUÇÃO — produção ganha faixa vermelha), pasta, tags, tema e jump host. **Abrir** (ou Cmd/Ctrl+T) abre uma aba; há também Editar, Duplicar e Excluir. A busca filtra por nome, host, usuário e tag.

**Conexão rápida** conecta sem criar perfil; nada é salvo.

## Host keys
Na primeira conexão com um host o app mostra host:porta, algoritmo e o fingerprint SHA-256. Compare com o fingerprint real do servidor por um canal confiável. "Rejeitar" é o padrão; "Aceitar uma vez" não grava; "Aceitar e salvar" grava em Configurações → Hosts confiáveis.

**Se a chave mudar**, a conexão é bloqueada com um painel vermelho, **sem botão para aceitar**. Pode ser reinstalação legítima do servidor ou um ataque. Confirme com o administrador; se for legítimo, abra Configurações → Hosts confiáveis, use **Esquecer** no host e conecte de novo (volta ao fluxo de host desconhecido).

## Credenciais e cofre
Senhas e passphrases ficam no cofre do sistema (Keychain, Credential Manager, Secret Service), nunca no banco nem em arquivo. "Perguntar" pede a cada conexão; "Salvar no cofre" vem desmarcado por padrão. Chaves privadas continuam onde estão (o app guarda só o caminho).

Se o cofre do sistema estiver indisponível, o app só usa um cofre em memória se você escolher explicitamente (some ao fechar o app). **Bloquear o cofre** (manual, por inatividade configurável ou ao suspender o computador) impede ler/salvar segredos, mas **não encerra sessões já conectadas**; novas conexões que dependem de segredo salvo pedem o desbloqueio.

## Jump host
No perfil, escolha outro perfil como jump host (um salto). A autenticação do salto segue o perfil dele, inclusive a validação da host key do salto.

## AWS SSM Session Manager
Conecta a instâncias EC2 sem IP público nem porta 22 aberta, e sem AWS CLI ou session-manager-plugin: o SSH passa dentro de um túnel do AWS Systems Manager, e a host key continua sendo verificada. No perfil, **Conexão → AWS SSM Session Manager**; o botão **Como configurar?** abre a ajuda completa no app (pt-BR, inglês e espanhol).
- **Host** vira o ID da instância (`i-…`); a host key fica registrada como `ID:porta`.
- **Credenciais AWS**: *Perfil AWS* usa o que já existe em `~/.aws` (chaves, `aws login`, SSO do CLI, `credential_process`; em branco = cadeia padrão); *IAM Identity Center* faz o login pelo navegador dentro do app (URL de início, região do SSO, conta e role), com tokens só no cofre do sistema.
- **Autenticação EC2 Instance Connect** (sugerida para perfis SSM): a cada conexão o app gera uma chave ed25519 só em memória e a publica por 60 s na instância pela API; nada é gravado no servidor.
- **Requisitos**: SSM Agent online na instância (perfil IAM com `AmazonSSMManagedInstanceCore`), sshd ativo, pacote `ec2-instance-connect` para a chave temporária; para quem conecta, `ssm:StartSession` (instância e documento `AWS-StartSSHSession`) e `ec2-instance-connect:SendSSHPublicKey`.
- Perfis SSM não usam jump host, mas podem ser o jump host de outros perfis. Sessões com criptografia KMS nas preferências do Session Manager ainda não são suportadas.

## MFA / keyboard-interactive
Perfis com keyboard-interactive mostram os prompts do servidor em um diálogo ("Autenticação adicional"). As respostas (MFA/OTP) são pedidas a cada conexão e nunca salvas.

## Continuidade com tmux
O app **não** preserva a sessão se a rede cair. **Reconectar cria uma nova sessão** com a tela limpa; **nada é reenviado** (nenhum comando anterior é repetido). Para continuidade, rode `tmux new -A -s trabalho` (ou `screen`) no servidor; depois de reconectar, `tmux attach` retoma o trabalho.

## Painéis e temas
**Dividir** (Cmd/Ctrl+Shift+D lado a lado, +E empilhado) cria painéis na mesma aba, cada um com seu perfil; +W fecha o painel focado, +J troca o foco. Tema por perfil: Escuro padrão, Claro padrão, Solarized Dark, Solarized Light, Dracula e Cyberpunk Neon, com cor de destaque e tamanho de fonte opcionais. Configurações define o tema claro/escuro do app, o tamanho da fonte (8–32), a confirmação de colagem multilinha, o aviso de produção, o bloqueio do cofre e a política padrão de sobrescrita.

## Importar ~/.ssh/config
"Importar ~/.ssh/config…" mostra uma prévia (conflitos com perfis existentes por host+usuário+porta aparecem marcados) e cria perfis na pasta "Importados". O arquivo nunca é alterado. Suportado: `Host` (sem curingas viram perfis; blocos com curinga viram padrões), `HostName`, `Port`, `User`, `IdentityFile`, `ProxyJump` (um salto; com vários, só o último), `ServerAliveInterval`, `ConnectTimeout`, `Include` e `ForwardAgent` (importado sempre desligado, com aviso).
**Ignorado, com aviso**: `ProxyCommand`, `LocalCommand`, `PermitLocalCommand`, `KnownHostsCommand`, `Match` (inclusive `Match exec`) e diretivas não suportadas. Nada é executado.

## Arquivos (SFTP)
O botão **Arquivos** (Cmd/Ctrl+Shift+F) abre colunas local e remota na aba conectada: navegar, ordenar, criar pasta, renomear, excluir (pasta pede confirmação), enviar/baixar por botões ou arrastar e soltar. Política "Se já existir": Perguntar, Sobrescrever, Pular ou Renomear.

Limitações: o **rename remoto não é atômico** quando o destino já existe (o antigo vira backup, o novo é renomeado e o backup removido). **Retomada** só para arquivos ≥ 4 MiB (com validação do trecho final). Symlinks e tipos especiais não são transferidos. Exclusão local é permanente (sem lixeira). Fechar a aba com transferências ativas pede confirmação e cancela a fila.

## Sincronização de pastas
**Sincronizar…** no painel de arquivos usa as pastas abertas nas duas colunas. Escolha a direção (o destino será alterado), a comparação (tamanho+data ou conteúdo, mais lenta) e as exclusões de padrões (`.git/` por padrão).
1. **Prévia**: nada é alterado; lista criar, atualizar, conflitos, pular e excluir.
2. **Conflitos** (data/tamanho): "Pular" por padrão ou "Usar origem" item a item.
3. **Executar**: só depois de revisar a prévia.

- **Nada é excluído por padrão.** Excluir exige o **modo espelho** (desligado, com aviso vermelho) e a confirmação "Confirmo excluir N itens".
- **PlanStale**: se o destino mudou depois da prévia, o app mostra "A prévia ficou velha: nada foi executado" e pede nova prévia; o plano velho nunca é executado.
- Uma execução interrompida pode ser retomada (journal por perfil); as decisões de conflito precisam ser dadas de novo.

## Atalhos
| Ação | macOS | Windows/Linux (fora do terminal) | Windows/Linux (terminal focado) |
|---|---|---|---|
| Nova aba do perfil | Cmd+T | Ctrl+T | Ctrl+Shift+T |
| Fechar aba | Cmd+W | Ctrl+W | Ctrl+Shift+W |
| Novo perfil | Cmd+N | Ctrl+N | Ctrl+Shift+N |
| Buscar perfis | Cmd+F | Ctrl+F | Ctrl+Shift+F (ver nota) |
| Dividir lado a lado / empilhado | Cmd+Shift+D / E | Ctrl+Shift+D / E | igual |
| Fechar painel / próximo painel | Cmd+Shift+W / J | Ctrl+Shift+W / J | igual |
| Painel de arquivos | Cmd+Shift+F | Ctrl+Shift+F | Ctrl+Shift+F |

Nota: o painel de arquivos tem prioridade sobre a busca em Ctrl+Shift+F. No Windows/Linux, com o terminal focado, `Ctrl+T/W/N/F` pertencem ao programa remoto e passam direto à sessão. Ctrl+V cola; Ctrl+C sem seleção envia `^C`.

## Onde ficam os dados
Ver [`DADOS.md`](DADOS.md): macOS `~/Library/Application Support/com.myssh.mySSH/`, Windows `%APPDATA%\myssh\mySSH\data\`, Linux `~/.local/share/myssh/`. O diretório é 0700 e o banco 0600 (permissões frouxas pré-existentes são corrigidas ao abrir). `MYSSH_DATA_DIR` troca o diretório (e usa cofre em memória); `MYSSH_MEMORY_VAULT=1` força cofre em memória.

## Backup e exportação
Exportar configuração grava JSON (perfis e pastas; **sem segredos**) com permissão 0600, de forma atômica (arquivo temporário + rename). Importar nunca sobrescreve: IDs repetidos ganham novos IDs. Para backup completo do estado local, copie `myssh.db` com o app fechado. Segredos ficam no cofre do sistema e não vão junto.

## Atualização
Não há atualização automática. Baixe a nova versão e **substitua o app** (macOS: substitua `mySSH.app` em Aplicativos; Windows/Linux: reinstale). Dados e segredos ficam fora do app. O banco migra sozinho para o novo esquema na primeira abertura (`PRAGMA user_version`). Não abra uma versão antiga sobre um banco já migrado por uma versão mais nova.
