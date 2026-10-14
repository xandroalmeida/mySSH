# Gerenciador de sessões SSH — análise e prompt para Claude Code

## Decisões e requisitos que faltavam

- **Interface por plataforma:** comportamento adequado a macOS, Windows e Linux não significa que todos os controles serão nativos. Definir atalhos, menus, diálogos, DPI, acessibilidade e integração com o sistema. Priorizar Rust também na UI; não adotar frontend JavaScript implicitamente.
- **Terminal real:** uma janela de texto não basta. É necessário emulador de terminal, PTY remoto, sequências ANSI/VT, Unicode, redimensionamento e aplicações como vim, top e tmux.
- **Segurança:** guardar senhas é apenas parte do problema. Verificar a identidade dos servidores, integrar SSH agent, proteger chaves e passphrases, evitar segredos em logs e definir bloqueio do cofre.
- **Sincronização:** transferência recursiva não equivale a sincronização bidirecional. Definir conflitos, exclusões, arquivos parciais, alterações durante a operação e diferenças entre sistemas de arquivos.
- **Operação:** incluir bastion/jump host, perda de rede, timeouts, cancelamento, backup, migrações, instaladores e validação nos três sistemas.

Premissas: aplicação desktop local, sem conta em nuvem; “conta” significa perfil de conexão SSH; tema específico por perfil. Sincronização inicial manual e unidirecional nos dois sentidos disponíveis; bidirecional automática fica para uma fase posterior.

## Prompt completo

Você é o Claude Opus, responsável pela arquitetura, coordenação, revisão e integração de um gerenciador desktop de sessões SSH. Os agentes Claude Sonnet são os programadores e devem implementar o código, testes e correções.

Quero um produto funcional. Avance por entregas executáveis e verificáveis, sem ciclos intermináveis de planejamento, refatoração ou documentação.

### 1. Organização dos agentes

- Opus: analise requisitos, tome decisões, defina contratos entre módulos, distribua tarefas, revise patches, coordene a integração e valide entregas. Não concentre a programação no agente principal.
- Sonnet: implemente UI, núcleo SSH, credenciais, persistência, SFTP, sincronização e testes. Distribua por tarefas pequenas; não mantenha agentes ociosos por obrigação.
- Use os identificadores reais de modelos disponíveis no ambiente. Verifique primeiro se consegue criar subagentes Sonnet. Não afirme que está usando Sonnet sem comprovação. Se o ambiente não permitir selecionar o modelo, explique a limitação antes de apresentar o fluxo como Opus + Sonnet.
- Cada tarefa precisa de objetivo, dependências, arquivos sob responsabilidade, critérios de aceite e forma de validação. Quando houver paralelo, use worktrees ou divisão de arquivos para evitar colisões.
- Antes de paralelizar módulos dependentes, defina suas interfaces. Não deixe vários agentes alterarem simultaneamente os mesmos contratos centrais.
- Toda entrega dos programadores deve informar alterações, verificações executadas, resultado e limitações. Opus revisa e integra antes de marcar a tarefa como concluída.
- Revise com atenção especial credenciais, validação de host, conteúdo remoto e operações de arquivos. Essa revisão pode ser feita por outro Sonnet, além da revisão final do Opus.

### 2. Produto e limites

Construa uma aplicação desktop em Rust para macOS, Windows e Linux, capaz de gerenciar conexões e sessões SSH simultâneas, proteger credenciais, personalizar temas por conexão e transferir/sincronizar arquivos entre máquina local e remota.

Premissas iniciais:

- Funciona localmente, sem login na aplicação, backend próprio, assinatura ou serviço em nuvem.
- “Conta” significa perfil de conexão: host + porta + usuário + opções + referências a credenciais. Não significa usuário de um SaaS.
- Um perfil pode abrir diversas sessões independentes. Credenciais podem ser reutilizadas por vários perfis sem duplicar segredos.
- Mensagens e interface em português do Brasil. Organize os textos para permitir tradução futura.
- Fora da primeira versão: colaboração, sincronização de credenciais em nuvem, execução simultânea de comandos em múltiplos servidores, macros automáticas, X11 forwarding e sincronização bidirecional automática.

### 3. Decisão técnica com prova funcional

Use Rust no núcleo e priorize Rust na UI. Escolha bibliotecas existentes para SSH, SFTP, emulação de terminal, armazenamento seguro e criptografia; não implemente esses protocolos do zero.

