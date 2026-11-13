import Link from "next/link";
import type { ReactNode } from "react";

const styles = {
  primary: "bg-ink text-paper hover:bg-accent hover:text-accent-ink",
  secondary: "border border-rule text-ink hover:border-accent",
  text: "px-0 text-ink underline decoration-rule underline-offset-4 hover:decoration-accent",
  inverse: "bg-band-ink text-band hover:opacity-90",
};

export function Button({ href, children, variant = "primary" }: { href: string; children: ReactNode; variant?: keyof typeof styles }) {
  const base = "inline-flex min-h-11 items-center gap-2 rounded-pill font-medium transition-colors max-sm:min-h-12";
  const pad = variant === "text" ? "" : "px-5";
  const external = href.startsWith("http");
  return external ? (
    <a href={href} className={`${base} ${pad} ${styles[variant]}`}>{children}</a>
  ) : (
    <Link href={href} className={`${base} ${pad} ${styles[variant]}`}>{children}</Link>
  );
}
