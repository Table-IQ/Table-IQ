# TODOS

Deferred from the Connected Home plan review (2026-09-27, `docs/superpowers/plans/2026-09-27-connected-home.md`).

- **Read-only production mode.** Production connections currently only confirm staged commits; a true read-only mode (refuse writes in the SQL editor too) is out of scope.
- **Confirm `+ Row` inserts in production.** The commit confirm covers staged cell edits only.
- **AI Ask inside ⌘K.** The lobby Ask bar covers it for now.
- **MCP status dot in the titlebar pill.**
- **Per-connection commit-confirm toggle.** Today it is tied to `env === "production"`.
- **Tag lobby queries in the SQL log.** `table_stats` shows in the log each time the lobby fetches.
- **Show Tour on non-main screens.** Help → Show Tour does nothing on the ConnectionManager, Kv and Doc screens; the tour lives in MainShell.
- **Narrow-window pill** (QA ISSUE-003, low). At 960px with both side panels open, the titlebar pill shrinks to env + engine only; the full text is in its tooltip. Consider collapsing a side panel or the engine label before the connection name.
- **Tauri-build QA pending.** Check the native View/Help menu bar (one of each), ⌘K twice toggles once, editor keys ⌘F/⌘[/⌘]/⌘⇧\ in CodeMirror, window drag from header gaps, and traffic-light centring on the ConnectionManager/Kv/Doc screens.