Compare no máximo duas alternativas viáveis de UI/terminal. Verifique documentação atual, licença, manutenção, acessibilidade, integração com o sistema, desempenho e empacotamento. Tauri só deve ser considerado deixando explícito o uso de WebView e a eventual necessidade de frontend em outra linguagem; não trate isso como equivalente a uma UI inteiramente em Rust.

Antes de construir a arquitetura inteira, delegue uma prova funcional a Sonnet: janela gráfica, conexão SSH real, validação de host, PTY remoto, terminal interativo, vim ou top funcionando e redimensionamento correto. A escolha deve se apoiar nessa execução, incluindo as limitações por plataforma.

Não reescreva um emulador de terminal para preservar uma escolha de framework. Se houver incompatibilidade relevante com Rust na UI, apresente a evidência e uma alternativa concreta para decisão do usuário.

### 4. Perfis e sessões

- CRUD de perfis com nome, hostname/IP, porta, usuário, autenticação, pasta/grupo, tags, favoritos, ambiente e tema.
- Busca e filtros; conexão rápida sem obrigação de salvar credenciais.
- Abas e painéis divididos, identificação clara de host/usuário/ambiente e aviso visual configurável para produção.
- Estados explícitos: desconectado, conectando, aguardando autenticação, conectado, reconectando e erro.
- Timeouts, keepalive e tratamento de perda de rede, suspensão e retomada do computador.
- Reconectar cria uma nova sessão; não prometer recuperação do processo remoto ou reenviar comandos automaticamente. Documentar o uso de tmux para continuidade remota.
- Solicitar confirmação ao fechar sessões ou transferências ativas; oferecer cancelamento e encerramento previsível.
- Suportar bastion/jump host com validação independente da identidade de cada servidor.
- Importar um subconjunto documentado de ~/.ssh/config, com preview, suporte a caminhos por sistema e aviso sobre diretivas ignoradas. Não executar ProxyCommand ou comandos locais apenas porque aparecem em um arquivo importado.

### 5. Terminal

- Emulador maduro com ANSI/VT, cores, UTF-8, caracteres largos/combinantes, tela alternativa, scrollback e bracketed paste.
- PTY remoto, envio correto de dimensões e TERM coerente com as capacidades realmente implementadas.
- Compatibilidade verificada com bash/zsh, vim, top e tmux.
- Copiar/colar, seleção, busca no histórico visível, fonte/tamanho e atalhos adequados ao sistema.
- Atalhos da aplicação não devem bloquear combinações essenciais do terminal.
- Backpressure, buffers limitados e processamento assíncrono: grande volume de saída não pode congelar a UI nem consumir memória indefinidamente.
- Aviso configurável para colagem de múltiplas linhas. A saída remota não pode executar comandos locais, abrir URLs ou alterar clipboard automaticamente; OSC 52 deve exigir autorização ou permanecer desativado.
- Não gravar sessões em disco por padrão. Se houver gravação futura, exigir opt-in e informar que o conteúdo pode conter segredos; não prometer mascaramento perfeito da saída remota.

### 6. Credenciais e identidade dos servidores

- Autenticação por senha, chave privada com/sem passphrase, SSH agent e keyboard-interactive, incluindo prompts de MFA sem persistir códigos temporários.
- Integração com agente local não implica agent forwarding. Deixe o encaminhamento desabilitado por padrão e exija ativação por perfil.
- Permitir referenciar chaves existentes sem copiá-las. Se o usuário importar uma chave para armazenamento gerenciado, proteger seu conteúdo com criptografia autenticada e chave de proteção no armazenamento seguro do sistema.
- Usar Keychain no macOS, armazenamento seguro apropriado no Windows e Secret Service ou equivalente validado no Linux. Conferir backend, persistência, limites e permissões reais da biblioteca escolhida.
- Senhas/passphrases persistidas devem ficar no armazenamento seguro ou no cofre criptografado; metadados guardam somente referências. Não usar JSON, SQLite ou variáveis de ambiente com segredos em texto puro.
- Se o armazenamento seguro estiver ausente ou bloqueado, oferecer uso temporário em memória ou pedir desbloqueio. Nunca aplicar fallback silencioso para texto puro. Um cofre portátil com senha mestra é evolução posterior, salvo necessidade comprovada.
- Bloqueio manual e automático por inatividade/bloqueio do sistema: impedir novas recuperações de credenciais e descartar cópias em memória quando possível. Deixar explícito que bloquear o cofre não encerra sessões SSH já autenticadas; oferecer opção separada para encerrá-las.
- Não expor segredos em logs de diagnóstico, erros, argumentos de processos, histórico de formulários, telemetria ou relatórios de agentes. Minimizar o tempo em memória e usar descarte seguro quando suportado, sem prometer proteção contra uma máquina já comprometida.
- Validar host keys antes de autenticar. Na primeira conexão, mostrar fingerprint SHA-256 e permitir aceitar ou rejeitar; orientar confirmação por canal confiável. Persistir a decisão por host/porta.
- Mudança de host key deve bloquear a conexão e exigir revisão explícita. Nunca usar “aceitar qualquer chave”, desabilitar validação ou aceitar alterações automaticamente.
- Tratar known_hosts, hashes, host aliases e importação com limites documentados; não corromper arquivos SSH do usuário. Preferir armazenamento próprio e importação controlada inicialmente.
- Usar configurações criptográficas modernas das bibliotecas e negar algoritmos obsoletos por padrão. Qualquer exceção deve ser visível e por perfil.

