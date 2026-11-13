import Link from "next/link";
import { BrandLogo } from "@/components/blocks/brand-logo";
import { GITHUB } from "@/lib/site";

export function Footer() {
  return (
    <footer className="border-t border-rule bg-paper">
      <div className="mx-auto grid max-w-site gap-10 px-gutter py-14 md:grid-cols-2">
        <div>
          <Link href="/" aria-label="TableIQ home" className="inline-block"><BrandLogo /></Link>
          <p className="mt-4 text-ink-2">Browse your databases.<br />See what your agent runs.</p>
        </div>
        <div className="flex flex-col gap-6 md:items-end">
          <nav aria-label="Footer" className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-ink-2">
            <a href={GITHUB} className="hover:text-ink">GitHub</a>
            <Link href="/#features" className="hover:text-ink">Features</Link>
            <Link href="/#engines" className="hover:text-ink">Engines</Link>
            <Link href="/#guides" className="hover:text-ink">Guides</Link>
            <Link href="/license" className="hover:text-ink">License</Link>
          </nav>
          <p className="text-sm text-muted">An <a href="https://ideolyst.com" className="underline decoration-rule underline-offset-4 hover:text-ink">Ideolyst</a> product. © 2026. Open source under <Link href="/license" className="underline decoration-rule underline-offset-4 hover:text-ink">AGPL-3.0-only</Link>.</p>
        </div>
      </div>
    </footer>
  );
}
