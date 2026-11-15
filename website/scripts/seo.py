#!/usr/bin/env python3
"""Derive Markdown, llms.txt, llms-full.txt, _headers and indexnow.json from the built site in out/.

usage: python3 scripts/seo.py [--check] [--dir out]

Runs automatically after `npm run build`. Next.js owns metadata, JSON-LD, sitemap and robots;
this script only exports the public HTML as text so answer engines read exactly what visitors see.
"""
import argparse, html, json, pathlib, re
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true", help="fail on stale output without changing files")
parser.add_argument("--dir", default="out", help="built site directory")
args = parser.parse_args()
stale = []

SITE = "https://tableiq.ideolyst.com"
dist = pathlib.Path(__file__).resolve().parent.parent / args.dir

def save(path, content):
    if path.exists() and path.read_text() == content:
        return
    if args.check:
        stale.append(str(path.relative_to(dist)))
    else:
        path.write_text(content)

def text(fragment):
    return html.unescape(re.sub(r"<[^>]+>", "", fragment)).strip()


def page_url(path):
    relative = path.relative_to(dist)
    return SITE + "/" + ("" if relative == pathlib.Path("index.html") else relative.with_suffix("").as_posix())

def metadata(source):
    title = text(re.search(r"<title>(.*?)</title>", source, re.S)[1])
    description = html.unescape(re.search(r'<meta name="description" content="([^"]*)"', source)[1])
    return title, description

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
# Canonical pages come from Next's sitemap; this script only derives the text exports from them.
pages = [(node.text, None) for node in ET.parse(dist / "sitemap.xml").findall("{*}url/{*}loc")]


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
headers = ["# Static assets served by Cloudflare Workers.", "/img/*", "  Cache-Control: public, max-age=604800", "", "/fonts/*", "  Cache-Control: public, max-age=604800", "", "/video/*", "  Accept-Ranges: bytes", ""]
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

availability = text(re.search(r'id="download".*?<p[^>]*>(.*?)</p>', home.read_text(), re.S)[1])
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
