import { useCallback, useEffect, useRef, useState } from "react";
import {
  listConnections,
  mcpActivity,
  mcpStatus,
  type ConnConfig,
  type McpActivity,
  type McpStatus,
} from "../lib/commands";
import { connName, mergeActivity, MCP_ACTIVITY_CAP } from "../lib/mcpActivity";
import { formatLogTs, shouldApply } from "../lib/sqlLog";
import { oneLine } from "../lib/history";
import { logError } from "../lib/report";
import { useSettingsUi } from "./Settings";
import { Toolbar, ToolbarButton, ToolbarSpacer } from "./ui/Toolbar";
import { StatusDot } from "./ui/StatusBar";

const POLL_MS = 2000;

const openMcpSettings = () => useSettingsUi.getState().show("mcp");

/**
 * MCP Activity (Task 15, tab kind "mcp"): every tool call the in-app MCP
 * server (Task 14) has handled, across every agent and shared connection —
 * the ring itself is server-wide, not per-connection, so this view takes no
 * `connId` and `MainShell` mounts it the same regardless of the active
 * connection.
 *
 * Polls `mcpActivity(lastSeq)` AND `mcpStatus()` every 2s, skipping a tick
 * while `document.hidden` or a previous poll is still in flight. `MainShell`
 * only renders this component while the "mcp" tab is the active one, so
 * mounting already covers "poll only while active" — unmounting stops the
 * interval. `gen`/`shouldApply` (reused from `sqlLog.ts`, same as
 * `MetricsBoard`'s widgets) drop a result that resolves after unmount, the
 * one reset event this view has (no clear button, no connection to switch
 * away from).
 *
 * Settings is an overlay, not a route — this view stays mounted underneath
 * it, so toggling the server from the header's own "Settings → MCP" button
 * and closing Settings again would otherwise leave the header showing the
 * pre-toggle status for up to one whole poll interval. A second effect
 * watches `useSettingsUi`'s `open` and refetches status the instant it flips
 * from true to false (fix round 1), on top of the poll tick above.
 *
 * Status is now written from two independent call sites (that poll tick and
 * the Settings-closed effect), so it gets its own request counter,
 * `statusReqRef`, separate from the activity poll's `genRef` (fix round 2):
 * every `refreshStatus()` call captures the counter's next value, and on
 * resolve applies the result only if no *newer* `refreshStatus()` call has
 * started since — via the same `shouldApply` used everywhere else — which
 * drops a response that resolves after unmount (both effects' cleanups bump
 * the counter) AND a slower response that would otherwise overwrite a
 * fresher one that started later but resolved first.
 */
