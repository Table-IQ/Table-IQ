#!/usr/bin/env python3
"""Regenerate JSON-LD and sitemap.xml from the HTML in dist/.

usage: python3 scripts/seo.py [--check]

The FAQPage schema is read from the visible <details> FAQ on the home page, so
the structured data can never disagree with what visitors see. Run after editing
copy, then commit dist/.
"""
import argparse, html, json, pathlib, re
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true", help="fail on stale output without changing files")
args = parser.parse_args()
stale = []

SITE = "https://tableiq.ideolyst.com"
dist = pathlib.Path(__file__).resolve().parent.parent / "dist"

def save(path, content):
    if path.exists() and path.read_text() == content:
        return
    if args.check:
        stale.append(str(path.relative_to(dist)))
    else:
        path.write_text(content)

def text(fragment):
    return html.unescape(re.sub(r"<[^>]+>", "", fragment)).strip()

org = {"@type": "Organization", "@id": "https://ideolyst.com/#org", "name": "Ideolyst", "url": "https://ideolyst.com"}

def page_url(path):
    relative = path.relative_to(dist)
    return SITE + "/" + ("" if relative == pathlib.Path("index.html") else relative.with_suffix("").as_posix())

def metadata(source):
    title = text(re.search(r"<title>(.*?)</title>", source, re.S)[1])
    description = html.unescape(re.search(r'<meta name="description" content="([^"]*)"', source)[1])
    return title, description

def prepare_page(path, source):
    """Keep search/social metadata and alternate formats on one canonical URL."""
    title, description = metadata(source)
    url = page_url(path)
    tags = [
        (r'<link rel="canonical"[^>]*>', f'<link rel="canonical" href="{url}">'),
        (r'<link rel="alternate" type="text/markdown"[^>]*>', f'<link rel="alternate" type="text/markdown" href="/{path.relative_to(dist).with_suffix(".md").as_posix()}">'),
        (r'<link rel="describedby"[^>]*>', '<link rel="describedby" href="/llms.txt" type="text/plain">'),
        (r'<link rel="preload"[^>]*>', '<link rel="preload" href="/fonts/instrument-sans-latin-500-normal.woff2" as="font" type="font/woff2" crossorigin>'),
    ]
    for name, value in {"robots": "index, follow, max-image-preview:large", "twitter:card": "summary_large_image", "twitter:title": title, "twitter:description": description, "twitter:image": SITE + "/img/og.png", "twitter:image:alt": "TableIQ: Let your agent in. See every query."}.items():
        tags.append((rf'<meta name="{re.escape(name)}"[^>]*>', f'<meta name="{name}" content="{html.escape(value, quote=True)}">'))
    for name, value in {"og:site_name": "TableIQ", "og:title": title, "og:description": description, "og:url": url, "og:image:width": "1200", "og:image:height": "630", "og:image:alt": "TableIQ: Let your agent in. See every query."}.items():
        tags.append((rf'<meta property="{re.escape(name)}"[^>]*>', f'<meta property="{name}" content="{html.escape(value, quote=True)}">'))
    for pattern, tag in tags:
        source, count = re.subn(pattern, lambda match: tag, source, count=1)
        if not count:
            source = source.replace("</head>", tag + "\n</head>", 1)
    # Existing .html bookmarks continue to work through Workers' native redirects.
    for target in [dist / "index.html", dist / "license.html", *sorted((dist / "guides").glob("*.html"))]:
        legacy = "/" + target.relative_to(dist).as_posix()
        canonical = page_url(target).removeprefix(SITE)
        source = source.replace(f'href="{legacy}', f'href="{canonical}')
        source = source.replace(f'href="{SITE}{legacy}', f'href="{SITE}{canonical}')
    return source

def inject(path, graph):
    s = prepare_page(path, path.read_text())
    block = '<script type="application/ld+json">' + json.dumps({"@context": "https://schema.org", "@graph": graph}, ensure_ascii=False) + "</script>"
    s, n = re.subn(r"(<!-- seo:jsonld -->).*?(<!-- /seo:jsonld -->)", lambda m: m[1] + block + m[2], s, flags=re.S)
    assert n == 1, f"seo:jsonld marker missing in {path}"
    save(path, s)

class FAQParser(HTMLParser):
    """Read only the FAQ container, never navigation or other disclosures."""
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self):
        super().__init__()
        self.depth = 0
        self.scopes = 0
        self.question = None
        self.answer = None
        self.field = None
        self.entries = []

    def handle_starttag(self, tag, attrs):
        if not self.depth:
            if "faq" not in dict(attrs).get("class", "").split():
                return
            self.depth = 1
            self.scopes += 1
        elif tag not in self.VOID:
            self.depth += 1
        if tag == "details":
            self.question, self.answer = [], []
        elif self.question is not None and tag in ("summary", "p"):
            self.field = self.question if tag == "summary" else self.answer

    def handle_data(self, data):
        if self.field is not None:
            self.field.append(data)

    def handle_endtag(self, tag):
        if not self.depth:
            return
        if tag in ("summary", "p"):
            self.field = None
        elif tag == "details" and self.question is not None:
            question = " ".join("".join(self.question).split())
            answer = " ".join("".join(self.answer).split())
            assert question and answer, "FAQ question or answer is empty"
            self.entries.append({"@type": "Question", "name": question, "acceptedAnswer": {"@type": "Answer", "text": answer}})
            self.question = self.answer = self.field = None
        if tag not in self.VOID:
            self.depth -= 1


