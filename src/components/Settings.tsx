import { useEffect, useRef, useState, type ReactNode } from "react";
import { create } from "zustand";
import { AiSettingsForm } from "./AiSettings";
import { loadRailLabel, saveRailLabel, type RailLabel } from "./ConnectionRail";
import {
  clampEditorFontSize,
  loadEditorFontSize,
  MAX_EDITOR_FONT_SIZE,
  MIN_EDITOR_FONT_SIZE,
  saveEditorFontSize,
} from "../lib/prefs";
import { useTheme } from "../theme/ThemeProvider";
import type { ThemeChoice } from "../lib/theme";
import { Panel } from "./ui/Panel";

export type Section = "general" | "editor" | "ai" | "toolbar" | "backup" | "mcp";

const SECTIONS: { key: Section; label: string }[] = [
  { key: "general", label: "General" },
  { key: "editor", label: "Editor" },
  { key: "ai", label: "AI" },
  { key: "toolbar", label: "Toolbar" },
  { key: "backup", label: "Backup tools" },
  { key: "mcp", label: "MCP" },
];

interface SettingsUiState {
  open: boolean;
  section: Section;
  show: (section?: Section) => void;
  close: () => void;
}

export const useSettingsUi = create<SettingsUiState>()((set) => ({
  open: false,
  section: "general",
  show: (section) => set((s) => ({ open: true, section: section ?? s.section })),
  close: () => set({ open: false }),
}));

/**
 * Close Settings on the next microtask instead of synchronously. Used by
 * App.tsx's window `keydown` handler for Escape: ConnectionManager and
 * MainShell each guard their own Escape handling on
 * `useSettingsUi.getState().open` too (an in-flight connection shouldn't
 * cancel, and the new-connection sheet shouldn't close, just because Settings
 * also closed on the same keypress). `window` keydown listeners fire in
 * registration order, which isn't guaranteed across components — closing
 * synchronously here could flip `open` to false *before* one of those other
 * handlers, still running on the same call stack, has even checked it.
 * Deferring past this event's synchronous dispatch keeps `open` stable for
 * every listener reacting to this one keypress, regardless of order.
 */
export function closeSettingsDeferred(): void {
  queueMicrotask(() => useSettingsUi.getState().close());
}

const selectField =
  "rounded-md border border-border bg-bg px-2 py-1 text-sm text-text focus:border-primary focus:outline-none";

/** Row of toggle buttons for a small closed set of choices (theme, rail
 *  label) — the active one filled, same visual language as the section nav. */
function ChoiceGroup<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="inline-flex overflow-hidden rounded-md border border-border">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={value === o.value}
          onClick={() => onChange(o.value)}
          className={`px-2.5 py-1 text-sm transition-colors ${
            value === o.value ? "bg-primary text-white" : "text-text hover:bg-surface-2"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex items-center justify-between gap-4 text-sm text-text">
      <span>{label}</span>
      {children}
    </label>
  );
}

function GeneralSection() {
  const { choice, setChoice } = useTheme();
  // ConnectionRail keeps its own local copy of this value (mounted separately,
  // only relevant once 2+ connections are open); Settings reads/writes the
  // same `tiq:rail:label` key via the extracted load/save, own local state.
  const [railLabel, setRailLabel] = useState<RailLabel>(loadRailLabel);
  return (
    <div className="space-y-4 p-4">
      <Field label="Theme">
        <ChoiceGroup<ThemeChoice>
          value={choice}
          onChange={setChoice}
          options={[
            { value: "light", label: "Light" },
            { value: "auto", label: "Auto" },
            { value: "dark", label: "Dark" },
          ]}
        />
      </Field>
      <Field label="Connection rail shows">
        <ChoiceGroup<RailLabel>
          value={railLabel}
          onChange={(v) => {
            saveRailLabel(v);
            setRailLabel(v);
          }}
          options={[
            { value: "database", label: "Database name" },
            { value: "connection", label: "Connection name" },
          ]}
        />
      </Field>
    </div>
  );
}

const FONT_SIZES = Array.from(
  { length: MAX_EDITOR_FONT_SIZE - MIN_EDITOR_FONT_SIZE + 1 },
  (_, i) => MIN_EDITOR_FONT_SIZE + i,
);

function EditorSection() {
  const [size, setSize] = useState(loadEditorFontSize);
  return (
    <div className="space-y-4 p-4">
      <Field label="Editor font size">
        <select
          className={selectField}
          value={size}
          onChange={(e) => {
            const next = clampEditorFontSize(Number(e.target.value));
            saveEditorFontSize(next);
            setSize(next);
            // Any mounted SqlEditor listens for this to re-apply font size live.
            window.dispatchEvent(new CustomEvent("tiq:prefs"));
          }}
        >
          {FONT_SIZES.map((n) => (
            <option key={n} value={n}>
              {n}px
            </option>
          ))}
        </select>
      </Field>
    </div>
  );
}

function Placeholder({ text }: { text: string }) {
  return <div className="p-4 text-sm text-muted">{text}</div>;
}

function ToolbarSection() {
  return <Placeholder text="Toolbar settings are coming in a later release." />;
}

function BackupSection() {
  return <Placeholder text="Backup tools are coming in a later release." />;
}

function McpSection() {
  return <Placeholder text="MCP settings are coming in a later release." />;
}

function sectionBody(section: Section) {
  switch (section) {
    case "general":
      return <GeneralSection />;
    case "editor":
      return <EditorSection />;
    case "ai":
      return <AiSettingsForm />;
    case "toolbar":
      return <ToolbarSection />;
    case "backup":
      return <BackupSection />;
    case "mcp":
      return <McpSection />;
  }
}

/**
 * Unified Settings page (⌘,). Mounted once at the app root (App.tsx), which
 * also owns the ⌘, / Esc window listener — this component just renders
 * whenever `useSettingsUi().open` is true, so it's reachable from
 * ConnectionManager, MainShell and the Kv/Doc shells alike.
 */
export function Settings() {
  const { open, section, show, close } = useSettingsUi();
  const navRef = useRef<HTMLButtonElement>(null);

  // Focus moves into the panel when it opens (keyboard accessibility).
  useEffect(() => {
    if (open) navRef.current?.focus();
  }, [open]);

  if (!open) return null;

  return (
    <Panel title="Settings" onClose={close} width="lg">
      <div className="flex min-h-[420px]">
        <nav aria-label="Settings sections" className="w-40 shrink-0 border-r border-border p-2">
          {SECTIONS.map((s, i) => {
            const active = s.key === section;
            return (
              <button
                key={s.key}
                ref={i === 0 ? navRef : undefined}
                onClick={() => show(s.key)}
                aria-current={active ? "true" : undefined}
                className={`mb-0.5 block w-full rounded-md px-2.5 py-1.5 text-left text-sm transition-colors ${
                  active ? "bg-surface-2 text-text" : "text-muted hover:bg-surface-2 hover:text-text"
                }`}
              >
                {s.label}
              </button>
            );
          })}
        </nav>
        <div className="min-w-0 flex-1 overflow-auto">{sectionBody(section)}</div>
      </div>
    </Panel>
  );
}
