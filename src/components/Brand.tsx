import markLight from "../assets/brand/tableiq-mark-ink.svg";
import markDark from "../assets/brand/tableiq-mark-white.svg";
import wordmarkLight from "../assets/brand/tableiq-logo-horizontal-light.svg";
import wordmarkDark from "../assets/brand/tableiq-logo-horizontal-dark.svg";

/** Supplied Witness Q artwork; the app theme selects the original light or dark variant. */
export function BrandMark({ size = 24, className = "" }: { size?: number; className?: string }) {
  return (
    <span className={`tableiq-brand ${className}`} style={{ width: size, height: size }} aria-hidden="true">
      <img className="brand-on-light" src={markLight} alt="" />
      <img className="brand-on-dark" src={markDark} alt="" />
    </span>
  );
}

/** Original outlined TableIQ lockup, preserving the supplied letterforms and colors. */
export function Wordmark({ className = "" }: { className?: string }) {
  return (
    <span className={`tableiq-brand tableiq-wordmark ${className}`}>
      <img className="brand-on-light" src={wordmarkLight} alt="TableIQ" />
      <img className="brand-on-dark" src={wordmarkDark} alt="TableIQ" />
    </span>
  );
}