home = dist / "index.html"
faq_parser = FAQParser()
faq_parser.feed(home.read_text())
assert faq_parser.scopes == 1 and faq_parser.entries, "Expected one nonempty FAQ container"
faq = faq_parser.entries
app = {
    "@type": "SoftwareApplication", "@id": SITE + "/#app", "name": "TableIQ", "url": SITE + "/",
    "applicationCategory": "DeveloperApplication", "operatingSystem": "macOS",
    "description": "Free, open-source Mac database client for browsing, querying and editing databases, with a local MCP server that lets coding agents query chosen connections read-only by default and logs every call.",
    "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
    "license": "https://www.gnu.org/licenses/agpl-3.0.html", "image": SITE + "/img/hero-light.jpg",
    "publisher": {"@id": org["@id"]},
}
inject(home, [org, app, {"@type": "WebSite", "@id": SITE + "/#site", "url": SITE + "/", "name": "TableIQ", "publisher": {"@id": org["@id"]}},
              {"@type": "FAQPage", "mainEntity": faq}])

license_url = page_url(dist / "license.html")
inject(dist / "license.html", [org, {
    "@type": "WebPage", "name": "TableIQ licensing", "url": license_url,
    "description": "AGPL-3.0-only for TableIQ-owned application code, including unsigned previews; third-party licenses preserved.",
    "about": {"@id": SITE + "/#app"},
}])
pages = [(page_url(home), "1.0"), (license_url, "0.5")]
for guide in sorted((dist / "guides").glob("*.html")):
    s = guide.read_text()
    url = page_url(guide)
    inject(guide, [org, {
        "@type": "TechArticle", "headline": text(re.search(r"<h1[^>]*>(.*?)</h1>", s, re.S)[1]), "url": url,
        "description": re.search(r'<meta name="description" content="([^"]*)"', s)[1],
        "about": {"@id": SITE + "/#app"}, "author": {"@id": org["@id"]}, "publisher": {"@id": org["@id"]},
    }, {"@type": "BreadcrumbList", "itemListElement": [
        {"@type": "ListItem", "position": 1, "name": "TableIQ", "item": SITE + "/"},
        {"@type": "ListItem", "position": 2, "name": "Guides", "item": SITE + "/#guides"},
        {"@type": "ListItem", "position": 3, "name": text(re.search(r"<h1[^>]*>(.*?)</h1>", s, re.S)[1]), "item": url}]}])
    pages.append((url, "0.7"))

