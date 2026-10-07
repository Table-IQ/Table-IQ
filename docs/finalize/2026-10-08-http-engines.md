# v0.8.7: ClickHouse, Turso (libSQL) and Elasticsearch, read-only

## Done
- ClickHouse (`drivers/clickhouse.rs`, HTTP interface, TSV with types), Turso/libSQL (`drivers/libsql.rs`, Hrana v2 pipeline/batch/sequence), Elasticsearch (`drivers/doc_elastic.rs` in the shared document shell, OpenSearch detected). All read-only in the grid, none shareable over MCP.
- Wiring per the "adding an engine" recipe: Engine union, icons, connection sheet tiles (10, wrapping), URL/secret handling (tokens and passwords never saved in `host`), SQL dialect and gating, backup/import refusals.
- Infra: `docker/clickhouse`, `docker/libsql` (open + JWT), `docker/elasticsearch`; a separate `http-engines` CI job so their ~2 GB does not share a 7 GB runner with ten other servers.

## Evidence
- `cargo test --locked` (all targets, CI-like env, containers up): everything passes except `redis_tls`, which CI runs in its own step with `SSL_CERT_FILE`. Two CockroachDB-dependent binaries (`mcp_live`, `profile_live`) failed once because the Docker VM OOM-killed the CockroachDB container (exit 137) and passed on a re-run (12/12, 10/10); the same happened to the Elasticsearch container, re-run 14/14. `clickhouse_live` 16/16, `libsql_live` 13/13, `elasticsearch_live` 14/14 (`TIQ_REQUIRE_LIVE=1`). Clippy `-D warnings` clean on Rust 1.99, `tsc` clean, vitest 1000 passed.
- Real-bridge browser QA (gstack browse, real `/__ipc`): connect, browse, page 200k/20k/5k rows, filter, sort, type rendering, read-only gating, SQL editor, cancel `SELECT sleep(3)`, URL paste moving tokens to the password field, bad-credential errors with no secret leaked, ES Query DSL / query_string / window message / Mapping / Aggregate, regression on Postgres, Mongo and Redis. 55 screenshots in `docs/finalize/assets/http-engines/` (e.g. `06-*` ClickHouse grid, `22-*` libSQL types, `38-*` Elasticsearch window message).
- QA found and fixed four UI bugs (ClickHouse opened on `default` instead of the connection's database; lobby header `http://host:58081:8080`; stray `:` in the title bar; stale `127.0.0.1` in the Server URL after switching tiles). Independent review found no credential/redirect/injection problem (reqwest strips credentials on cross-host redirects; no cert verification is skipped); fixed: ClickHouse cancel `query_id` collisions across processes, `readonly=1` users, unbounded response body, libSQL ±Inf failing a page, parse-error attribution, prefix-match "local" checks that sent a bearer token over http to `127.evil.example`, Elasticsearch seed writing its marker after a failed bulk, scheme-less `user:pw@host` and query secrets saved in plaintext, aggregation field collisions.

## Not verified / limits
- No cloud run: Turso cloud, Elasticsearch with security on, ClickHouse over HTTPS were not exercised (local servers only; the container has no auth, so a wrong-password rejection is covered for ClickHouse and libSQL only).
- `libsql://127.0.0.1:port` goes over TLS (scheme says so) and fails against a plain local `sqld`; use `http://127.0.0.1:port`.
- No private CA for HTTPS servers; ClickHouse `OFFSET` paging can repeat rows on tables without a unique key; SQL editor autocompletes MySQL keywords for ClickHouse/libSQL.
- Settings → MCP does not list these engines at all (the "not shareable" note is never shown): matches `mcp.test.ts` but the copy is misleading.
- The new `http-engines` CI job was not run locally as a job; its commands were.
