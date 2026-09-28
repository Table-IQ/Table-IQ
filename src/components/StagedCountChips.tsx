import { Fragment } from "react";
import { Chip } from "./ui/Chip";
import type { StagedCounts } from "../lib/stagedChanges";

/** `N updated · N deleted · N inserted` for the table status bar; zero kinds are left out. */
export function StagedCountChips({ counts }: { counts: StagedCounts }) {
  const parts = [
    { n: counts.updated, label: "updated", tone: "primary" as const },
    { n: counts.deleted, label: "deleted", tone: "danger" as const },
    { n: counts.inserted, label: "inserted", tone: "ok" as const },
  ].filter((p) => p.n > 0);
  if (parts.length === 0) return null;
  return (
    <span className="inline-flex shrink-0 items-center gap-1 whitespace-nowrap" aria-label="Staged changes">
      {parts.map((p, i) => (
        <Fragment key={p.label}>
          {i > 0 && <span className="text-muted-2">·</span>}
          <Chip tone={p.tone}>{`${p.n} ${p.label}`}</Chip>
        </Fragment>
      ))}
    </span>
  );
}
