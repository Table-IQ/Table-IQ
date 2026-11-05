#!/usr/bin/env python3
"""Check published HTML, JSON-LD, sitemap and local asset/link references."""
import json
from html.parser import HTMLParser
from pathlib import Path
import re
from urllib.parse import unquote, urljoin, urlsplit
import xml.etree.ElementTree as ET

SITE = "https://tableiq.ideolyst.com"
DIST = Path(__file__).resolve().parent.parent / "dist"


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.ids = set()
        self.references = []
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if attrs.get("id"):
            if attrs["id"] in self.ids:
                raise ValueError(f"duplicate id: {attrs['id']}")
            self.ids.add(attrs["id"])
        for attr in ("href", "src", "poster"):
            if attrs.get(attr):
                self.references.append(attrs[attr])
        if attrs.get("srcset"):
            self.references.extend(item.strip().split()[0] for item in attrs["srcset"].split(","))


def main():
    pages = {}
    for path in sorted(DIST.rglob("*.html")):
        if any(part.startswith(".") for part in path.relative_to(DIST).parts):
            continue
        source = path.read_text()
        pages[path] = Page(source)
        blocks = re.findall(r'<script type="application/ld\+json">(.*?)</script>', source, re.S)
        if not blocks:
            raise ValueError(f"{path.relative_to(DIST)}: missing JSON-LD")
        for block in blocks:
            graph = json.loads(block)
            if graph.get("@context") != "https://schema.org" or not graph.get("@graph"):
                raise ValueError(f"{path.relative_to(DIST)}: invalid JSON-LD graph")

    def check_reference(owner, reference):
        url = urlsplit(urljoin(f"{SITE}/{owner.relative_to(DIST)}", reference))
        if url.scheme not in ("http", "https") or url.netloc != urlsplit(SITE).netloc:
            return
        target = (DIST / unquote(url.path).lstrip("/")).resolve()
        if not target.is_relative_to(DIST.resolve()):
            raise ValueError(f"{owner.relative_to(DIST)}: path outside site: {reference}")
        if target.is_dir():
            target /= "index.html"
        if not target.is_file():
            raise ValueError(f"{owner.relative_to(DIST)}: missing local file: {reference}")
        if url.fragment and target in pages and unquote(url.fragment) not in pages[target].ids:
            raise ValueError(f"{owner.relative_to(DIST)}: missing anchor: {reference}")

    for path, page in pages.items():
        for reference in page.references:
            check_reference(path, reference)
    for path in DIST.rglob("*.css"):
        for reference in re.findall(r'url\([\s\'"]*([^\)\'"\s]+)[\s\'"]*\)', path.read_text()):
            check_reference(path, reference)

    sitemap = DIST / "sitemap.xml"
    urls = [node.text for node in ET.parse(sitemap).findall("{*}url/{*}loc")]
    expected = {f"{SITE}/" if path.name == "index.html" else f"{SITE}/{path.relative_to(DIST)}" for path in pages}
    if len(urls) != len(set(urls)) or set(urls) != expected:
        raise ValueError("sitemap URLs must match published HTML pages exactly")
    for url in urls:
        check_reference(sitemap, url)
    print(f"Validated {len(pages)} HTML pages, JSON-LD, sitemap, and local links/assets")


if __name__ == "__main__":
    main()
