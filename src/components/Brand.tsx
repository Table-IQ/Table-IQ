import { useId } from "react";

/**
 * Inline TableIQ mark: a 2×2 table whose top-right cell *is* the **AI sparkle**
 * (4-point star). The three cells use currentColor so they adapt to theme; the spark
 * carries the indigo→violet AI gradient (per DESIGN.md's "violet/gradient reserved
 * for AI"). Same geometry as src/assets/brand/glyph.svg and the app tile.
 */
export function BrandMark({ size = 24, className = "" }: { size?: number; className?: string }) {
  // Per-instance gradient id so multiple marks on one page never share a DOM id.
  const gid = `tiq-spark-${useId()}`;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={gid} x1="51" y1="25.6" x2="74.4" y2="49" gradientUnits="userSpaceOnUse">
          <stop stopColor="#6366F1" />
          <stop offset="1" stopColor="#A855F7" />
        </linearGradient>
      </defs>
      <rect x="26.6" y="26.6" width="21.5" height="21.5" rx="4.3" fill="currentColor" />
      <rect x="26.6" y="52" width="21.5" height="21.5" rx="4.3" fill="currentColor" />
      <rect x="52" y="52" width="21.5" height="21.5" rx="4.3" fill="currentColor" />
      {/* The fourth cell: a 4-point AI sparkle, slightly larger so it breaks out of the grid. */}
      <path
        d="M62.7 25.6 L65.6 34.4 L74.4 37.3 L65.6 40.2 L62.7 49 L59.8 40.2 L51 37.3 L59.8 34.4 Z"
        fill={`url(#${gid})`}
      />
    </svg>
  );
}

/** TableIQ wordmark: "Table" in text color, "IQ" in the accent. */
export function Wordmark({ className = "" }: { className?: string }) {
  return (
    <span className={`inline-flex items-center gap-2 font-semibold tracking-tight ${className}`}>
      <BrandMark size={22} className="text-primary" />
      <span>
        Table<span className="text-primary">IQ</span>
      </span>
    </span>
  );
}
