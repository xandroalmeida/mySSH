---
name: release
description: Versiona o mySSH (SemVer), gera release notes, cria a tag anotada vX.Y.Z, publica no GitHub com o pacote da plataforma atual e atualiza a página do projeto. Sem commits novos desde a última tag, só gera e anexa o pacote desta plataforma à release existente (build em várias máquinas).
argument-hint: "[patch|minor|major|X.Y.Z] (padrão: inferido dos commits; sem commits novos, anexa o pacote desta plataforma)"
---

# Release do mySSH

Arquivo canônico deste comando. `.opencode/commands/release.md` e
`.agents/skills/release/SKILL.md` são symlinks para cá: edite só este arquivo.

Argumento recebido: `$ARGUMENTS`
(Se aparecer literalmente `$ARGUMENTS`, ou vier vazio, o pedido do usuário na conversa é o argumento.)

Siga os passos em ordem. Pare e informe o usuário em qualquer falha: não pule passos, não use `--no-verify` e não force push.

## 0. Escolher o modo
Há dois modos. Cada máquina gera só o pacote da própria plataforma, então uma release completa é: rodar `/release` em uma máquina (cria a versão) e depois rodar `/release` nas outras (cada uma anexa o seu pacote).

1. Faça primeiro as verificações de repositório do passo 1 (árvore limpa, `main`, `git fetch --tags origin`, não atrás de `origin/main`; se estiver atrás, `git pull --ff-only`).
2. Última tag: `T=$(git describe --tags --abbrev=0 --match 'v*')`.
3. Arquivos alterados desde a tag, **ignorando os da página** (o passo 8 sempre faz um commit da página depois da tag) e este próprio procedimento (mudá-lo não altera o app):
   ```
   git diff --name-only "$T" HEAD -- . ':(exclude)docs/index.html' ':(exclude)docs/pt-br/index.html' ':(exclude)docs/es/index.html' ':(exclude)docs/sitemap.xml' ':(exclude)docs/llms.txt' ':(exclude)tools/site/release.json' ':(exclude).claude/commands/release.md'
   ```
4. Decida:
   - **Saída vazia e sem argumento** → **modo binários**: siga só a seção "Modo binários" no fim deste arquivo.
   - **Saída vazia, mas com argumento de versão** → não há nada novo para versionar: pergunte ao usuário se ele quer só anexar o pacote desta plataforma a `$T` ou criar mesmo uma versão nova.
   - **Saída não vazia** → **modo release completa**: siga os passos 1 a 10.
   - Sem nenhuma tag → primeira release: modo release completa.

## 1. Pré-condições
- `git status --porcelain` vazio e branch `main`. Se houver alterações, pare e pergunte.
- `git fetch --tags origin` e garanta que `main` não está atrás de `origin/main`.
- Versão atual: `make version`, que lê `[workspace.package] version` no `Cargo.toml` da raiz.
- Última tag: `git describe --tags --abbrev=0 --match 'v*'`. Se não houver tag, esta é a primeira release: as notas cobrem todo o histórico e a versão alvo é a atual.

## 2. Decidir a nova versão (SemVer)
- Argumento `patch|minor|major` → incremente a parte correspondente.
- Argumento `X.Y.Z` → use essa versão; ela deve ser maior que a última tag.
- Sem argumento → infira dos commits desde a última tag:
  - quebra de API, de formato de dados ou de comportamento → `major` (enquanto for `0.x`, use `minor`);
  - funcionalidade nova → `minor`;
  - só correções, docs ou build → `patch`.

  Mostre a versão escolhida e o motivo em uma linha antes de seguir.
- Recuse se a tag `vX.Y.Z` já existir local ou remotamente.

## 3. Validar
Rode `make check`. Se falhar, pare e mostre o erro. Se o Docker estiver disponível, rode também `make test-real` e informe o resultado. Sem Docker, diga que esse passo foi pulado.

## 4. Atualizar a versão
- Se a versão mudou, edite **somente** `[workspace.package] version` no `Cargo.toml` da raiz. Os crates herdam com `version.workspace = true`.
- Rode `cargo update --workspace` para o `Cargo.lock` refletir a versão nova, e depois `make check` de novo. Isso é rápido.

## 5. Release notes
Gere a partir de `git log <última-tag>..HEAD --no-merges --pretty='%h %s'`. Sem tag anterior, use o histórico inteiro. Leia também os `docs/M*-RESULTADO.md` relevantes. Escreva em pt-BR, para usuário final, sem jargão interno de agentes e sem hashes no corpo:

```
## mySSH vX.Y.Z — AAAA-MM-DD

### Destaques
(2–5 bullets do que mais importa para quem usa)

### Novidades
### Correções
### Segurança
### Mudanças incompatíveis   (omitir se não houver)

### Limitações conhecidas
(fatos verificados: plataformas não testadas, artefatos não assinados etc.)

### Instalação (macOS)
`make install-mac`, ou abra o .dmg anexado. O app não é assinado: na primeira abertura, clique com o botão direito e escolha Abrir.
```

Omita as seções vazias. Grave em `docs/releases/vX.Y.Z.md` e acrescente o mesmo conteúdo ao topo de `CHANGELOG.md` (crie o arquivo se não existir, com o título `# Changelog`).

## 6. Commit e tag
- `git add Cargo.toml Cargo.lock CHANGELOG.md docs/releases/vX.Y.Z.md`
- Commit: `release: vX.Y.Z`, com a linha de coautoria exigida pelo ambiente, se houver.
- Tag anotada: `git tag -a vX.Y.Z --cleanup=verbatim -F docs/releases/vX.Y.Z.md`

