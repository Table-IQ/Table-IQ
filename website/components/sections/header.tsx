import Link from "next/link";
import { BrandLogo } from "@/components/blocks/brand-logo";
import { ThemeToggle } from "@/components/motion/theme-toggle";
import { Button } from "@/components/ui/button";
import { DOWNLOAD_URL, GITHUB, NAV } from "@/lib/site";

const DESKTOP_NAV = NAV.filter((item) => ["Demo", "Features", "MCP Activity", "Compare"].includes(item.label));

export function Header() {
  return (
    <div className="sticky top-0 z-50 px-4 pt-4">
      <header className="mx-auto flex h-16 max-w-5xl items-center justify-between gap-4 rounded-card border border-rule bg-paper/95 px-4 backdrop-blur">
        <Link href="/" aria-label="TableIQ home" className="shrink-0">
          <BrandLogo />
        </Link>
        <nav aria-label="Main navigation" className="flex items-center gap-1 md:gap-2">
          {DESKTOP_NAV.map((n) => (
            <Link key={n.href} href={n.href} className="hidden min-h-11 items-center whitespace-nowrap px-3 text-sm text-ink-2 hover:text-ink xl:inline-flex">{n.label}</Link>
          ))}
          <a href={GITHUB} className="hidden min-h-11 items-center px-3 text-sm text-ink-2 hover:text-ink sm:inline-flex">GitHub</a>
          <div className="hidden md:block">
            <Button href={DOWNLOAD_URL} variant="secondary">Download</Button>
          </div>
          <details className="relative xl:hidden">
            <summary title="Menu" className="grid size-11 cursor-pointer list-none place-items-center rounded-pill border border-rule text-ink-2 transition-colors hover:border-accent hover:text-ink [&::-webkit-details-marker]:hidden">
              <svg aria-hidden="true" viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
                <path d="M4 6h16M4 12h16M4 18h16" />
              </svg>
              <span className="sr-only">Menu</span>
            </summary>
            <div className="absolute right-0 mt-2 grid w-60 gap-1 rounded-card border border-rule bg-paper p-2 shadow-xl">
              {[...NAV, { href: GITHUB, label: "GitHub" }, { href: "/#download", label: "Download developer preview" }].map((n) => (
                <a key={n.href} href={n.href} className="flex min-h-12 items-center rounded-pill px-3 text-ink-2 hover:bg-paper-2 hover:text-ink">{n.label}</a>
              ))}
            </div>
          </details>
          <ThemeToggle />
        </nav>
      </header>
    </div>
  );
}
