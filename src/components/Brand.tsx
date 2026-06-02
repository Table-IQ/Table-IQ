import { useId } from "react";

/**
 * Inline TableIQ mark: a "T" built from three table cells + a stem, topped by an
 * **AI sparkle** (4-point star) at the upper-right. The T uses currentColor so it
 * adapts to theme; the spark carries the indigo→violet AI gradient (per DESIGN.md's
 * "violet/gradient reserved for AI"). Slice 30: the old flat dot became this spark.
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
        <linearGradient id={gid} x1="68" y1="6" x2="92" y2="30" gradientUnits="userSpaceOnUse">
          <stop stopColor="#6366F1" />
          <stop offset="1" stopColor="#A855F7" />
        </linearGradient>
      </defs>
      <rect x="20" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="42" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="64" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="42" y="39" width="16" height="38" rx="3" fill="currentColor" />
      {/* 4-point AI sparkle (sharp concave star) where the old dot sat. */}
      <path
        d="M80 6 L82.5 15.5 L92 18 L82.5 20.5 L80 30 L77.5 20.5 L68 18 L77.5 15.5 Z"
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
