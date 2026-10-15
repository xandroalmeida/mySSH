#!/usr/bin/env python3
"""Gera a página do projeto (GitHub Pages, pasta docs/) em inglês, pt-BR e espanhol.

Uso:
  python3 tools/site/build.py vX.Y.Z   # lê a release vX.Y.Z do GitHub (gh) e regenera
  python3 tools/site/build.py          # regenera com a release salva em release.json

Entradas: template.html (layout único), strings.json (textos por idioma) e release.json
(versão, data e binários da release, escrito pelo modo com argumento).
Saídas: docs/index.html (en), docs/pt-br/index.html e docs/es/index.html.
"""

import html
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DOCS = ROOT / "docs"
REPO = "https://github.com/xandroalmeida/mySSH"
SITE_URL = "https://xandroalmeida.github.io/mySSH/"
RELEASE_JSON = HERE / "release.json"


def fetch_release(tag: str) -> dict:
    out = subprocess.run(
        ["gh", "release", "view", tag, "--repo", "xandroalmeida/mySSH",
         "--json", "tagName,publishedAt,isPrerelease,assets"],
        check=True, capture_output=True, text=True,
    ).stdout
    data = json.loads(out)
    return {
        "tag": data["tagName"],
        "version": data["tagName"].lstrip("v"),
        "date": data["publishedAt"][:10],
        "prerelease": data["isPrerelease"],
        "assets": [
            {"name": a["name"], "url": a["url"], "size": a["size"]}
            for a in data["assets"]
        ],
    }


def classify(name: str):
    """(sistema, chave do tipo, chave da arquitetura) de um binário; None se não for pacote."""
    low = name.lower()
    arch = next((a for a in ("aarch64", "arm64", "x86_64", "amd64") if a in low), None)
    if low.endswith(".dmg"):
        return "mac", "kind_dmg", arch
    if low.endswith(".deb"):
        return "linux", "kind_deb", arch
    if low.endswith(".appimage"):
        return "linux", "kind_appimage", arch
    if low.endswith(".msi"):
        return "windows", "kind_msi", arch
    if low.endswith(".exe"):
        return "windows", "kind_exe", arch
    return None


def human_size(n: int) -> str:
    return f"{n / (1024 * 1024):.1f} MB"


def downloads_html(rel: dict, t: dict) -> str:
    by_os = {"mac": [], "linux": [], "windows": []}
    for a in rel["assets"]:
        c = classify(a["name"])
        if c:
            by_os[c[0]].append((c, a))
    e = html.escape
    cards = []
    for os_key in ("mac", "linux", "windows"):
        items = by_os[os_key]
        lines = [f'      <article class="dl" id="dl-{os_key}">', f"        <h3>{e(t['os_' + os_key])}</h3>"]
        if items:
            lines.append("        <ul>")
            for (_, kind, arch), a in items:
                if arch and os_key != "mac":  # "Apple Silicon"/"Intel" só fazem sentido no macOS
                    arch = {"x86_64": "amd64", "aarch64": "arm64"}.get(arch, arch)
                label = t[kind] + (f" · {t['arch_' + arch]}" if arch else "")
                lines.append(
                    f'          <li><a href="{e(a["url"])}" download>{e(label)}'
                    f' <small>{human_size(a["size"])}</small></a></li>'
                )
            lines.append("        </ul>")
        else:
            lines.append(f'        <p class="none">{e(t["no_binary"])}</p>')
        lines.append(f'        <p class="hint">{e(t["note_" + os_key])}</p>')
        lines.append("      </article>")
        cards.append("\n".join(lines))
    return "\n".join(cards)


def render(template: str, lang: str, strings: dict, rel: dict) -> str:
    t = strings[lang]
    depth = t["path"].count("/")
    root = "../" * depth
    e = html.escape
    lang_links = "".join(
        f'<a href="{root}{s["path"]}" hreflang="{s["html_lang"]}" data-lang="{code}"'
        + (' aria-current="page"' if code == lang else "")
        + f">{e(s['name'])}</a>"
        for code, s in strings.items()
    )
    hreflang = "\n".join(
        [f'<link rel="alternate" hreflang="{s["html_lang"]}" href="{SITE_URL}{s["path"]}">'
         for s in strings.values()]
        + [f'<link rel="alternate" hreflang="x-default" href="{SITE_URL}">']
    )
    values = {k: e(v) for k, v in t.items()}
    values.update({
        "lang": lang,
        "root": root,
        "repo": REPO,
        "site_url": SITE_URL,
        "version": e(rel["version"]),
        "tag": e(rel["tag"]),
        "date": e(rel["date"]),
        "release_url": f"{REPO}/releases/tag/{e(rel['tag'])}",
        "prerelease_badge": f' <span class="badge">{e(t["prerelease"])}</span>' if rel["prerelease"] else "",
        "lang_links": lang_links,
        "hreflang": hreflang,
        "downloads": downloads_html(rel, t),
    })

    def sub(m):
        key = m.group(1)
        if key not in values:
            raise KeyError(f"placeholder sem valor: {key} ({lang})")
        return values[key]

    return re.sub(r"\{\{(\w+)\}\}", sub, template)


def main() -> None:
    if len(sys.argv) > 2:
        sys.exit(__doc__)
    if len(sys.argv) == 2:
        rel = fetch_release(sys.argv[1])
        RELEASE_JSON.write_text(json.dumps(rel, indent=2, ensure_ascii=False) + "\n")
    else:
        rel = json.loads(RELEASE_JSON.read_text())
    strings = json.loads((HERE / "strings.json").read_text())
    keys = set(strings["en"])
    for code, s in strings.items():
        if set(s) != keys:
            sys.exit(f"strings.json: chaves diferentes em {code}: {sorted(set(s) ^ keys)}")
    template = (HERE / "template.html").read_text()
    for code, s in strings.items():
        out = DOCS / s["path"] / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(render(template, code, strings, rel))
        print(f"gerado {out.relative_to(ROOT)} (v{rel['version']})")


if __name__ == "__main__":
    main()