export function McpActivityView() {
  const [activities, setActivities] = useState<McpActivity[]>([]);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [conns, setConns] = useState<ConnConfig[]>([]);
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const lastSeqRef = useRef(0);
  const genRef = useRef(0);
  const statusReqRef = useRef(0);
  const settingsOpen = useSettingsUi((s) => s.open);
  const wasSettingsOpenRef = useRef(settingsOpen);

  const refreshStatus = useCallback(() => {
    const req = ++statusReqRef.current;
    mcpStatus()
      .then((s) => {
        if (shouldApply(req, statusReqRef.current)) setStatus(s);
      })
      .catch((e) => logError("mcp activity: status", e));
  }, []);

  useEffect(() => {
    listConnections()
      .then(setConns)
      .catch((e) => logError("mcp activity: connections", e));
  }, []);

  useEffect(() => {
    let stopped = false;
    let inFlight = false;
    const gen = ++genRef.current;
    const tick = () => {
      if (stopped || document.hidden || inFlight) return;
      inFlight = true;
      // Independent of the activity fetch below (fix round 2) — a status
      // failure must not drop this tick's activity merge, and vice versa.
      refreshStatus();
      mcpActivity(lastSeqRef.current)
        .then((next) => {
          if (stopped || !shouldApply(gen, genRef.current) || next.length === 0) return;
          lastSeqRef.current = next[next.length - 1].seq;
          setActivities((prev) => mergeActivity(prev, next, MCP_ACTIVITY_CAP));
        })
        .catch((e) => logError("mcp activity: poll", e))
        .finally(() => {
          inFlight = false;
        });
    };
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      stopped = true;
      genRef.current++;
      statusReqRef.current++; // invalidate a status fetch this tick started but hasn't resolved yet
      window.clearInterval(id);
    };
  }, [refreshStatus]);

  // Settings just closed (true → false) — refresh immediately rather than
  // wait for the next poll tick, so flipping the server on/off in Settings
  // and closing it shows up in the header right away.
  useEffect(() => {
    const wasOpen = wasSettingsOpenRef.current;
    wasSettingsOpenRef.current = settingsOpen;
    if (wasOpen && !settingsOpen) refreshStatus();
    return () => {
      statusReqRef.current++; // invalidate on unmount, or before the next open/close flip
    };
  }, [settingsOpen, refreshStatus]);

  const toggleExpanded = (seq: number) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(seq)) next.delete(seq);
      else next.add(seq);
      return next;
    });

  const statusText = status?.running
    ? `Running at ${status.url}`
    : status?.error ?? "MCP server is stopped";

  return (
    <div className="flex h-full flex-col">
      <Toolbar>
        <StatusDot tone={status?.running ? "ok" : status?.error ? "danger" : "muted"} />
        <span className={status?.error && !status.running ? "text-danger" : "text-text"}>
          {statusText}
        </span>
        <ToolbarSpacer />
        <ToolbarButton label="Settings → MCP" onClick={openMcpSettings} />
      </Toolbar>
      <div className="min-h-0 flex-1 overflow-auto">
        {activities.length === 0 ? (
          <div className="p-4 text-xs text-muted">
            No MCP activity yet. Share a connection and enable the server in{" "}
            <button type="button" className="text-primary hover:underline" onClick={openMcpSettings}>
              Settings → MCP
            </button>
            , then connect an agent (Claude Code, Claude Desktop) — its tool calls will show up
            here.
          </div>
        ) : (
          <table className="w-full border-collapse text-xs">
            <thead className="sticky top-0 bg-surface text-[10px] uppercase tracking-wider text-muted">
              <tr>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Time</th>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Client</th>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Tool</th>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Connection</th>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">SQL</th>
                <th className="border-b border-border px-2 py-1 text-right font-semibold">Rows</th>
                <th className="border-b border-border px-2 py-1 text-right font-semibold">ms</th>
                <th className="border-b border-border px-2 py-1 text-left font-semibold">Status</th>
              </tr>
            </thead>
            <tbody>
              {activities.map((a) => (
                <ActivityRow
                  key={a.seq}
                  activity={a}
                  connLabel={connName(a.conn_id, conns)}
                  expanded={expanded.has(a.seq)}
                  onToggleExpanded={() => toggleExpanded(a.seq)}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function ActivityRow({
  activity: a,
  connLabel,
  expanded,
  onToggleExpanded,
}: {
  activity: McpActivity;
  connLabel: string;
  expanded: boolean;
  onToggleExpanded: () => void;
}) {
  return (
    <tr className="hover:bg-surface-2">
      <td className="whitespace-nowrap border-b border-border px-2 py-1 font-mono text-muted">
        {formatLogTs(a.ts_ms * 1000)}
      </td>
      <td className="max-w-[140px] truncate border-b border-border px-2 py-1" title={a.client}>
        {a.client}
      </td>
      <td className="whitespace-nowrap border-b border-border px-2 py-1 font-mono">{a.tool}</td>
      <td className="max-w-[140px] truncate border-b border-border px-2 py-1" title={connLabel}>
        {connLabel}
      </td>
      <td className="border-b border-border px-2 py-1 font-mono">
        {a.sql === null ? (
          <span className="text-muted">—</span>
        ) : expanded ? (
          <div className="flex flex-col items-start gap-1">
            <pre className="max-w-full whitespace-pre-wrap break-all text-[11px]">{a.sql}</pre>
            <button
              type="button"
              className="text-primary hover:underline"
              aria-expanded={true}
              aria-label={`Hide full SQL (row ${a.seq})`}
              onClick={onToggleExpanded}
            >
              Collapse
            </button>
          </div>
        ) : (
          <div className="flex min-w-0 items-center gap-2">
            <span className="min-w-0 truncate">{oneLine(a.sql)}</span>
            <button
              type="button"
              className="shrink-0 text-primary hover:underline"
              aria-expanded={false}
              aria-label={`Show full SQL (row ${a.seq})`}
              onClick={onToggleExpanded}
            >
              Expand
            </button>
          </div>
        )}
      </td>
      <td className="border-b border-border px-2 py-1 text-right font-mono">{a.rows ?? "—"}</td>
      <td className="border-b border-border px-2 py-1 text-right font-mono">{a.ms}</td>
      <td className="border-b border-border px-2 py-1">
        {a.ok ? (
          <span className="inline-flex items-center gap-1.5 text-ok">
            <StatusDot tone="ok" />
            OK
          </span>
        ) : (
          <span className="inline-flex items-center gap-1.5 text-danger" title={a.error ?? undefined}>
            <StatusDot tone="danger" />
            Error
          </span>
        )}
      </td>
    </tr>
  );
}
