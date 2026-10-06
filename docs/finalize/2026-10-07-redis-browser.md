# v0.8.6: full Redis browser

## Done
- Backend: bounded paged reads for every type (string, list, set, hash, zset, stream, RedisJSON), binary-safe values, `MULTI`/`EXEC` batches with `WATCH` and optimistic `expect`/`expectAbsent`, TTL/rename/delete, a console with a denylist and a confirm class, ACL user names, `rediss://` with certificate and host name verified, URL-in-host with the password moved to the keychain.
- UI (`KvShell`): `:` folder key tree over paged SCAN, per-type editors with staged changes and a redis-command Review, Console tab with history, connection sheet for `redis://`/`rediss://` and ACL users.
- MCP: `redis_scan`, `redis_get`, `redis_dbsize`, read-only, fail-closed guard (`mcp/redis_guard.rs`), no command tool, no write path.
- Infra: `docker/redis/` (Redis Stack, ACL, TLS, scratch servers), CI services and required-live steps.

## Evidence
- `cargo test --locked` (CI-like env): lib 403 passed; every target passes except `redis_tls`, which needs its own `SSL_CERT_FILE` (CI runs it in a separate step) — run that way: 1/1. With `TIQ_REQUIRE_LIVE=1`: `redis_browse` 18, `redis_mcp_live` 8, `redis_guard` 6. Clippy `-D warnings` clean on Rust 1.99. `tsc` clean, vitest 968 passed.
- Real-bridge QA (not the mock) via `browse`: tree, per-type staged edits committed (verified in redis-cli), optimistic conflict, paging a 5,000-item list, TTL/persist/rename/delete, console incl. refused and confirm flows, binary key view, `rediss://` paste. 22 screenshots in `docs/finalize/assets/redis/` (e.g. `key-tree`, `hash-review`, `hash-conflict`, `console`).
- MCP guard: 177 refused inputs (362 live calls); the oracle checks `INFO commandstats` write counters, `DBSIZE` and a `DUMP` of every key are unchanged; the oracle sees a raw `PING\r\nSET` really injecting a second command and the guard stopping the same string; 20-way mutation check catches every removal.
- Independent review found no way for an agent to write, plus data-corruption bugs in the UI, all fixed with tests: JSON edits rewrote big integers and `1.0`; list "Load more" dropped duplicate values; removing a list item deleted the wrong duplicate; secrets stayed in console history; a failed `EXEC` offered a blind retry that duplicated entries; a new hash field overwrote an unseen one (new `expectAbsent`). Backend: SSH tunnel errors leaked to agents (now an allowlist of error codes), seed TTLs expired after an hour and broke tests, the console confirm class was too narrow.

## Not verified / limits
- Cluster and Sentinel are not implemented (TODOS.md); no private-CA option for `rediss://`.
- The review fixes were checked by unit tests and a curl spot check over the bridge for list removal and JSON big integers; the browser QA was not re-run.
- `SCAN` ignores key ACLs, so `redis_scan` with `*` fails for the restricted ACL user; agents must use the `agent:*` prefix (stated in the tool description).
- Keys must be UTF-8; a binary-named key is listed but cannot be opened.
- Staged changes to a very large key reload only the first page after commit.
