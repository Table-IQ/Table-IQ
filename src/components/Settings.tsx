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
import { loadToolbar, orderedIds, saveToolbar, type ToolbarPref } from "../lib/toolbarPrefs";
import { DEFAULT_HIDDEN_TOOLBAR_IDS, DEFAULT_TOOLBAR_PREF, TOOLBAR_ITEMS } from "../lib/toolbarItems";

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
 * Close Settings on the next macrotask instead of synchronously. Used by
 * App.tsx's window `keydown` handler for Escape: ConnectionManager and
 * MainShell each guard their own Escape handling on
 * `useSettingsUi.getState().open` too (an in-flight connection shouldn't
 * cancel, and the new-connection sheet shouldn't close, just because Settings
 * also closed on the same keypress). `window` keydown listeners fire in
 * registration order, which isn't guaranteed across components — closing
 * synchronously here could flip `open` to false *before* one of those other
 * handlers, still running on the same call stack, has even checked it.
 *
 * `queueMicrotask` is NOT safe for this: for a native, user-triggered event
 * the browser runs a microtask checkpoint after *each* listener callback
 * returns (the JS stack is empty between listeners), so a microtask-deferred
 * close would already have run by the time the next window listener for this
 * same keypress executes — reproducing the exact race this is meant to fix.
 * `setTimeout(…, 0)` schedules a macrotask, which only runs once every
 * listener for this event has finished, so `open` stays true for the whole
 * dispatch regardless of registration order.
 */
export function closeSettingsDeferred(): void {
  setTimeout(() => useSettingsUi.getState().close(), 0);
}

const selectField =
  "rounded-md border border-border bg-bg px-2 py-1 text-sm text-text focus:border-primary focus:outline-none";

/** Row of toggle buttons for a small closed set of choices (theme, rail
 *  label) — the active one filled, same visual language as the section nav. */
function ChoiceGroup<T extends string>({
  value,
  options,
  onChange,
  "aria-label": ariaLabel,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  "aria-label": string;
}) {
  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className="inline-flex overflow-hidden rounded-md border border-border"
    >
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

/** `group`: the children are their own control (a ChoiceGroup — a set of
 *  buttons with its own `role="group"`/`aria-label`), so wrapping in a
 *  `<label>` would make clicking the label text activate the first button.
 *  Omit `group` for a real single control (e.g. a `<select>`), where the
 *  `<label>` association is exactly what's wanted. */
function Field({
  label,
  children,
  group,
}: {
  label: string;
  children: ReactNode;
  group?: boolean;
}) {
  const Tag = group ? "div" : "label";
  return (
    <Tag className="flex items-center justify-between gap-4 text-sm text-text">
      <span>{label}</span>
      {children}
    </Tag>
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
      <Field label="Theme" group>
        <ChoiceGroup<ThemeChoice>
          aria-label="Theme"
          value={choice}
          onChange={setChoice}
          options={[
            { value: "light", label: "Light" },
            { value: "auto", label: "Auto" },
            { value: "dark", label: "Dark" },
          ]}
        />
      </Field>
      <Field label="Connection rail shows" group>
        <ChoiceGroup<RailLabel>
          aria-label="Connection rail shows"
          value={railLabel}
          onChange={(v) => {
            saveRailLabel(v);
            setRailLabel(v);
            // A mounted ConnectionRail (own local copy of this pref) listens
            // for this to re-read `loadRailLabel()` live.
            window.dispatchEvent(new CustomEvent("tiq:prefs"));
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

/**
 * Checkbox list (hide/show) + ↑/↓ reorder for the title-bar's right icon
 * group (Task 7). No drag library — matches the brief. `TOOLBAR_ITEMS` is the
 * id+label source shared with `MainShell`, which owns the icon/onClick
 * wiring; this section only reads/writes the pref and lets `MainShell` know
 * via the `tiq:toolbar` event (same custom-event precedent as `tiq:sidebar`).
 */
function ToolbarSection() {
  const [pref, setPref] = useState<ToolbarPref>(() => loadToolbar() ?? DEFAULT_TOOLBAR_PREF);

  const persist = (next: ToolbarPref) => {
    setPref(next);
    saveToolbar(next);
    window.dispatchEvent(new Event("tiq:toolbar"));
  };

  // Full order — visible *and* hidden — so a hidden item still has a row to
  // re-enable from. Shares `orderedIds` with `applyToolbar` (toolbarPrefs.ts)
  // instead of re-implementing the same "known pref.order ids first, then
  // append anything new" rule here.
  const allIds = TOOLBAR_ITEMS.map((i) => i.id);
  const fullOrder = orderedIds(allIds, pref.order);

  // An id missing from `pref.order` is genuinely new — this pref predates
  // it — so its visibility falls back to `DEFAULT_HIDDEN_TOOLBAR_IDS` rather
  // than always showing up checked (same rule `applyToolbar` uses). Once an
  // id is known (in `pref.order`), `pref.hidden` alone governs.
  const knownIds = new Set(pref.order);
  const isHidden = (id: string) =>
    knownIds.has(id) ? pref.hidden.includes(id) : DEFAULT_HIDDEN_TOOLBAR_IDS.includes(id);

  // Any persist from here promotes every id to "known" (order becomes the
  // full list) — so `hidden` must be materialized from `isHidden` first,
  // otherwise a default-hidden-but-not-yet-known id would silently flip to
  // visible the moment it's promoted (its absence from the old `pref.hidden`
  // would no longer fall back to the default).
  const materializedHidden = fullOrder.filter(isHidden);

  const move = (id: string, dir: -1 | 1) => {
    const i = fullOrder.indexOf(id);
    const j = i + dir;
    if (j < 0 || j >= fullOrder.length) return;
    const next = [...fullOrder];
    [next[i], next[j]] = [next[j], next[i]];
    persist({ order: next, hidden: materializedHidden });
  };

  const toggleHidden = (id: string) => {
    const hidden = isHidden(id)
      ? materializedHidden.filter((h) => h !== id)
      : [...materializedHidden, id];
    persist({ order: fullOrder, hidden });
  };

  return (
    <div className="space-y-3 p-4">
      <div className="space-y-0.5">
        {fullOrder.map((id, i) => {
          const item = TOOLBAR_ITEMS.find((t) => t.id === id);
          if (!item) return null;
          return (
            <div
              key={id}
              className="flex items-center justify-between gap-2 rounded-md px-2 py-1 hover:bg-surface-2"
            >
              <label className="flex items-center gap-2 text-sm text-text">
                <input
                  type="checkbox"
                  checked={!isHidden(id)}
                  onChange={() => toggleHidden(id)}
                />
                {item.label}
              </label>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  aria-label={`Move ${item.label} up`}
                  disabled={i === 0}
                  onClick={() => move(id, -1)}
                  className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-30"
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={`Move ${item.label} down`}
                  disabled={i === fullOrder.length - 1}
                  onClick={() => move(id, 1)}
                  className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted transition-colors hover:bg-surface-2 hover:text-text disabled:opacity-30"
                >
                  ↓
                </button>
              </div>
            </div>
          );
        })}
      </div>
      <button
        type="button"
        onClick={() => persist(DEFAULT_TOOLBAR_PREF)}
        className="text-xs text-muted underline underline-offset-2 hover:text-text"
      >
        Reset to default
      </button>
    </div>
  );
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
