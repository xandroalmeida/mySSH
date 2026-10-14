---
name: release
description: Versiona o mySSH (SemVer), gera release notes, cria a tag anotada vX.Y.Z, publica no GitHub e anexa o .dmg do macOS.
argument-hint: "[patch|minor|major|X.Y.Z] (padrão: inferido dos commits)"
---

# Release do mySSH

Arquivo canônico deste comando. `.opencode/commands/release.md` e
`.agents/skills/release/SKILL.md` são symlinks para cá: edite só este arquivo.

Argumento recebido: `$ARGUMENTS`
(Se aparecer literalmente `$ARGUMENTS`, ou vier vazio, o pedido do usuário na conversa é o argumento.)

Siga os passos em ordem. Pare e informe o usuário em qualquer falha: não pule passos, não use `--no-verify` e não force push.

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
- Pacote macOS (só se estiver em macOS): `make package-mac`. O alvo já recusa um binário de release que contenha ganchos de demo.
- `gh release create vX.Y.Z --title "mySSH vX.Y.Z" --notes-file docs/releases/vX.Y.Z.md`, anexando `target/release/mySSH_X.Y.Z_<arch>.dmg` se ele foi gerado. Use `--prerelease` enquanto a versão for `0.x`.
- Não habilite workflows de CI ou Release no GitHub: eles foram desabilitados de propósito.

## 8. Relatório final (curto)
Informe a versão anterior e a nova, a tag, o link da release, se o .dmg foi anexado, o resultado de `make check` e de `make test-real` (ou que foi pulado) e as release notes em poucas linhas.