# Omit optional modification dates: a regeneration is not a content update.
save(dist / "sitemap.xml", '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    + "".join(f"  <url><loc>{url}</loc><priority>{priority}</priority></url>\n" for url, priority in pages) + "</urlset>\n")


class Markdown(HTMLParser):
    """Export these static pages' main content, preserving their SQL verbatim."""
    def __init__(self, url):
        super().__init__()
        self.url, self.active, self.skip = url, False, 0
        self.output, self.links, self.lists = [], [], []
        self.pre, self.language, self.heading = None, "", False
        self.columns, self.header_row = 0, False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "main":
            self.active = True
            return
        if not self.active:
            return
        if self.skip:
            if tag not in FAQParser.VOID:
                self.skip += 1
            return
        if tag in {"nav", "script", "style", "button", "svg"}:
            self.skip = 1
            return
        if self.pre is not None:
            if tag == "code":
                self.language = attrs.get("class", "").removeprefix("language-")
            return
        if tag == "pre":
            self.pre, self.language = [], ""
        elif tag in {"h1", "h2", "h3", "h4"}:
            self.output.append("\n\n" + "#" * int(tag[1]) + " ")
            self.heading = True
        elif tag == "summary":
            self.output.append("\n\n### ")
        elif tag in {"p", "figcaption"}:
            self.output.append("\n\n")
        elif tag == "br":
            self.output.append(" " if self.heading else "\n")
        elif tag in {"ol", "ul"}:
            self.lists.append([tag, int(attrs.get("start", "1")) - 1])
            self.output.append("\n")
        elif tag == "li":
            if self.lists and self.lists[-1][0] == "ol":
                self.lists[-1][1] += 1
                self.output.append(f"\n{self.lists[-1][1]}. ")
            else:
                self.output.append("\n- ")
        elif tag == "a":
            self.links.append(urljoin(self.url, attrs.get("href", "")))
            self.output.append("[")
        elif tag == "code":
            self.output.append("`")
        elif tag == "tr":
            self.output.append("\n| ")
            self.columns, self.header_row = 0, False
        elif tag in {"td", "th"}:
            self.columns += 1
            self.header_row |= tag == "th" and attrs.get("scope") != "row"

    def handle_data(self, data):
        if not self.active or self.skip:
            return
        if self.pre is not None:
            self.pre.append(data)
        else:
            self.output.append(re.sub(r"\s+", " ", data))

    def handle_endtag(self, tag):
        if tag == "main":
            self.active = False
        if not self.active:
            return
        if self.skip:
            if tag not in FAQParser.VOID:
                self.skip -= 1
            return
        if self.pre is not None:
            if tag == "pre":
                code = "".join(self.pre)
                self.output.append((f"```{self.language}\n{code}" + ("" if code.endswith("\n") else "\n") + "```" ,))
                self.pre = None
            return
        if tag in {"h1", "h2", "h3", "h4"}:
            self.heading = False
            self.output.append("\n\n")
        elif tag in {"p", "figcaption", "summary"}:
            self.output.append("\n\n")
        elif tag in {"ol", "ul"}:
            self.lists.pop()
            self.output.append("\n\n")
        elif tag == "a" and self.links:
            self.output.append(f"]({self.links.pop()})")
        elif tag == "code":
            self.output.append("`")
        elif tag in {"td", "th"}:
            self.output.append(" | ")
        elif tag == "tr" and self.header_row:
            self.output.append("\n| " + " | ".join(["---"] * self.columns) + " |\n")

    def render(self):
        # Normalize layout whitespace outside code blocks only.
        chunks, prose = [], []
        for fragment in [*self.output, ("",)]:
            if isinstance(fragment, tuple):
                if prose:
                    value = re.sub(r"[ \t]+\n", "\n", "".join(prose))
                    chunks.append(re.sub(r"\n{3,}", "\n\n", value).strip())
                    prose = []
                chunks.append(fragment[0])
            else:
                prose.append(fragment)
        return "\n\n".join(chunk for chunk in chunks if chunk) + "\n"


exports = []
headers = ["# Static assets served by Cloudflare Workers.", "/img/*", "  Cache-Control: public, max-age=604800", "", "/fonts/*", "  Cache-Control: public, max-age=604800", ""]
for path in [home, *sorted((dist / "guides").glob("*.html")), dist / "license.html"]:
    url = page_url(path)
    reader = Markdown(url)
    reader.feed(path.read_text())
    markdown = f"Canonical page: {url}\n\n" + reader.render()
    target = path.with_suffix(".md")
    save(target, markdown)
    title, _ = metadata(path.read_text())
    exports.append((title, target, markdown))
    headers.extend(["/" + target.relative_to(dist).as_posix(), "  Content-Type: text/markdown; charset=utf-8", "  X-Robots-Tag: noindex, follow", f'  Link: <{url}>; rel="canonical"', ""])

availability = text(re.search(r'<div class="download">.*?<p>(.*?)</p>', home.read_text(), re.S)[1])
llms = f"# TableIQ\n\n> {faq[0]['acceptedAnswer']['text']}\n\n## Availability\n\n{availability}\n\n## Documentation\n\n"
llms += "".join(f"- [{title}]({SITE}/{path.relative_to(dist).as_posix()}): Markdown of the public page\n" for title, path, _ in exports)
llms += f"- [Complete reference]({SITE}/llms-full.txt): all public page content in one file\n"
llms += f"- [Engine support]({SITE}/#engine-list): supported engines and Beta labels\n"
llms += "\n## Permissions and privacy\n\n" + next(item["acceptedAnswer"]["text"] for item in faq if item["name"] == "Is it safe to point an agent at production?") + "\n"
save(dist / "llms.txt", llms)
save(dist / "llms-full.txt", "# TableIQ public reference\n\n" + "\n\n---\n\n".join(markdown for _, _, markdown in exports))
for name in ("llms.txt", "llms-full.txt", "indexnow-key.txt"):
    headers.extend(["/" + name, "  Content-Type: text/plain; charset=utf-8", "  X-Robots-Tag: noindex, follow", ""])
save(dist / "_headers", "\n".join(headers))
# IndexNow uses a public ownership-verification file, not a private credential.
key = (dist / "indexnow-key.txt").read_text().strip()
assert re.fullmatch(r"[a-f0-9]{32}", key), "Invalid public IndexNow verification key"
save(dist / "indexnow.json", json.dumps({"host": urlsplit(SITE).netloc, "key": key, "keyLocation": SITE + "/indexnow-key.txt", "urlList": [url for url, _ in pages]}, indent=2) + "\n")
if stale:
    parser.exit(1, "Stale SEO output: " + ", ".join(stale) + ". Run python3 website/scripts/seo.py\n")
print(f"{len(faq)} FAQ entries, {len(pages)} sitemap URLs")
