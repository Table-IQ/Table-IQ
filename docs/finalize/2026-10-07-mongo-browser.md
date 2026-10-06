# v0.8.5: full MongoDB browser

## Done
- Backend: filter/sort/projection/skip/limit, count, canonical Extended JSON, staged `apply_changes` (transaction on a replica set, best-effort on standalone), indexes, aggregate (`$out`/`$merge` refused), `mongodb+srv://` and full connection strings, real Test connection.
- UI (`DocShell`): collection sidebar with counts, query bar, Table/JSON/Tree views, staged edit/delete/insert with a mongo-shell Review, indexes pane, aggregation runner.
- MCP: `mongo_list_collections`, `mongo_find`, `mongo_count`, `mongo_aggregate`, read-only, fail-closed guard (`mcp/mongo_guard.rs`), no write tools.
- CI: MongoDB replica set + standalone in `docker/mongo/`, `mongo_browse` and `mongo_mcp_live` in the must-not-skip step; `clippy::double_must_use` allowed (Rust 1.99 vs `async_trait`; fixes main's red clippy after #20).

## Evidence
- `cargo test --locked` (no REQUIRE_LIVE): exit 0, lib 383 passed. `TIQ_REQUIRE_LIVE=1` `mongo_browse` 15/15, `mongo_guard` 6/6, `mongo_mcp_live` 9/9. Clippy `-D warnings` clean locally (Rust 1.95). `tsc` clean, vitest 904 passed.
- Real-bridge QA (not the mock) against the replica set and standalone server through `browse`: replica-set commit of 3 changes in one transaction, optimistic conflict ("nothing was committed"), standalone best-effort, filter/sort/paging, invalid-filter error with no request sent, indexes, aggregate, `$out` refusal, truncation notice; `browse console --errors` clean. 21 screenshots in `docs/finalize/assets/mongo/` (e.g. `02-users-table.png`, `07-committed-replica-set.png`, `13-standalone-best-effort.png`).
- Guard: about 160-case adversarial table (every object case runs as filter, sort, projection and `$match`), also run live over HTTP with a before/after snapshot of the whole database; 9-way mutation check caught every break.
- Independent security review found a real bypass (`$_internalJsEmit` / `$_internalJsReduce` run server-side JS through `mongo_aggregate`); fixed by refusing every `$_` key, added to the corpus with a live proof. Also fixed: typed password in Host persisted in plaintext, standalone partial failure dropping failed edits, `diffDoc` edge cases, a healthcheck that re-seeded forever.

## Not verified / limits
- Bridge QA was not re-run after the review fixes (unit tests cover them).
- `mongodb+srv://` is unit-tested at the URL level only; no live SRV test (needs DNS and a cluster).
- The new `clippy::double_must_use` allow is untested locally (local Rust 1.95 doesn't raise the lint); CI on Rust 1.99 is the check.
- Agents see whatever database the session is on; the guard cannot hide data the Mongo role can read. Share through a least-privilege role.
- Whole-document edit only from the Table view's context menu; projected queries are read-only.
