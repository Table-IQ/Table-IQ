/** Inline TableIQ mark (cells-T + spark). Uses currentColor so it adapts to theme. */
export function BrandMark({ size = 24, className = "" }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <rect x="20" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="42" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="64" y="26" width="16" height="13" rx="2.5" fill="currentColor" />
      <rect x="42" y="39" width="16" height="38" rx="3" fill="currentColor" />
      <circle cx="78" cy="22" r="6" fill="var(--accent)" />
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
