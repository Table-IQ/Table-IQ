#!/usr/bin/env python3
"""Regenerate JSON-LD and sitemap.xml from the HTML in dist/.

usage: python3 scripts/seo.py [--check]

The FAQPage schema is read from the visible <details> FAQ on the home page, so
the structured data can never disagree with what visitors see. Run after editing
copy, then commit dist/.
"""
import argparse, html, json, pathlib, re
from html.parser import HTMLParser

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true", help="fail on stale output without changing files")
args = parser.parse_args()
stale = []

SITE = "https://tableiq.ideolyst.com"
dist = pathlib.Path(__file__).resolve().parent.parent / "dist"

def save(path, content):
    if path.read_text() == content:
        return
    if args.check:
        stale.append(str(path.relative_to(dist)))
    else:
        path.write_text(content)

def text(fragment):
    return html.unescape(re.sub(r"<[^>]+>", "", fragment)).strip()

org = {"@type": "Organization", "@id": "https://ideolyst.com/#org", "name": "Ideolyst", "url": "https://ideolyst.com"}

def inject(path, graph):
    s = path.read_text()
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

license_url = SITE + "/license.html"
inject(dist / "license.html", [org, {
    "@type": "WebPage", "name": "TableIQ licensing", "url": license_url,
    "description": "AGPL-3.0-only for TableIQ-owned application code, including unsigned previews; third-party licenses preserved.",
    "about": {"@id": SITE + "/#app"},
}])
pages = [("", "1.0"), ("license.html", "0.5")]
for guide in sorted((dist / "guides").glob("*.html")):
    s = guide.read_text()
    url = f"{SITE}/guides/{guide.name}"
    inject(guide, [org, {
        "@type": "TechArticle", "headline": text(re.search(r"<h1[^>]*>(.*?)</h1>", s, re.S)[1]), "url": url,
        "description": re.search(r'<meta name="description" content="([^"]*)"', s)[1],
        "about": {"@id": SITE + "/#app"}, "author": {"@id": org["@id"]}, "publisher": {"@id": org["@id"]},
    }, {"@type": "BreadcrumbList", "itemListElement": [
        {"@type": "ListItem", "position": 1, "name": "TableIQ", "item": SITE + "/"},
        {"@type": "ListItem", "position": 2, "name": "Guides", "item": SITE + "/#guides"},
        {"@type": "ListItem", "position": 3, "name": text(re.search(r"<h1[^>]*>(.*?)</h1>", s, re.S)[1]), "item": url}]}])
    pages.append((f"guides/{guide.name}", "0.7"))

# Omit optional modification dates: a regeneration is not a content update.
save(dist / "sitemap.xml", '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    + "".join(f"  <url><loc>{SITE}/{p}</loc><priority>{pr}</priority></url>\n" for p, pr in pages) + "</urlset>\n")
if stale:
    parser.exit(1, "Stale SEO output: " + ", ".join(stale) + ". Run python3 website/scripts/seo.py\n")
print(f"{len(faq)} FAQ entries, {len(pages)} sitemap URLs")
