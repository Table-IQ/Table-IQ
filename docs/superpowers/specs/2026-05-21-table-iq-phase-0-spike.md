# Table-IQ — Phase 0 Spec (Foundations & Risk Spike)

Derived from the approved build plan (`/Users/rudy/.claude/plans/this-is-a-phenomenal-twinkling-wall.md`). This spec scopes **only Phase 0**: prove the make-or-break architecture before building product features.

## Goal

A runnable Tauri v2 + React scaffold that **proves the performance architecture** with measured numbers, plus the trait skeleton everything else will hang off. This is a go/no-go gate, not a feature.

## Success criteria (numeric — the gate)

Against a synthetic test table of **1,000,000 rows × 100+ columns** including `NUMERIC(38,10)`, a large `TEXT`/`JSON` blob column, a NULL-heavy column, and `timestamptz`:

1. **Scroll smoothness:** sustained **60 fps** (target **120 fps** on ProMotion) during programmatic fast-scroll through the grid.
2. **Bounded JS heap:** resident JS heap stays **< N MB regardless of result size** — N is *set during the spike* (hypothesis: < 150 MB) and recorded as the standing budget. Heap must not grow with total row count.
3. **Window-fetch latency:** the Rust→UI row-window request **p99 < 16 ms**.

Both windowing paths (below) must meet these. If they can't, the architecture is reconsidered before Phase 1.

## In scope

1. **Scaffold:** Tauri v2 + React + Vite + Tailwind + Zustand; runs on macOS (arm64); `pnpm tauri dev` launches.
2. **Driver trait skeleton:** `SqlDriver`, `DocumentDriver`, `KvDriver` defined with the shared `FetchWindow`, `RowKey`, `CancelToken` types from the plan. Phase 0 needs only a **Postgres `SqlDriver`** implementation (for the cursor path); `DocumentDriver`/`KvDriver` are trait definitions + `todo!()` stubs that compile.
3. **Two windowing paths (spec #2 of plan):**
   - **(a) Materialize:** synthesize the 1M-row dataset as an Arrow table in Rust, hold it resident, serve row-windows to the UI on request.
   - **(b) Cursor:** stream the same shape from **Postgres (Docker)** via keyset pagination with **bounded Rust RAM** (RAM must not scale with table size). Measured with a memory probe.
4. **Three IPC transports** for the window payload, switchable at runtime and benchmarked:
   - raw-byte `tauri::ipc::Response` (no base64),
   - `tauri::ipc::Channel`,
   - custom async URI-scheme protocol.
   Payload encoded as **Arrow IPC**, decoded in JS with `apache-arrow`.
5. **Grid:** `glide-data-grid` with a windowed data source — `getCellContent` reads from the currently-loaded Arrow window; out-of-window scroll triggers a fetch + small prefetch buffer.
6. **Benchmark harness:** an in-app "Bench" view that runs a scripted fast-scroll, measures fps (rAF deltas), JS heap (`performance.memory` where available), and window-fetch p99, for each transport × each windowing path, and writes a `bench.json` + on-screen table.

## Out of scope (Phase 0)

- No connection manager UI, SSH, keychain, AI, schema browser, SQL editor, editing/writes, MySQL/SQLite/SQL Server/Mongo/Redis impls (trait stubs only), no Windows/Linux. All deferred to later phases.

## Components & file layout

```
Table-IQ/
├─ src-tauri/
│  ├─ Cargo.toml                      # tauri, tokio, sqlx(pg), arrow, deadpool, serde
│  ├─ src/main.rs                     # Tauri app, command + channel + protocol wiring
│  ├─ src/drivers/mod.rs              # SqlDriver/DocumentDriver/KvDriver traits + shared types
│  ├─ src/drivers/postgres.rs         # SqlDriver impl: keyset cursor streaming
│  ├─ src/query/window.rs             # FetchWindow: Materialize + Cursor; bounded RAM
│  ├─ src/query/synth.rs              # synthetic 1M-row Arrow generator (test dataset shape)
│  ├─ src/ipc/grid_transport.rs       # 3 transports: Response / Channel / custom protocol
│  └─ src/bench/mod.rs                # server-side timing + memory probe
├─ src/
│  ├─ main.tsx, App.tsx
│  ├─ components/DataGrid.tsx         # glide-data-grid + windowed Arrow source
│  ├─ components/BenchView.tsx        # scripted scroll + fps/heap/p99 capture
│  ├─ lib/arrowWindow.ts             # arrow-js decode + window cache + prefetch
│  ├─ lib/transport.ts               # call each transport, time it
│  └─ state/benchStore.ts            # Zustand: selected transport/path, results
├─ docker/postgres/                   # compose + seed for the cursor-path dataset
└─ bench/                             # output artifacts (gitignored)
```

## Data flow

1. UI selects (windowing path, transport). 2. UI requests window `[offset, len]` via `lib/transport.ts`. 3. Rust resolves from the resident Arrow table (Materialize) or the Postgres keyset cursor (Cursor), slices the window, encodes Arrow IPC, returns via the chosen transport. 4. `lib/arrowWindow.ts` decodes into typed arrays, caches the window (+prefetch). 5. `DataGrid.tsx` `getCellContent` reads the cache. 6. `BenchView.tsx` scripts scroll and records metrics.

## Testing (TDD)

- **Rust unit:** `synth.rs` produces correct schema/row count/null distribution; `window.rs` Materialize slicing returns correct rows for edge offsets (0, last, past-end); Arrow IPC round-trips (encode→decode equals source) for NUMERIC/timestamptz/NULL/large-text.
- **Rust integration (Docker PG):** cursor keyset pagination returns the full set in order with **bounded RSS** (assert peak memory under a cap while paging 1M rows).
- **Frontend unit (vitest):** `arrowWindow.ts` decodes a fixture Arrow IPC buffer to expected values; window cache evicts beyond budget; prefetch math.
- **Bench (not a pass/fail unit test):** the harness produces the metrics table; the *gate* is reviewed against the numeric criteria.

## Risks specific to Phase 0

- `performance.memory` is Chromium-only and coarse; on macOS WKWebView it may be unavailable → fall back to measuring via Rust-side process RSS + DOM/JS object counts, and document the method.
- Measuring "fps" reliably under programmatic scroll requires driving scroll at a fixed cadence and reading rAF deltas; document the scroll script so numbers are reproducible.
- Custom URI-scheme protocol wiring in Tauri v2 is the fiddliest transport; if it underperforms or is too complex, record that and prefer Response/Channel.
