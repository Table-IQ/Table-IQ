"""Regression checks for the deployment gate, using disposable site copies."""
from pathlib import Path
import json
import html
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


class WebsiteChecks(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.site = Path(self.scratch.name) / "website"
        shutil.copytree(Path(__file__).resolve().parent.parent, self.site, ignore=shutil.ignore_patterns(".hallmark", ".wrangler", "node_modules", "__pycache__"))

    def run_script(self, name, *args):
        return subprocess.run(
            [sys.executable, str(self.site / "scripts" / name), *args],
            capture_output=True, text=True,
        )

    def change(self, name, before, after):
        path = self.site / "dist" / name
        content = path.read_text()
        self.assertIn(before, content)
        path.write_text(content.replace(before, after, 1))

    def test_committed_site_passes_without_writes(self):
        before = {p: p.read_bytes() for p in (self.site / "dist").rglob("*") if p.is_file()}
        for name, args in (("seo.py", ["--check"]), ("validate.py", [])):
            result = self.run_script(name, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(all(p.read_bytes() == content for p, content in before.items()))

    def test_stale_faq_fails_without_rewriting_then_regenerates(self):
        self.change("index.html", "<summary>What is TableIQ?</summary>", "<summary>What does TableIQ do?</summary>")
        home = self.site / "dist/index.html"
        before = home.read_bytes()
        result = self.run_script("seo.py", "--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Stale SEO output: index.html", result.stderr)
        self.assertEqual(home.read_bytes(), before)
        result = self.run_script("seo.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.run_script("seo.py", "--check")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_asset_fails(self):
        (self.site / "dist/img/hero-light.jpg").unlink()
        result = self.run_script("validate.py")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing local file: /img/hero-light.jpg", result.stderr)

    def test_broken_anchor_fails(self):
        self.change("index.html", 'href="#features"', 'href="#missing-feature"')
        result = self.run_script("validate.py")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing anchor: #missing-feature", result.stderr)

    def test_wrong_sitemap_fails(self):
        self.change("sitemap.xml", "/license</loc>", "/missing</loc>")
        result = self.run_script("validate.py")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sitemap URLs must match", result.stderr)

    def test_indexable_pages_have_direct_canonicals_and_markdown_alternates(self):
        for path in (self.site / "dist").rglob("*.html"):
            source = path.read_text()
            self.assertNotIn("noindex", source)
            canonical = re.search(r'<link rel="canonical" href="([^"]+)"', source)[1]
            self.assertFalse(canonical.endswith(".html"), canonical)
            self.assertIn(f'<meta property="og:url" content="{canonical}">', source)
            self.assertIn('rel="alternate" type="text/markdown"', source)
            self.assertIn('rel="describedby"', source)

    def test_generated_markdown_preserves_sql_and_faq_answers(self):
        result = self.run_script("seo.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        guide = self.site / "dist/guides/postgres-investigation.html"
        markdown = guide.with_suffix(".md").read_text()
        for code in re.findall(r'<pre><code[^>]*>(.*?)</code></pre>', guide.read_text(), re.S):
            self.assertIn(html.unescape(code), markdown)
        self.change("index.html", "<summary>What is TableIQ?</summary>", "<summary>What does TableIQ do?</summary>")
        result = self.run_script("seo.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("What does TableIQ do?", (self.site / "dist/index.md").read_text())
        self.assertIn("What does TableIQ do?", (self.site / "dist/llms-full.txt").read_text())

    def test_check_rejects_missing_markdown_without_creating_it(self):
        path = self.site / "dist/index.md"
        path.unlink(missing_ok=True)
        result = self.run_script("seo.py", "--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("index.md", result.stderr)
        self.assertFalse(path.exists())

    def test_navigation_and_other_disclosures_never_enter_faq_schema(self):
        self.change("index.html", "<main id=\"main\">", '<details><summary>Unrelated menu</summary><div>Not a FAQ</div></details><main id="main">')
        result = self.run_script("seo.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        content = (self.site / "dist/index.html").read_text()
        graph = json.loads(re.search(r'<script type="application/ld\+json">(.*?)</script>', content, re.S)[1])["@graph"]
        faq = next(item for item in graph if item["@type"] == "FAQPage")["mainEntity"]
        self.assertEqual(len(faq), 7)
        self.assertEqual(faq[0]["name"], "What is TableIQ?")
        self.assertTrue(all(len(item["name"]) < 100 for item in faq))
        self.assertNotIn("Unrelated menu", json.dumps(faq))


if __name__ == "__main__":
    unittest.main()