## 7. Publicar
- `git push origin main` e depois `git push origin vX.Y.Z`
- Gere o pacote da plataforma atual conforme "Pacotes por plataforma", abaixo.
- `gh release create vX.Y.Z --title "mySSH vX.Y.Z" --notes-file docs/releases/vX.Y.Z.md <arquivos do pacote>`. Use `--prerelease` enquanto a versão for `0.x`.
- Diga ao usuário que os pacotes das outras plataformas são anexados rodando `/release` em cada uma delas (modo binários).
- Não habilite workflows de CI ou Release no GitHub: eles foram desabilitados de propósito.

## 8. Atualizar a página do projeto
A página (https://xandroalmeida.github.io/mySSH/, em inglês, pt-BR e espanhol) é servida pelo GitHub Pages a partir de `docs/` na `main`. Ela mostra a versão, a data e os links de download dos binários da release, então só pode ser gerada **depois** do `gh release create` e dos uploads.
- `python3 tools/site/build.py vX.Y.Z`: lê a release no GitHub com `gh` e regenera `docs/index.html`, `docs/pt-br/index.html` `docs/es/index.html`, `docs/sitemap.xml` e `docs/llms.txt`, além de `tools/site/release.json`. Não edite essas páginas à mão: layout em `tools/site/template.html`, textos em `tools/site/strings.json`.
- Confira que as três páginas citam `vX.Y.Z` e que cada binário anexado aparece em Download (`grep -c 'releases/download/vX.Y.Z' docs/index.html`).
- `git add docs/index.html docs/pt-br/index.html docs/es/index.html docs/sitemap.xml docs/llms.txt tools/site/release.json`
- Commit `docs(site): página atualizada para vX.Y.Z`, com a linha de coautoria exigida pelo ambiente, se houver. Depois `git push origin main`. O Pages publica sozinho em cerca de um minuto.
- Se a release ganhar um tipo novo de binário (por exemplo, Windows), confira em `classify()` de `tools/site/build.py` se ele é reconhecido.

## 9. Limpar artefatos de build
Os artefatos de compilação (`target/`) passam de dezenas de GB. Depois que tudo foi publicado, apague-os para liberar disco.
- Só limpe se todos os passos anteriores deram certo. Em qualquer falha, mantenha `target/` para diagnóstico ou para um novo upload.
- Antes, confirme que cada arquivo do pacote está anexado à release com o mesmo tamanho do arquivo local (`gh release view vX.Y.Z --json assets --jq '.assets[] | "\(.name) \(.size)"'`), porque a limpeza apaga as cópias locais.
- `du -sh target` (anote), depois `cargo clean` (o mesmo que `make clean`; funciona também no Windows, onde pode não haver `make`).
- A próxima compilação recomeça do zero e leva alguns minutos a mais. Isso é esperado.

## 10. Relatório final (curto)
Informe a versão anterior e a nova, a tag, o link da release, o link da página atualizada, quais pacotes foram anexados e quais plataformas ainda faltam, o resultado de `make check` e de `make test-real` (ou que foi pulado), o espaço liberado pela limpeza (ou por que ela não foi feita) e as release notes em poucas linhas.

## Pacotes por plataforma
Gere só o pacote da plataforma em que está rodando (`uname -s`; no Windows, Git Bash/MSYS informa `MINGW*`/`MSYS*`). Os alvos do Makefile já recusam um binário de release que contenha ganchos de demo.

| Plataforma | Comando | Arquivos gerados |
|---|---|---|
| macOS | `make package-mac` | `target/release/mySSH_X.Y.Z_<arch>.dmg` |
| Linux | `make package-linux` (bibliotecas: `make deps-linux`) | `target/release/myssh_X.Y.Z_<deb-arch>.deb` e `target/release/myssh_X.Y.Z_<arch>.AppImage` |
| Windows | `cargo build --release -p myssh-app` e depois, em `crates/app`, `cargo packager --release --formats nsis` | o instalador `.exe` em `target/release/` |

- Confira que cada arquivo existe e que o nome traz a versão `X.Y.Z` da tag. Se não trouxer, pare: o código local não é o da tag.
- Windows nunca foi validado (ver `docs/VALIDACAO.md`). Gere o pacote, mas avise o usuário e registre isso no relatório.
- Plataforma não listada: pare e informe.

## Modo binários (nenhum commit novo desde a última tag)
Anexa o pacote desta plataforma à release que já existe, sem criar versão, commit de versão ou tag.

1. `V=${T#v}`. A release precisa existir: `gh release view "$T" --json tagName,assets`. Se não existir, pare e informe (a tag foi criada mas a release não foi publicada).
2. O código precisa ser o da tag: a verificação do passo 0 já garante que só os arquivos da página mudaram. Confira também que `[workspace.package] version` (`make version`) é igual a `$V`.
3. Rode `make check` e informe o resultado. Se falhar, pare.
4. Gere o pacote da plataforma atual conforme "Pacotes por plataforma".
5. Se um arquivo com o mesmo nome já estiver anexado à release, pare e pergunte: o usuário pode querer manter o existente. Só substitua com `gh release upload --clobber` se ele pedir.
6. `gh release upload "$T" <arquivos do pacote>`.
7. Atualize a página (passo 8, com `vX.Y.Z` = `$T`), para que o novo download apareça nos três idiomas: regenere, faça o commit `docs(site): página atualizada para $T (pacote <plataforma>)` e `git push origin main`. Esse commit só mexe nos arquivos da página, então as outras máquinas continuam no modo binários.
8. Limpe os artefatos de build conforme o passo 9.
9. Relatório: tag, plataforma, arquivos anexados com tamanho, resultado de `make check`, link da release e da página, espaço liberado pela limpeza (ou por que ela não foi feita) e as plataformas que ainda não têm pacote (pela lista de assets da release).
