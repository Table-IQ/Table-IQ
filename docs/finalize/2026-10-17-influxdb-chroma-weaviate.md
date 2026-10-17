# v0.8.9: InfluxDB 3, ChromaDB and Weaviate, read-only

## Done
- InfluxDB 3 (`drivers/influxdb.rs`, SQL over HTTP, Parquet results, binds cast to the column type), ChromaDB (`drivers/doc_chroma.rs`) and Weaviate (`drivers/doc_weaviate.rs`, GraphQL built from a whitelist) in the shared document shell. All read-only; none shareable over MCP.
- Security rules from earlier reviews applied from the start (credentials only in sensitive headers, never in saved host or name, redirects off, exact-loopback plain http, 64 MiB cap, query secrets refused in TS and Rust).
- Infra: `docker/{influxdb,chroma,weaviate}`; InfluxDB runs three servers (open, token, and Core's default file limit), Chroma sits behind an nginx token gate because the pinned server has no auth; a second CI job `search-engines` holds the new engines and the HTTP search engines.

## Evidence
- Local on Rust 1.99, final run: clippy `-D warnings` clean; `--lib` 499 passed; with `TIQ_REQUIRE_LIVE=1`: `influxdb_live` 13, `chroma_live` 20, `weaviate_live` 23. `tsc` clean, vitest 1080 passed.
- Real-bridge browser QA of all three engines (connect, URL paste with secrets moving out, bad-credential errors, browse and page 20k/5k rows, filters, sort, SQL editor, window messages, Mapping/Aggregate, read-only gating, Settings → MCP). About 80 screenshots in `docs/finalize/assets/slice6-engines/`. QA found and fixed six UI bugs (stale pager under an error, stale title-bar database, wrong read-only copy, write affordances on read-only engines, "(unnamed)" connections, "1 columns").
- Independent review found no GraphQL/SQL injection or credential leak. Fixed: Weaviate made up to ~5000 REST calls per page for null ints (now a handful, failures degrade), Chroma showed "10,000+" for large unfiltered collections, a pasted `http://…:8080` flipped the sheet to Weaviate (also a local Turso), truncated error copy, "Password field" wording. QA found InfluxDB 3 Core refuses to open a big table unfiltered ("would scan 432 Parquet files"): pages and counts now retry on a time window of the newest files with a clear message and an "Add a filter" button.

## Not verified / limits
- Local servers only (no TLS endpoint, no cloud, no InfluxDB 3 Enterprise); Chroma Cloud and Weaviate Cloud untested.
- The InfluxDB file-limit fix and the Postgres/Mongo/Redis regression were not re-run in a browser after the final changes (unit and live tests only; those containers were not up).
- Known gaps: an InfluxDB windowed page covers only the newest data; Weaviate sort on a sparse property still reports the class size; Chroma `chroma://host/db` path doesn't fill Database; first-use keychain race on the MCP token; an explicit `http://` URL to a remote host sends the token unencrypted; Parquet decoding of a hostile 64 MiB reply is not bounded.
