# TODOS

Deferred from the Connected Home plan review (2026-09-27, `docs/superpowers/plans/2026-09-27-connected-home.md`).

- **Read-only production mode.** Production connections currently only confirm staged commits; a true read-only mode (refuse writes in the SQL editor too) is out of scope.
- **AI Ask inside ⌘K.** The lobby Ask bar covers it for now.
- **MCP status dot in the titlebar pill.**
- **Per-connection commit-confirm toggle.** Today it is tied to `env === "production"`.
- **Tag lobby queries in the SQL log.** `table_stats` shows in the log each time the lobby fetches.
- **Show Tour on non-main screens.** Help → Show Tour does nothing on the ConnectionManager, Kv and Doc screens; the tour lives in MainShell.
- **Narrow-window pill** (QA ISSUE-003, low). At 960px with both side panels open, the titlebar pill shrinks to env + engine only; the full text is in its tooltip. Consider collapsing a side panel or the engine label before the connection name.
- **Tauri-build QA pending.** Check the native View/Help menu bar (one of each), ⌘K twice toggles once, editor keys ⌘F/⌘[/⌘]/⌘⇧\ in CodeMirror, window drag from header gaps, and traffic-light centring on the ConnectionManager/Kv/Doc screens.
- **Connection sheet follow-ups** (2026-09-28):
  - Validate the SSH host and port.
  - Strip `?query` from pasted `sqlite://` URLs.
  - Add `aria-live` to the "Copied" confirmation.
  - Make the "Saved" status neutral instead of green.
  - Let Test fall back to the saved keychain password when the field is blank. This needs a backend change.
- **Staged changes follow-ups** (2026-09-28):
  - Merge a row's edits into one UPDATE (fixes the composite-PK two-column edit).
  - Refresh the Inspector after a commit.
  - Hide the UPDATE footnote in a delete-only Review.
  - Add a "Delete rows" menu item for multi-row selection.
  - Guard against repeated inserts when retrying a commit whose outcome was uncertain.
- **MCP contract follow-ups** (2026-09-29, from the v0.8.1 adversarial review):
  - After a Postgres/MySQL/SQLite timeout, `run_query` replies "The query was cancelled." before the cancel is even sent (`src-tauri/src/mcp/server.rs`, `on_timeout`). Say "TableIQ asked the database to cancel the query" instead, or wait for the cancel result.
  - "Reads run inside a read-only transaction" is only literally true on Postgres and MySQL. SQLite, DuckDB and SQL Server use BEGIN…ROLLBACK, which undoes a write rather than refusing it.
  - With writes allowed on SQL Server, "exactly one statement" isn't enforced (T-SQL doesn't need `;` between statements), and only the first result set comes back.
  - A MySQL `CALL` that returns several result sets merges their rows, so `truncated: false` doesn't always mean the result is complete.

## Completed

- **Confirm `+ Row` inserts in production.** New rows are now staged and go through the production commit confirm. **Completed:** v0.8.0 (2026-09-28)
