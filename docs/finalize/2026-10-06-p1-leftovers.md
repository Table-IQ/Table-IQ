# v0.8.4: P1 engine-profile leftovers

## Done
- Cloud URL presets (RDS/Aurora, Azure PG/MySQL, Cloud SQL; Azure SQL TLS host).
- Transaction probe at connect (only a server error on `BEGIN` turns transactions off).
- Explain-unavailable message for CockroachDB; Metrics read-only error naming the engine.
- `TIQ_REQUIRE_LIVE` via shared `live_url` in 25 live test binaries, wired into CI.
- CockroachDB opened to MCP (own guard dialect, live-proven); TiDB kept closed. Allowlist entries carry an access generation (`rev`), so entries from before an engine opened must be re-shared.

## Evidence
- `cargo test --locked`: full run exit 0 (after the review fixes, `--lib` 369 passed, `guard_suite`, `profile_live` 10/10 and `mcp_live` 12/12 re-run green). `cargo clippy --all-targets -D warnings` clean. `tsc` clean, vitest 844 passed. Run against live PG 16, MySQL 8.4, TiDB 8.5, CockroachDB 26.3, MariaDB 11.
- One TiDB flake ("Information schema is out of date") during `profile_live` under high machine load; passed after restarting the container.
- Independent review found two issues, both fixed: replication statements (`CREATE VIRTUAL CLUSTER … FROM REPLICATION`) were not refused on a writes-allowed CockroachDB connection; the Metrics retry could mislabel an unrelated failure.

## Not verified
- No browser QA through the IPC bridge for this slice (the UI change is two message strings and the MCP note). Not covered: `/qa`, `/design-review`, screenshots.
- Tests were written alongside the code for tasks 1-4, not strictly test-first.
- The 16 Beta profiles were not re-run: no local images or cloud accounts. Cloud presets are `tested:false`.
- TiDB over MCP remains closed (TODOS.md).