### 7. Transferência e sincronização

Use SFTP sobre SSH. Não dependa de rsync instalado no destino nem faça transferências por comandos shell improvisados.

Transferências:

- Navegador de arquivos local/remoto; upload/download de arquivos e diretórios recursivos; drag-and-drop onde suportado.
- Fila com concorrência limitada, progresso, bytes, taxa, cancelamento e erros por arquivo. Transferência não deve comprometer a interatividade do terminal.
- Política explícita para sobrescrever, pular ou renomear arquivos existentes.
- Gravar em arquivo temporário e publicar por rename ao terminar, quando suportado. Não afirmar atomicidade ou substituição segura onde o servidor/sistema não oferecer essas garantias.
- Retomada apenas quando houver suporte e validação de que origem/destino permanecem compatíveis; não retomar cegamente pelo tamanho.
- Tratar permissões, timestamps, Unicode, espaços, arquivos grandes e nomes incompatíveis com o sistema local. Não requerer sudo para operações comuns.

Sincronização da primeira versão:

- Manual e unidirecional: local → remoto ou remoto → local, escolhida pelo usuário.
- Comparar árvores e produzir plano/dry-run com inclusões, alterações, conflitos e exclusões antes da execução.
- Não excluir arquivos por padrão. Modo espelho com exclusões deve ser ativado explicitamente e confirmado após preview.
- Suportar exclusões por padrões, incluindo .git e diretórios definidos pelo usuário.
- Não seguir symlinks por padrão. Evitar ciclos, path traversal e operações fora das raízes selecionadas; revalidar caminhos e condições durante a execução.
- Definir comparação por tamanho/mtime e suas limitações. Oferecer comparação por conteúdo quando necessária, considerando que checksum remoto pode exigir leitura via SFTP e custo de rede; não assumir extensão disponível.
- Considerar resolução de timestamps, case sensitivity e diferenças de nomes entre sistemas. Não escolher vencedores silenciosamente em casos ambíguos.
- Detectar alteração relevante após o preview, revalidar antes de operações destrutivas e pedir novo plano quando necessário. Preview não elimina alterações concorrentes no destino.
- Persistir resultado/journal sem segredos; permitir cancelamento e retomada segura. Não prometer transação atômica da pasta inteira.
- Bidirecional fica para fase posterior e exige baseline persistente, detecção de alterações dos dois lados, conflitos explícitos e política de exclusão. Copiar nos dois sentidos não basta.

### 8. Interface e temas

- UI consistente como produto e adaptada às convenções de cada sistema: Cmd/Ctrl, menus, botões, diálogos, foco, notificações, DPI e múltiplos monitores.
- Declarar quais controles são nativos e quais são desenhados pelo framework. No Linux, documentar ambientes gráficos e backends efetivamente testados.
- Layout com sidebar de perfis, área de terminal e painel de arquivos/transferências acessível. Boa densidade de informação, sem comprometer legibilidade.
- Navegação por teclado, foco visível, contraste e acessibilidade verificada conforme suporte do toolkit.
- Tema global claro/escuro/sistema; tema e paleta de terminal por perfil, herdando o global quando não houver override. Tema por perfil se aplica à aba/painel correspondente e não altera toda a aplicação.
- Permitir identificar ambiente por cor sem depender apenas dela. Texto/ícone deve complementar o sinal visual.

### 9. Arquitetura e persistência

