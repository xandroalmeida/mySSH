#!/usr/bin/env python3
"""Gera a página do projeto (GitHub Pages, pasta docs/) em inglês, pt-BR e espanhol.

Uso:
  python3 tools/site/build.py vX.Y.Z   # lê a release vX.Y.Z do GitHub (gh) e regenera
  python3 tools/site/build.py          # regenera com a release salva em release.json

Entradas: template.html (layout único), strings.json (textos por idioma) e release.json
(versão, data e binários da release, escrito pelo modo com argumento).
Saídas: docs/index.html (en), docs/pt-br/index.html, docs/es/index.html, docs/sitemap.xml
e docs/llms.txt (resumo em Markdown para buscadores de IA).
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
AUTHOR = "Alexandro Almeida"
AUTHOR_URL = "https://github.com/xandroalmeida"
SCREENSHOTS = ["dez-abas.png", "paineis-divididos.png", "arquivos-conflito.png", "sync-previa.png"]
FAQ_KEYS = [(f"q{i}", f"a{i}") for i in range(1, 7)]


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


def faq_html(t: dict) -> str:
    e = html.escape
    return "\n".join(
        f"      <details><summary>{e(t[q])}</summary><p>{e(t[a])}</p></details>"
        for q, a in FAQ_KEYS
    )


def jsonld(lang: str, strings: dict, rel: dict) -> str:
    """Dados estruturados schema.org (SoftwareApplication, WebPage e FAQPage) da página."""
    t = strings[lang]
    url = SITE_URL + t["path"]
    author = {"@type": "Person", "name": AUTHOR, "url": AUTHOR_URL}
    oses = {"mac": "macOS", "linux": "Linux", "windows": "Windows"}
    downloads = [a["url"] for a in rel["assets"] if classify(a["name"])]
    app = {
        "@type": "SoftwareApplication",
        "@id": SITE_URL + "#app",
        "name": "mySSH",
        "alternateName": "mySSH SSH client",
        "description": t["a1"],
        "url": url,
        "applicationCategory": "DeveloperApplication",
        "applicationSubCategory": "SSH client",
        "operatingSystem": ", ".join(oses.values()),
        "softwareVersion": rel["version"],
        "datePublished": rel["date"],
        "releaseNotes": f"{REPO}/releases/tag/{rel['tag']}",
        "downloadUrl": downloads or f"{REPO}/releases",
        "installUrl": url + "#download",
        "screenshot": [SITE_URL + "img/" + s for s in SCREENSHOTS],
        "image": SITE_URL + "assets/icon-256.png",
        "featureList": [t[f"f{i}_t"] + ": " + t[f"f{i}_d"] for i in range(1, 9)],
        "inLanguage": [s["html_lang"] for s in strings.values()],
        "isAccessibleForFree": True,
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
        "license": ["https://opensource.org/license/mit", "https://www.apache.org/licenses/LICENSE-2.0"],
        "author": author,
        "sameAs": [REPO],
    }
    page = {
        "@type": "WebPage",
        "@id": url + "#page",
        "url": url,
        "name": t["title"],
        "description": t["description"],
        "inLanguage": t["html_lang"],
        "dateModified": rel["date"],
        "about": {"@id": SITE_URL + "#app"},
        "primaryImageOfPage": SITE_URL + "img/paineis-divididos.png",
    }
    faq = {
        "@type": "FAQPage",
        "@id": url + "#faq",
        "inLanguage": t["html_lang"],
        "mainEntity": [
            {"@type": "Question", "name": t[q], "acceptedAnswer": {"@type": "Answer", "text": t[a]}}
            for q, a in FAQ_KEYS
        ],
    }
    code = {
        "@type": "SoftwareSourceCode",
        "name": "mySSH",
        "codeRepository": REPO,
        "programmingLanguage": "Rust",
        "license": app["license"],
        "targetProduct": {"@id": SITE_URL + "#app"},
    }
    data = {"@context": "https://schema.org", "@graph": [app, page, faq, code]}
    # "</" não pode aparecer dentro de <script>.
    return json.dumps(data, ensure_ascii=False, indent=2).replace("</", "<\\/")


def sitemap_xml(strings: dict, rel: dict) -> str:
    alts = "".join(
        f'\n    <xhtml:link rel="alternate" hreflang="{s["html_lang"]}" href="{SITE_URL}{s["path"]}"/>'
        for s in strings.values()
    ) + f'\n    <xhtml:link rel="alternate" hreflang="x-default" href="{SITE_URL}"/>'
    urls = "".join(
        f"\n  <url>\n    <loc>{SITE_URL}{s['path']}</loc>\n    <lastmod>{rel['date']}</lastmod>{alts}\n  </url>"
        for s in strings.values()
    )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"'
        ' xmlns:xhtml="http://www.w3.org/1999/xhtml">'
        f"{urls}\n</urlset>\n"
    )


def llms_txt(strings: dict, rel: dict) -> str:
    """Resumo em Markdown (convenção llms.txt) para buscadores e assistentes de IA."""
    t = strings["en"]
    lines = [
        "# mySSH",
        "",
        f"> {t['a1']}",
        "",
        f"Latest version: v{rel['version']} ({rel['date']}"
        + (", pre-release" if rel["prerelease"] else "") + "). "
        "License: MIT or Apache-2.0. "
        f"Author: {AUTHOR}. Languages: English, Portuguese (Brazil), Spanish.",
        "",
        "## Features",
        "",
    ]
    lines += [f"- **{t[f'f{i}_t']}**: {t[f'f{i}_d']}" for i in range(1, 9)]
    lines += ["", "## FAQ", ""]
    for q, a in FAQ_KEYS:
        lines += [f"### {t[q]}", "", t[a], ""]
    lines += ["## Download", ""]
    for a in rel["assets"]:
        if classify(a["name"]):
            lines.append(f"- [{a['name']}]({a['url']})")
    lines += [
        f"- [All releases]({REPO}/releases)",
        "",
        "## Links",
        "",
        f"- [Website (English)]({SITE_URL}), [Português]({SITE_URL}pt-br/), [Español]({SITE_URL}es/)",
        f"- [Source code on GitHub]({REPO})",
        f"- [Changelog]({REPO}/blob/main/CHANGELOG.md)",
        f"- [User guide (Portuguese)]({REPO}/blob/main/docs/USO.md)",
        f"- [Issues]({REPO}/issues)",
        "",
    ]
    return "\n".join(lines)


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
    og_alternates = "\n".join(
        f'<meta property="og:locale:alternate" content="{s["og_locale"]}">'
        for code, s in strings.items() if code != lang
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
        "faq": faq_html(t),
        "jsonld": jsonld(lang, strings, rel),
        "og_locale_alternates": og_alternates,
        "author": e(AUTHOR),
        "author_url": AUTHOR_URL,
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
    for name, content in (("sitemap.xml", sitemap_xml(strings, rel)), ("llms.txt", llms_txt(strings, rel))):
        (DOCS / name).write_text(content)
        print(f"gerado docs/{name}")


if __name__ == "__main__":
    main()
