"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import { BrandLogo } from "@/components/blocks/brand-logo";
import { ThemeToggle } from "@/components/motion/theme-toggle";
import { DOWNLOAD_URL, GITHUB } from "@/lib/site";

const NAVIGATION = [
  { href: "/#features", label: "Product" },
  { href: "/launch-films", label: "Demos" },
  { href: "/#compare", label: "Compare" },
  { href: "/guides/postgres-investigation", label: "Docs" },
];
const MOBILE_NAVIGATION = [...NAVIGATION, { href: "/#mcp-preview", label: "MCP Activity" }];

export function Header() {
  const pathname = usePathname();
  const dialog = useRef<HTMLDialogElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => {
    const menu = dialog.current;
    if (!menuOpen || !menu) return;
    menu.showModal();
    return () => menu.close();
  }, [menuOpen]);

  function current(href: string) {
    return href === pathname ? "page" as const : undefined;
  }

  return (
    <header className="site-header sticky top-0 z-50 px-4 pt-4">
      <div className="header-bar mx-auto flex min-h-16 max-w-6xl items-center justify-between gap-2 rounded-2xl border border-rule/70 bg-paper/90 px-3 py-2 backdrop-blur-xl md:gap-6 md:px-5">
        <Link href="/" aria-label="TableIQ home" aria-current={current("/")} className="header-brand shrink-0 rounded-lg [&_img]:h-auto [&_img]:w-28 md:[&_img]:w-32"><BrandLogo /></Link>
        <nav aria-label="Main navigation" className="hidden items-center gap-1 lg:flex">
          {NAVIGATION.map((item) => (
            <Link key={item.href} href={item.href} aria-current={current(item.href)} className="header-link inline-flex min-h-11 items-center rounded-lg px-3 text-sm font-medium text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink aria-[current=page]:text-ink">{item.label}</Link>
          ))}
        </nav>
        <div className="flex shrink-0 items-center gap-1 sm:gap-2">
          <a href={GITHUB} className="hidden min-h-11 items-center rounded-lg px-3 text-sm text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink sm:inline-flex">GitHub</a>
          <a href={DOWNLOAD_URL} className="header-download inline-flex min-h-11 items-center justify-center whitespace-nowrap rounded-lg bg-ink px-3 text-sm font-semibold text-paper transition-colors hover:bg-accent hover:text-accent-ink active:scale-[.98] md:px-4">
            <span className="sm:hidden">Download</span><span className="hidden sm:inline">Download for Mac</span>
          </a>
          <ThemeToggle />
          <button type="button" data-needs-js aria-label="Open menu" aria-expanded={menuOpen} aria-controls="mobile-navigation" onClick={() => setMenuOpen(true)} className="menu-toggle grid size-11 place-items-center rounded-lg text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink lg:hidden">
            <svg aria-hidden="true" viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="M4 7h16M4 12h16M4 17h16" /></svg>
          </button>
        </div>
      </div>
      <dialog ref={dialog} id="mobile-navigation" aria-labelledby="mobile-menu-title" onClose={() => setMenuOpen(false)} className="mobile-menu fixed inset-0 m-0 h-dvh max-h-none w-screen max-w-none bg-paper p-0 text-ink">
        <div className="flex min-h-full flex-col px-6 pb-8 pt-6">
          <div className="flex items-center justify-between gap-4">
            <Link href="/" aria-label="TableIQ home" onClick={() => setMenuOpen(false)}><BrandLogo /></Link>
            <button type="button" aria-label="Close menu" onClick={() => setMenuOpen(false)} className="grid size-11 place-items-center rounded-lg text-ink-2 transition-colors hover:bg-paper-2 hover:text-ink">
              <svg aria-hidden="true" viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="m6 6 12 12M18 6 6 18" /></svg>
            </button>
          </div>
          <h2 id="mobile-menu-title" className="sr-only">Explore TableIQ</h2>
          <nav aria-label="Mobile navigation" className="my-12 grid gap-2">
            {MOBILE_NAVIGATION.map((item, index) => (
              <Link key={item.href} href={item.href} aria-current={current(item.href)} onClick={() => setMenuOpen(false)} style={{ animationDelay: `${index * 60}ms` }} className="menu-link flex min-h-14 items-center justify-between gap-4 rounded-lg px-3 text-3xl font-medium tracking-tight transition-colors hover:bg-paper-2">
                {item.label}<span aria-hidden="true" className="text-lg text-muted">↗</span>
              </Link>
            ))}
          </nav>
          <div className="mt-auto grid gap-4">
            <p className="text-sm text-muted">Free and open source. No TableIQ account required.</p>
            <a href={DOWNLOAD_URL} className="flex min-h-12 items-center justify-center rounded-lg bg-ink px-4 text-base font-semibold text-paper transition-colors hover:bg-accent hover:text-accent-ink">Download for Mac</a>
            <a href={GITHUB} className="inline-flex min-h-11 items-center justify-center text-sm text-ink-2 underline underline-offset-4">View source on GitHub</a>
          </div>
        </div>
      </dialog>
      <noscript><nav aria-label="Navigation" className="mx-auto mt-2 flex max-w-6xl flex-wrap justify-center gap-4 text-sm lg:hidden">{NAVIGATION.map((item) => <a key={item.href} href={item.href}>{item.label}</a>)}</nav></noscript>
    </header>
  );
}