- Separar minimamente UI, casos de uso, núcleo SSH/SFTP, credenciais, armazenamento e sincronização. Evitar camadas cerimoniais, microserviços e sistemas de plugins antes de existir necessidade.
- Compartilhar políticas de host trust e autenticação entre terminal, jump host e SFTP. Não criar um caminho de transferência que ignore validação de servidor.
- Usar runtime assíncrono, cancelamento, limites de recursos e tarefas supervisionadas. Nenhuma operação de rede ou disco pesada deve bloquear a thread gráfica.
- SQLite pode armazenar metadados com migrações, integridade referencial e escritas consistentes. Segredos ficam separados; hostname e usuário também merecem cuidado nos diagnósticos/exportações.
- Backup/exportação inicial de configurações sem segredos, com schema versionado e importação validada. Não exportar referências locais a credenciais como se fossem portáveis. Backup criptografado de segredos é uma funcionalidade posterior com requisitos próprios de recuperação.
- Sem telemetria por padrão. Documentar dependências, licenças, diretórios de dados e processo de atualização.

### 10. Marcos e aceite

M0 — prova funcional e decisão de stack: janela, SSH real, verificação de host, terminal interativo e resize. Documentar evidência e lacunas de plataforma.

M1 — uso básico: perfis persistidos, senha/chave, abas, sessões simultâneas, desconexão e erros claros. Já deve ser possível abrir o app e trabalhar em um servidor real.

M2 — segurança e experiência: armazenamento seguro, SSH agent/MFA, bloqueio, bastion, importação limitada de SSH config, temas por perfil e painéis divididos.

M3 — SFTP: navegação local/remota, upload/download recursivo, fila, progresso, cancelamento e política de sobrescrita.

M4 — sincronização: preview, modos unidirecionais, exclusões, conflitos, tratamento de mudanças concorrentes e modo espelho protegido.

M5 — distribuição: build e instaladores para as três plataformas, documentação de uso, validação manual por plataforma e revisão final de segurança. Assinatura/notarização exigem credenciais próprias; preparar o fluxo e declarar quando os artefatos ainda não estiverem assinados.

Um marco só está concluído quando seu fluxo funcionar integrado. Mocks podem ajudar testes, mas não comprovam SSH/SFTP real. Telas vazias, dados fictícios e TODOs essenciais não contam como funcionalidades prontas.

Testes obrigatórios proporcionais ao risco:

- Testes unitários para trust de host, resolução de perfis, plano de sincronização, conflitos e proteção de caminhos.
- Integração com servidor SSH/SFTP de teste para senha, chave, host desconhecido/alterado, PTY, cancelamento e falha de rede.
- Cenários de arquivos com espaços/Unicode, symlinks, arquivos grandes, permissões negadas, disco cheio, nomes colidentes e alteração durante transferência/sincronização.
- Garantir que exclusão não ocorre com o padrão de sincronização e que diagnósticos gerados pela aplicação não vazam credenciais de teste.
- Demonstrar pelo menos dez sessões simultâneas sem bloqueio perceptível da interface, em máquina de referência documentada; registrar consumo e gargalos observados.
- fmt, clippy, testes, auditoria de dependências e builds em CI para macOS, Linux e Windows. Build em CI não substitui validação da UI, keyring e instalador em cada sistema; declarar o que não foi executado.

### 11. Como começar e conduzir

Inspecione primeiro o repositório, ferramentas, instruções locais e disponibilidade real de modelos. Se houver código, aproveite o que funciona e preserve mudanças do usuário.

Crie um AGENTS.md curto com papéis Opus/Sonnet, comandos e convenções; um plano por marcos com critérios de aceite; e registre apenas decisões arquiteturais relevantes. Não gaste a primeira entrega produzindo uma coleção extensa de documentos.

Apresente brevemente premissas, riscos concretos e duas alternativas no máximo, então delegue a prova funcional do M0 e avance para o M1. Só interrompa para ambiguidades que mudem substancialmente o produto, exigência de credenciais externas ou bloqueio comprovado; resolva detalhes reversíveis com julgamento técnico.

Ao fim de cada marco, informe: o que funciona, como executar, o que foi testado, limitações verificadas e próximo passo. Nunca afirme que algo foi testado em um sistema que não estava disponível. Prossiga para o próximo marco dentro do escopo autorizado, mantendo pontos de verificação utilizáveis e sem revisitar a arquitetura repetidamente sem nova evidência.

## Referências técnicas consultadas

- Tauri — limites entre núcleo Rust e frontend WebView: https://v2.tauri.app/security/
- OpenSSH — configuração, host trust, jump hosts e autenticação: https://man.openbsd.org/ssh_config
- Keyring — armazenamento seguro por plataforma: https://docs.rs/keyring
- Russh SFTP — biblioteca de SFTP: https://docs.rs/russh-sftp

As bibliotecas são candidatas, não decisões já validadas. O prompt exige verificar versões, licenças e compatibilidade no ambiente de implementação.
