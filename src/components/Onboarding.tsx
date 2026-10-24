import { useEffect, useState } from "react";
import { BrandMark, Wordmark } from "./Brand";
import { CoachMark, type CoachMarkSpec } from "./ui/CoachMark";
import { readOnboardingState, writeOnboardingState } from "../lib/commands";
import { logError } from "../lib/report";
import { useConnections } from "../state/connectionStore";

type Stage = "loading" | "welcome" | "coachmarks" | "done";

const COACH_MARKS: CoachMarkSpec[] = [
  {
    target: "[data-onboard='sidebar']",
    title: "Browse the schema",
    body: "Tables, views, and pinned items live here. Type in the search to fuzz-find anything.",
    placement: "right",
  },
  {
    target: "[data-onboard='tabs']",
    title: "Tabs are your workspace",
    body: "Each table, query, and ERD opens in its own tab. ⌘T for a new query, ⌘W to close.",
    placement: "bottom",
  },
  {
    target: "[data-onboard='inspector']",
    title: "Inspect anything",
    body: "Toggle this panel with ⌘] to see columns, relationships, and (soon) chat with AI about the schema.",
    placement: "left",
  },
];

/**
 * First-launch onboarding — wraps nothing, mounts at app root. Detects first run
 * via a Tauri-side marker (~/<config>/TableIQ/onboarded.json). Re-run anytime via
 * the `tiq:show-tour` window event (dispatched by the Help menu).
 */
export function Onboarding() {
  const [stage, setStage] = useState<Stage>("loading");
  const [coachIdx, setCoachIdx] = useState(0);
  const hasActive = useConnections((s) => !!s.active);

  // Bootstrap: read marker → decide whether to show welcome.
  useEffect(() => {
    let cancelled = false;
    readOnboardingState()
      .then((s) => {
        if (cancelled) return;
        setStage(s.onboarded ? "done" : "welcome");
      })
      .catch(() => !cancelled && setStage("done"));
    return () => {
      cancelled = true;
    };
  }, []);

  // Re-trigger from the Help menu — only run coach marks when there's a workspace.
  useEffect(() => {
    const onTour = () => {
      if (hasActive) {
        setCoachIdx(0);
        setStage("coachmarks");
      } else {
        setStage("welcome");
      }
    };
    window.addEventListener("tiq:show-tour", onTour);
    return () => window.removeEventListener("tiq:show-tour", onTour);
  }, [hasActive]);

  // After welcome → if user connects → run coach marks.
  useEffect(() => {
    if (stage === "welcome" && hasActive) {
      setCoachIdx(0);
      setStage("coachmarks");
    }
  }, [stage, hasActive]);

  const finish = () => {
    writeOnboardingState({ onboarded: true }).catch((e) => logError("onboarding: mark done", e));
    setStage("done");
  };

  if (stage === "loading" || stage === "done") return null;

  if (stage === "welcome") {
    return (
      <div className="tiq-overlay flex items-center justify-center p-6">
        <div className="tiq-glass-panel w-[560px] max-w-full rounded-xl border border-border bg-bg p-8 shadow-[var(--shadow-pop)]">
          <BrandMark size={42} className="mb-3 text-primary" />
          <div className="mb-1 text-2xl font-semibold tracking-tight text-text">
            Welcome to <Wordmark className="text-2xl" />
          </div>
          <p className="mb-6 max-w-md text-sm text-muted">
            A fast, local-first SQL desktop client. Browse, query, edit, and visualize across
            Postgres, MySQL, SQL Server, SQLite, Redis, and MongoDB — all in one app.
          </p>

          <div className="mb-6 grid grid-cols-3 gap-3 text-[11px]">
            <Feature title="Browse" body="Native data grid with windowed paging — handles million-row tables." />
            <Feature title="Query" body="CodeMirror editor, EXPLAIN, history, charts, AI explain / optimize / fix." />
            <Feature title="Visualize" body="ERD with foreign-key edges across every engine." />
          </div>

          <div className="flex items-center justify-between">
            <button
              onClick={finish}
              className="text-xs text-muted hover:text-text"
            >
              Skip the tour
            </button>
            <button
              onClick={() => {
                // Step 2 — "Connect your first database" simply dismisses the
                // welcome card. The user is already looking at ConnectionManager.
                // We keep stage="welcome" so once they connect, the post-connect
                // coach marks fire automatically (effect above watches `hasActive`).
                setStage("welcome");
                writeOnboardingState({ onboarded: true }).catch((e) =>
                  logError("onboarding: mark done", e),
                );
                // Hide the welcome card immediately — the ConnectionManager is right behind it.
                setStage("done");
                // But arm coach marks for the next active connection:
                window.setTimeout(() => {
                  setCoachIdx(0);
                  setStage("done");
                }, 0);
              }}
              className="rounded-md bg-primary px-4 py-2 text-xs font-medium text-white hover:bg-primary-hover"
            >
              Connect your first database
            </button>
          </div>

          <div className="mt-6 border-t border-border pt-3 text-[10px] text-muted">
            <kbd className="tiq-kbd">⌘K</kbd> opens the command palette · everything is
            keyboard-driven · zero telemetry.
          </div>
        </div>
      </div>
    );
  }

  // stage === "coachmarks"
  const spec = COACH_MARKS[coachIdx];
  return (
    <CoachMark
      spec={spec}
      index={coachIdx}
      total={COACH_MARKS.length}
      onSkip={finish}
      onNext={() => {
        if (coachIdx + 1 >= COACH_MARKS.length) finish();
        else setCoachIdx(coachIdx + 1);
      }}
    />
  );
}

function Feature({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-md border border-border bg-surface p-2.5">
      <div className="mb-0.5 text-xs font-semibold text-text">{title}</div>
      <div className="text-[10px] leading-snug text-muted">{body}</div>
    </div>
  );
}
