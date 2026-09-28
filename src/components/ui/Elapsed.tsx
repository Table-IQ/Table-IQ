import { useEffect, useState, type HTMLAttributes } from "react";
import { formatElapsed } from "../../lib/motion";

/** Live "1.2s" since `since` (a `performance.now()` stamp). Owns its own 100ms
 *  tick, so only this span re-renders — never the view around it. Renders
 *  nothing until `minMs` has passed (pass the busy delay so a fast op shows none). */
export function Elapsed({
  since,
  minMs = 0,
  ...rest
}: { since: number; minMs?: number } & HTMLAttributes<HTMLSpanElement>) {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    setNow(performance.now());
    const id = window.setInterval(() => setNow(performance.now()), 100);
    return () => window.clearInterval(id);
  }, [since]);
  const ms = Math.max(0, now - since);
  if (ms < minMs) return null;
  return <span {...rest}>{formatElapsed(ms)}</span>;
}
