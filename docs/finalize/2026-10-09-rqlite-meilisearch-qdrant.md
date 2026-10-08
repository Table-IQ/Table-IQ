# v0.8.8: rqlite, Meilisearch and Qdrant, read-only

## Done
- rqlite (`drivers/rqlite.rs`, HTTP API, SQLite dialect), Meilisearch (`drivers/doc_meili.rs`) and Qdrant (`drivers/doc_qdrant.rs`) in the shared document shell. All read-only; none shareable over MCP.
- Wiring and the security rules from the v0.8.7 review applied from the start: credentials only in sensitive headers, never in saved hosts or errors, redirects off, exact-loopback plain http, 64 MiB body caps, query secrets refused.
- Infra: `docker/{rqlite,meilisearch,qdrant}` (rqlite also with an auth-enabled instance), run by the `http-engines` CI job, which now has a job timeout and `--wait-timeout 300` on every service start.

## Evidence
- Local on Rust 1.99, final run: clippy `-D warnings` clean; `cargo test --lib` 462 passed; with `TIQ_REQUIRE_LIVE=1`: `rqlite_live` 13, `meilisearch_live` 18, `qdrant_live` 20, `libsql_live` 13, `clickhouse_live` 16 (the last two re-run because shared helpers changed). `tsc` clean, vitest 1044 passed.
- Real-bridge browser QA (gstack browse) of all three engines: connect, URL paste with credentials moving to fields and `?api_key=` refused, bad-key errors without secrets, browse and page (rqlite 20k rows, Meilisearch 5000, Qdrant 5000), filters, sorts, window messages, Mapping/Settings, Aggregate/facets, SQL editor, read-only gating, reader-user permission errors; Postgres and Mongo regression. 86 screenshots in `docs/finalize/assets/slice5-engines/` (e.g. `40-*` Meilisearch, `60-*` Qdrant). QA fixed seven UI bugs: the key in a pasted URL leaked into the connection Name, the api_key error only appeared on blur, rqlite Alter/New Table dialogs dead-ended, empty read-only grids were blank, Aggregate and Settings → MCP empty-state copy.
- Independent review found no credential leak, no write path through rqlite's browse code, no MCP gap. Fixed: base64 mis-decoding of text in BLOB columns (now sends `blob_array`), integers past 2^53 shown rounded (now exact strings), password redaction breaking the write retry, transport errors dropping earlier results, memory spike on large results, CI hang risk.

## Not verified / limits
- Local servers only (no TLS endpoint, no cloud); Elasticsearch regression not re-run (container stopped for memory).
- Known gaps: rqlite `run_script` is always refused (the UI runs DDL one statement at a time instead); a Meilisearch page that straddles `maxTotalHits` is returned short without a hint; Qdrant named-vector summaries have inconsistent key order; an explicit `http://` URL to a remote host sends credentials unencrypted; rqlite SELECT of a BLOB expression shows lossy text; Edit → Test connection ignores the saved password/key.
- Settings → MCP lists none of the new engines (by design, with clearer copy now).
