# Table-IQ Phase 0 (Foundations & Risk Spike) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the Table-IQ performance architecture — a Tauri v2 + React app that windows a 1M-row Arrow result into a `glide-data-grid` at 60/120fps with bounded JS heap and p99 window-fetch < 16ms, across two windowing paths (Materialize + Postgres cursor) and three IPC transports.

**Architecture:** Rust core holds/streams data as Apache Arrow and serves fixed-size row windows; the React UI requests windows on scroll, decodes Arrow IPC with `apache-arrow`, caches a small window + prefetch, and renders via glide's per-cell callback. A bench harness drives scripted scroll and records fps/heap/latency per (path × transport).

**Tech Stack:** Tauri v2, Rust (tokio, arrow-rs, sqlx/postgres, deadpool), React + Vite + TypeScript + Tailwind + Zustand, `@glideapps/glide-data-grid`, `apache-arrow`, Docker Postgres.

**Environment note:** Rust is at `~/.cargo` — every Rust step must be run after `source "$HOME/.cargo/env"`. Node is via nvm (v24), pnpm available.

---

## File Structure

```
src-tauri/
  Cargo.toml                  # deps
  tauri.conf.json             # tauri config (from scaffold, edited)
  src/main.rs                 # app entry, command/channel/protocol registration, AppState
  src/drivers/mod.rs          # shared types (FetchWindow, RowKey, CancelToken, RawCell, ...) + 3 traits
  src/drivers/postgres.rs     # SqlDriver impl: keyset cursor streaming (cursor path)
  src/query/synth.rs          # synthetic 1M-row Arrow RecordBatch generator (materialize path)
  src/query/window.rs         # WindowSource enum: Materialize | Cursor; slice -> Arrow IPC bytes
  src/ipc/transport.rs        # 3 transports + Tauri commands (get_window_response / _channel) + protocol
  src/bench/mod.rs            # server-side timing + process RSS probe
src/
  main.tsx, App.tsx, index.css
  lib/arrowWindow.ts          # arrow-js decode + window cache + prefetch
  lib/transport.ts            # invoke each transport, time round-trip
  state/benchStore.ts         # Zustand: path, transport, metrics
  components/DataGrid.tsx     # glide-data-grid + windowed source
  components/BenchView.tsx    # scripted scroll, fps/heap/p99 capture, results table
docker/postgres/
  docker-compose.yml          # postgres:16
  seed.sql                    # 1M-row wide table matching the test shape
bench/                        # gitignored output (bench.json)
```

---

## Task 1: Scaffold Tauri v2 + React + Vite + TS

**Files:**
- Create: whole `src-tauri/` + `src/` + `package.json` + `vite.config.ts` (via scaffold tool)

- [ ] **Step 1: Scaffold with create-tauri-app (non-interactive)**

Run:
```bash
cd /Users/rudy/dev/rudy/Table-IQ
source "$HOME/.cargo/env"
# scaffold into current (empty-ish) dir using pnpm + react-ts template
pnpm create tauri-app@latest . --template react-ts --manager pnpm --yes 2>&1 | tail -20
```
Expected: creates `src/`, `src-tauri/`, `package.json`, `vite.config.ts`. If the tool refuses because the dir isn't empty (it has `.git`, `docs/`, `.gitignore`), scaffold into a temp dir and move files in:
```bash
mkdir -p /tmp/tiq && (cd /tmp/tiq && pnpm create tauri-app@latest app --template react-ts --manager pnpm --yes)
rsync -a --exclude .git /tmp/tiq/app/ /Users/rudy/dev/rudy/Table-IQ/
```

- [ ] **Step 2: Install JS deps**

Run: `cd /Users/rudy/dev/rudy/Table-IQ && pnpm install 2>&1 | tail -5`
Expected: lockfile written, no errors.

- [ ] **Step 3: Verify Rust core builds**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo build 2>&1 | tail -15`
Expected: compiles (first build downloads tauri crates — slow, OK).

- [ ] **Step 4: Add Tailwind + Zustand + grid + arrow**

Run:
```bash
cd /Users/rudy/dev/rudy/Table-IQ
pnpm add zustand @glideapps/glide-data-grid apache-arrow
pnpm add -D tailwindcss @tailwindcss/vite
```
Edit `vite.config.ts` to add the tailwind plugin; replace `src/index.css` first line with `@import "tailwindcss";`.
Expected: deps in package.json.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: scaffold Tauri v2 + React + Vite + Tailwind + Zustand"
```

---

## Task 2: Rust deps + driver trait skeleton

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/drivers/mod.rs`
- Test: `src-tauri/src/drivers/mod.rs` (unit `#[cfg(test)]`)

- [ ] **Step 1: Add Rust deps to `src-tauri/Cargo.toml`** (under `[dependencies]`)

```toml
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
arrow = { version = "53", features = ["ipc"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "rust_decimal"] }
deadpool = "0.12"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
sysinfo = "0.32"
```

- [ ] **Step 2: Write the failing test** in `src-tauri/src/drivers/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_window_variants_construct() {
        let m = FetchWindow::Materialize { cap_rows: 1_000_000, cap_bytes: 512 << 20 };
        let c = FetchWindow::Cursor { page_size: 10_000 };
        assert!(matches!(m, FetchWindow::Materialize { .. }));
        assert!(matches!(c, FetchWindow::Cursor { .. }));
    }

    #[test]
    fn rowkey_holds_identity_and_original() {
        let k = RowKey { by: KeyBy::PrimaryKey(vec!["id".into()]), original: vec![RawCell::Text("1".into())] };
        assert!(matches!(k.by, KeyBy::PrimaryKey(_)));
        assert_eq!(k.original.len(), 1);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test drivers:: 2>&1 | tail -15`
Expected: FAIL — `FetchWindow`/`RowKey`/`KeyBy`/`RawCell` not defined.

- [ ] **Step 4: Write minimal implementation** (top of `src-tauri/src/drivers/mod.rs`)

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FetchWindow {
    /// Hold the whole result resident as Arrow iff under caps.
    Materialize { cap_rows: usize, cap_bytes: usize },
    /// Keyset/cursor stream from the DB; bounded RAM.
    Cursor { page_size: usize },
}

#[derive(Debug, Clone)]
pub enum RawCell { Null, Text(String), Bytes(Vec<u8>) }

#[derive(Debug, Clone)]
pub enum KeyBy { PrimaryKey(Vec<String>), AllColumns }

#[derive(Debug, Clone)]
pub struct RowKey { pub by: KeyBy, pub original: Vec<RawCell> }

/// Opaque per-engine cancellation handle (Phase 0: placeholder).
#[derive(Debug, Clone, Default)]
pub struct CancelToken;

#[derive(thiserror::Error, Debug)]
pub enum DriverError {
    #[error("db error: {0}")] Db(String),
    #[error("not implemented")] NotImplemented,
}
pub type DResult<T> = Result<T, DriverError>;

// Phase 0 needs SqlDriver fully; Document/Kv are defined but stubbed.
#[async_trait]
pub trait SqlDriver: Send + Sync {
    /// Open a cursor over `sql`, returning a handle the window layer pages through.
    async fn open_cursor(&self, sql: &str, page_size: usize) -> DResult<CursorId>;
    /// Fetch the next `n` rows from a cursor as an Arrow RecordBatch (IPC-encoded by caller).
    async fn fetch_next(&self, cursor: &CursorId, n: usize) -> DResult<arrow::record_batch::RecordBatch>;
    async fn close_cursor(&self, cursor: &CursorId) -> DResult<()>;
}

#[async_trait]
pub trait DocumentDriver: Send + Sync {
    async fn list_collections(&self) -> DResult<Vec<String>> { Err(DriverError::NotImplemented) }
}
#[async_trait]
pub trait KvDriver: Send + Sync {
    async fn scan(&self, _pattern: &str) -> DResult<Vec<String>> { Err(DriverError::NotImplemented) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CursorId(pub String);

pub mod postgres;
```

- [ ] **Step 5: Declare module in `src-tauri/src/main.rs`** — add `mod drivers;` and create empty `src-tauri/src/query/mod.rs`, `src-tauri/src/ipc/mod.rs`, `src-tauri/src/bench/mod.rs` placeholders + `mod query; mod ipc; mod bench;`. Add a stub `src-tauri/src/drivers/postgres.rs` with `// impl in Task 6`.

- [ ] **Step 6: Run test to verify it passes**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test drivers:: 2>&1 | tail -15`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(core): driver trait skeleton + shared windowing types"
```

---

## Task 3: Synthetic 1M-row Arrow generator

**Files:**
- Create: `src-tauri/src/query/synth.rs`
- Modify: `src-tauri/src/query/mod.rs` (`pub mod synth;`)
- Test: in `synth.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synth_shape_is_correct() {
        let batch = synth_batch(0, 1000, 100);
        assert_eq!(batch.num_rows(), 1000);
        assert!(batch.num_columns() >= 100);
        // schema includes the tricky types by name
        let names: Vec<_> = batch.schema().fields().iter().map(|f| f.name().clone()).collect();
        assert!(names.iter().any(|n| n == "amount_numeric"));
        assert!(names.iter().any(|n| n == "blob_text"));
        assert!(names.iter().any(|n| n == "nullable_int"));
        assert!(names.iter().any(|n| n == "created_at"));
    }
    #[test]
    fn nullable_column_has_nulls() {
        let batch = synth_batch(0, 1000, 100);
        let idx = batch.schema().index_of("nullable_int").unwrap();
        assert!(batch.column(idx).null_count() > 0);
    }
}
```

- [ ] **Step 2: Run to verify fails**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test synth:: 2>&1 | tail -15`
Expected: FAIL — `synth_batch` undefined.

- [ ] **Step 3: Implement `synth_batch`** in `src-tauri/src/query/synth.rs`

```rust
use std::sync::Arc;
use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;

/// Build a wide synthetic batch of `rows` starting at `start_id`, with at least `min_cols` columns.
/// Columns include the tricky types from the spec: NUMERIC(38,10) (Decimal128), large TEXT, NULL-heavy, timestamptz.
pub fn synth_batch(start_id: i64, rows: usize, min_cols: usize) -> RecordBatch {
    let mut fields: Vec<Field> = Vec::new();
    let mut cols: Vec<ArrayRef> = Vec::new();

    // id
    fields.push(Field::new("id", DataType::Int64, false));
    cols.push(Arc::new(Int64Array::from_iter_values((0..rows).map(|i| start_id + i as i64))));

    // amount_numeric : Decimal128(38,10)
    fields.push(Field::new("amount_numeric", DataType::Decimal128(38, 10), false));
    let dec = Decimal128Array::from_iter_values((0..rows).map(|i| (start_id + i as i64) as i128 * 1_234_500_000i128))
        .with_precision_and_scale(38, 10).unwrap();
    cols.push(Arc::new(dec));

    // created_at : Timestamp(Microsecond, "UTC")
    fields.push(Field::new("created_at", DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())), false));
    cols.push(Arc::new(TimestampMicrosecondArray::from_iter_values((0..rows).map(|i| 1_600_000_000_000_000i64 + i as i64 * 60_000_000)).with_timezone("UTC")));

    // blob_text : large TEXT (~1KB each)
    fields.push(Field::new("blob_text", DataType::Utf8, false));
    let big = "x".repeat(1024);
    cols.push(Arc::new(StringArray::from_iter_values((0..rows).map(|i| format!("{}-{}", i, big)))));

    // nullable_int : NULL-heavy (every 3rd null)
    fields.push(Field::new("nullable_int", DataType::Int64, true));
    cols.push(Arc::new(Int64Array::from_iter((0..rows).map(|i| if i % 3 == 0 { None } else { Some(i as i64) }))));

    // fill remaining columns to reach min_cols with plain int columns
    let mut c = fields.len();
    while c < min_cols {
        fields.push(Field::new(format!("c{c}"), DataType::Int64, false));
        cols.push(Arc::new(Int64Array::from_iter_values((0..rows).map(|i| (i as i64).wrapping_mul(c as i64)))));
        c += 1;
    }

    RecordBatch::try_new(Arc::new(Schema::new(fields)), cols).unwrap()
}
```

- [ ] **Step 4: Run to verify passes**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test synth:: 2>&1 | tail -15`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(core): synthetic wide 1M-row Arrow generator"
```

---

## Task 4: Windowing (Materialize) + Arrow IPC slice

**Files:**
- Create: `src-tauri/src/query/window.rs`
- Modify: `src-tauri/src/query/mod.rs` (`pub mod window;`)
- Test: in `window.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::synth::synth_batch;
    use std::sync::Arc;

    fn src() -> WindowSource { WindowSource::materialize(Arc::new(synth_batch(0, 10_000, 100))) }

    #[test]
    fn window_slice_returns_requested_rows() {
        let s = src();
        let bytes = s.window_ipc(50, 100).unwrap();
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.num_rows(), 100);
        let ids = batch.column(0).as_any().downcast_ref::<arrow::array::Int64Array>().unwrap();
        assert_eq!(ids.value(0), 50);
        assert_eq!(ids.value(99), 149);
    }

    #[test]
    fn window_past_end_is_clamped() {
        let s = src();
        let bytes = s.window_ipc(9_950, 100).unwrap(); // only 50 left
        let batch = decode_ipc(&bytes);
        assert_eq!(batch.num_rows(), 50);
    }

    #[test]
    fn ipc_roundtrip_preserves_tricky_types() {
        let s = src();
        let bytes = s.window_ipc(0, 5).unwrap();
        let batch = decode_ipc(&bytes);
        // decimal + timestamp survive
        assert_eq!(batch.schema(), synth_batch(0,5,100).schema());
    }

    // test helper
    fn decode_ipc(bytes: &[u8]) -> arrow::record_batch::RecordBatch {
        let mut r = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None).unwrap();
        r.next().unwrap().unwrap()
    }
}
```

- [ ] **Step 2: Run to verify fails**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test window:: 2>&1 | tail -15`
Expected: FAIL — `WindowSource`/`window_ipc` undefined.

- [ ] **Step 3: Implement** `src-tauri/src/query/window.rs`

```rust
use std::sync::Arc;
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use crate::drivers::DriverError;

/// A source of row windows. Materialize holds the whole batch; Cursor pages from a SqlDriver (Task 6).
pub enum WindowSource {
    Materialize(Arc<RecordBatch>),
    // Cursor variant added in Task 6
}

impl WindowSource {
    pub fn materialize(batch: Arc<RecordBatch>) -> Self { WindowSource::Materialize(batch) }

    pub fn total_rows(&self) -> usize {
        match self { WindowSource::Materialize(b) => b.num_rows() }
    }

    /// Return rows [offset, offset+len) (clamped) encoded as Arrow IPC stream bytes.
    pub fn window_ipc(&self, offset: usize, len: usize) -> Result<Vec<u8>, DriverError> {
        match self {
            WindowSource::Materialize(b) => {
                let total = b.num_rows();
                if offset >= total { return encode_ipc(&b.slice(0, 0)); }
                let take = len.min(total - offset);
                let slice = b.slice(offset, take);
                encode_ipc(&slice)
            }
        }
    }
}

pub fn encode_ipc(batch: &RecordBatch) -> Result<Vec<u8>, DriverError> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = StreamWriter::try_new(&mut buf, &batch.schema()).map_err(|e| DriverError::Db(e.to_string()))?;
        w.write(batch).map_err(|e| DriverError::Db(e.to_string()))?;
        w.finish().map_err(|e| DriverError::Db(e.to_string()))?;
    }
    Ok(buf)
}
```

- [ ] **Step 4: Run to verify passes**

Run: `source "$HOME/.cargo/env" && cd src-tauri && cargo test window:: 2>&1 | tail -15`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(core): Materialize window source + Arrow IPC slicing"
```

---

## Task 5: Docker Postgres + seed (cursor path data)

**Files:**
- Create: `docker/postgres/docker-compose.yml`, `docker/postgres/seed.sql`

- [ ] **Step 1: Write compose** `docker/postgres/docker-compose.yml`

```yaml
services:
  pg:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: tableiq
      POSTGRES_DB: tableiq
    ports: ["55432:5432"]
    volumes:
      - ./seed.sql:/docker-entrypoint-initdb.d/seed.sql:ro
```

- [ ] **Step 2: Write seed** `docker/postgres/seed.sql` (wide table; generate_series fills 1M rows)

```sql
CREATE TABLE wide (
  id            bigint primary key,
  amount_numeric numeric(38,10) not null,
  created_at    timestamptz not null,
  blob_text     text not null,
  nullable_int  bigint
);
INSERT INTO wide (id, amount_numeric, created_at, blob_text, nullable_int)
SELECT g,
       (g * 1234.5)::numeric(38,10),
       timestamptz '2020-09-13 12:26:40+00' + (g || ' minutes')::interval,
       g || '-' || repeat('x', 1024),
       CASE WHEN g % 3 = 0 THEN NULL ELSE g END
FROM generate_series(1, 1000000) AS g;
-- extra plain columns to reach 100+ cols
DO $$ BEGIN FOR i IN 6..101 LOOP EXECUTE format('ALTER TABLE wide ADD COLUMN c%s bigint DEFAULT 0', i); END LOOP; END $$;
```

- [ ] **Step 3: Bring it up and verify count**

Run:
```bash
cd /Users/rudy/dev/rudy/Table-IQ/docker/postgres && docker compose up -d
sleep 5 # initdb + 1M insert can take ~30-60s; poll instead:
until docker compose exec -T pg psql -U postgres -d tableiq -c "select count(*) from wide" 2>/dev/null | grep -q 1000000; do sleep 3; done
docker compose exec -T pg psql -U postgres -d tableiq -c "select count(*) from wide"
```
Expected: `1000000`. (Use a Monitor until-loop, not foreground sleep, per harness rules.)

- [ ] **Step 4: Commit**

```bash
git add docker && git commit -m "test: docker postgres + 1M-row wide seed for cursor path"
```

---

## Task 6: Postgres SqlDriver — keyset cursor + bounded-RAM test

**Files:**
- Modify: `src-tauri/src/drivers/postgres.rs`, `src-tauri/src/query/window.rs` (add Cursor variant)
- Test: integration test `src-tauri/tests/cursor_bounded_ram.rs` (gated on `TIQ_PG_URL`)

- [ ] **Step 1: Write the failing integration test** `src-tauri/tests/cursor_bounded_ram.rs`

```rust
// Run only when a PG URL is provided: TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq
#[tokio::test]
async fn cursor_pages_full_set_with_bounded_ram() {
    let Ok(url) = std::env::var("TIQ_PG_URL") else { eprintln!("skip: no TIQ_PG_URL"); return; };
    use table_iq::drivers::{SqlDriver, postgres::PgDriver};
    let drv = PgDriver::connect(&url).await.unwrap();
    let cur = drv.open_cursor("SELECT * FROM wide ORDER BY id", 10_000).await.unwrap();

    let mut total = 0usize;
    let mut peak_rss = 0u64;
    let mut sys = sysinfo::System::new();
    loop {
        let batch = drv.fetch_next(&cur, 10_000).await.unwrap();
        if batch.num_rows() == 0 { break; }
        total += batch.num_rows();
        sys.refresh_memory();
        peak_rss = peak_rss.max(table_iq::bench::process_rss());
    }
    drv.close_cursor(&cur).await.unwrap();
    assert_eq!(total, 1_000_000);
    // bounded: paging 1M rows must not hold them all — peak RSS under 600MB
    assert!(peak_rss < 600 * 1024 * 1024, "peak RSS too high: {peak_rss}");
}
```

- [ ] **Step 2: Run to verify fails**

Run: `source "$HOME/.cargo/env" && cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq cargo test --test cursor_bounded_ram 2>&1 | tail -20`
Expected: FAIL — `PgDriver` undefined / not compiling.

- [ ] **Step 3: Implement `PgDriver`** in `src-tauri/src/drivers/postgres.rs` (keyset pagination by `id`; converts rows → Arrow). Provide real code:

```rust
use async_trait::async_trait;
use std::sync::Mutex;
use std::collections::HashMap;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions, Row};
use arrow::array::*;
use arrow::datatypes::*;
use std::sync::Arc;
use super::{SqlDriver, CursorId, DResult, DriverError};

pub struct PgDriver { pool: Pool<Postgres>, cursors: Mutex<HashMap<String, CursorState>> }
struct CursorState { sql_base: String, last_id: i64, page: usize }

impl PgDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        let pool = PgPoolOptions::new().max_connections(4).connect(url).await
            .map_err(|e| DriverError::Db(e.to_string()))?;
        Ok(Self { pool, cursors: Mutex::new(HashMap::new()) })
    }
}

#[async_trait]
impl SqlDriver for PgDriver {
    async fn open_cursor(&self, sql: &str, page_size: usize) -> DResult<CursorId> {
        // Phase 0: keyset over the known `wide` table; store the base for paging.
        let id = format!("cur-{}", uuid_like());
        self.cursors.lock().unwrap().insert(id.clone(), CursorState { sql_base: sql.to_string(), last_id: 0, page: page_size });
        Ok(CursorId(id))
    }
    async fn fetch_next(&self, cursor: &CursorId, n: usize) -> DResult<RecordBatch> {
        let (last_id, page) = { let m = self.cursors.lock().unwrap(); let s = m.get(&cursor.0).ok_or(DriverError::Db("no cursor".into()))?; (s.last_id, s.page.max(n)) };
        let q = "SELECT id, amount_numeric::text AS amount_numeric, (extract(epoch from created_at)*1e6)::bigint AS created_us, blob_text, nullable_int FROM wide WHERE id > $1 ORDER BY id LIMIT $2";
        let rows = sqlx::query(q).bind(last_id).bind(page as i64).fetch_all(&self.pool).await.map_err(|e| DriverError::Db(e.to_string()))?;
        let batch = rows_to_batch(&rows)?;
        if let Some(max) = batch.column(0).as_any().downcast_ref::<Int64Array>().and_then(|a| if a.is_empty() { None } else { Some(a.value(a.len()-1)) }) {
            self.cursors.lock().unwrap().get_mut(&cursor.0).unwrap().last_id = max;
        }
        Ok(batch)
    }
    async fn close_cursor(&self, cursor: &CursorId) -> DResult<()> { self.cursors.lock().unwrap().remove(&cursor.0); Ok(()) }
}

fn uuid_like() -> String { format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()) }

fn rows_to_batch(rows: &[sqlx::postgres::PgRow]) -> DResult<RecordBatch> {
    let n = rows.len();
    let ids = Int64Array::from_iter_values(rows.iter().map(|r| r.get::<i64,_>("id")));
    let amounts = StringArray::from_iter_values(rows.iter().map(|r| r.get::<String,_>("amount_numeric")));
    let created = TimestampMicrosecondArray::from_iter_values(rows.iter().map(|r| r.get::<i64,_>("created_us"))).with_timezone("UTC");
    let blob = StringArray::from_iter_values(rows.iter().map(|r| r.get::<String,_>("blob_text")));
    let nint = Int64Array::from_iter(rows.iter().map(|r| r.get::<Option<i64>,_>("nullable_int")));
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("amount_numeric", DataType::Utf8, false), // exact text (spec #3 — no float round-trip)
        Field::new("created_at", DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())), false),
        Field::new("blob_text", DataType::Utf8, false),
        Field::new("nullable_int", DataType::Int64, true),
    ]);
    let _ = n;
    RecordBatch::try_new(Arc::new(schema), vec![Arc::new(ids), Arc::new(amounts), Arc::new(created), Arc::new(blob), Arc::new(nint)])
        .map_err(|e| DriverError::Db(e.to_string()))
}
```

Add `uuid`-free id (above) to avoid an extra dep. Add `WindowSource::Cursor` in `window.rs` that calls `fetch_next` and IPC-encodes each page. Make crate name available to integration test: add `name = "table_iq"` under `[lib]` in Cargo.toml and create `src-tauri/src/lib.rs` re-exporting `pub mod drivers; pub mod query; pub mod bench;` (and have `main.rs` use the lib).

- [ ] **Step 4: Implement `bench::process_rss`** in `src-tauri/src/bench/mod.rs`

```rust
pub fn process_rss() -> u64 {
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().unwrap();
    sys.refresh_process(pid);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0) // bytes
}
```

- [ ] **Step 5: Run to verify passes** (PG must be up from Task 5)

Run: `source "$HOME/.cargo/env" && cd src-tauri && TIQ_PG_URL=postgres://postgres:tableiq@127.0.0.1:55432/tableiq cargo test --test cursor_bounded_ram -- --nocapture 2>&1 | tail -20`
Expected: PASS — total 1,000,000, peak RSS under cap.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(core): postgres keyset cursor driver + bounded-RAM integration test"
```

---

## Task 7: IPC transports + Tauri commands

**Files:**
- Create: `src-tauri/src/ipc/transport.rs`
- Modify: `src-tauri/src/main.rs` (AppState holds the two `WindowSource`s; register commands + protocol)

- [ ] **Step 1: Implement AppState + commands** in `src-tauri/src/main.rs`

```rust
// AppState: a Materialize source (synth 1M) + optional Cursor source (PG).
struct AppState { materialize: std::sync::Arc<crate::query::window::WindowSource> }

#[tauri::command]
fn get_window_response(state: tauri::State<AppState>, offset: usize, len: usize) -> tauri::ipc::Response {
    let bytes = state.materialize.window_ipc(offset, len).unwrap_or_default();
    tauri::ipc::Response::new(bytes) // raw bytes, no base64
}

#[tauri::command]
async fn get_window_channel(state: tauri::State<'_, AppState>, offset: usize, len: usize, on_window: tauri::ipc::Channel<Vec<u8>>) -> Result<(), String> {
    let bytes = state.materialize.window_ipc(offset, len).map_err(|e| e.to_string())?;
    on_window.send(bytes).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Register custom protocol** in the `tauri::Builder` (in `main.rs` `run()`):

```rust
.register_asynchronous_uri_scheme_protocol("tiqwin", |ctx, req, responder| {
    // URL: tiqwin://window/<offset>/<len>
    let path = req.uri().path().trim_start_matches('/');
    let parts: Vec<_> = path.split('/').collect();
    let (offset, len) = (parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0usize),
                         parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0usize));
    let state = ctx.app_handle().state::<AppState>();
    let bytes = state.materialize.window_ipc(offset, len).unwrap_or_default();
    responder.respond(http::Response::builder().header("content-type","application/vnd.apache.arrow.stream").body(bytes).unwrap());
})
```

(Add `http` dep; build AppState in `.setup()` with `synth_batch` over 1M rows wrapped in `WindowSource::materialize`. Register `get_window_response`, `get_window_channel` in `invoke_handler`.)

- [ ] **Step 3: Verify it builds + app launches**

Run: `source "$HOME/.cargo/env" && cd /Users/rudy/dev/rudy/Table-IQ && pnpm tauri build --debug 2>&1 | tail -20` (or `pnpm tauri dev` for a launch smoke test).
Expected: compiles; window opens.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(ipc): three window transports (response/channel/custom protocol)"
```

---

## Task 8: Frontend Arrow window decode + cache (vitest)

**Files:**
- Create: `src/lib/arrowWindow.ts`, `src/lib/arrowWindow.test.ts`
- Modify: `package.json` (add `vitest`), `vite.config.ts` (test config)

- [ ] **Step 1: Add vitest**: `pnpm add -D vitest` and a `"test": "vitest run"` script.

- [ ] **Step 2: Write the failing test** `src/lib/arrowWindow.test.ts`

```ts
import { describe, it, expect } from "vitest";
import { tableFromIPC } from "apache-arrow";
import { decodeWindow } from "./arrowWindow";

// Build a tiny Arrow IPC buffer in-test via apache-arrow to assert decode works.
import { tableToIPC, makeTable, vectorFromArray, Int64 } from "apache-arrow";

describe("decodeWindow", () => {
  it("decodes ids from an Arrow IPC stream buffer", () => {
    const table = makeTable({ id: vectorFromArray([10n, 11n, 12n], new Int64()) });
    const buf = tableToIPC(table, "stream");
    const w = decodeWindow(buf);
    expect(w.numRows).toBe(3);
    expect(w.getCell(0, "id")).toBe("10");
    expect(w.getCell(2, "id")).toBe("12");
  });
});
```

- [ ] **Step 3: Run to verify fails**

Run: `pnpm test 2>&1 | tail -20`
Expected: FAIL — `decodeWindow` not found.

- [ ] **Step 4: Implement** `src/lib/arrowWindow.ts`

```ts
import { tableFromIPC, Table } from "apache-arrow";

export interface DecodedWindow {
  numRows: number;
  columns: string[];
  getCell(row: number, col: string): string; // stringified for the grid; null -> ""
}

export function decodeWindow(buf: Uint8Array): DecodedWindow {
  const table: Table = tableFromIPC(buf);
  const columns = table.schema.fields.map((f) => f.name);
  return {
    numRows: table.numRows,
    columns,
    getCell(row, col) {
      const v = table.getChild(col)?.get(row);
      return v == null ? "" : String(v);
    },
  };
}
```

- [ ] **Step 5: Run to verify passes**

Run: `pnpm test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Add a window cache + prefetch** (extend `arrowWindow.ts`): a `WindowCache` keyed by page index that keeps at most K pages (LRU) and exposes `ensure(offset,len)`. Add a vitest asserting it evicts beyond K. Commit.

```bash
git add -A && git commit -m "feat(ui): arrow-js window decode + LRU window cache"
```

---

## Task 9: Transport client + Zustand store

**Files:**
- Create: `src/lib/transport.ts`, `src/state/benchStore.ts`

- [ ] **Step 1: Implement transport client** `src/lib/transport.ts` (times each round-trip; returns bytes)

```ts
import { invoke, Channel } from "@tauri-apps/api/core";

export type TransportKind = "response" | "channel" | "protocol";

export async function fetchWindow(kind: TransportKind, offset: number, len: number): Promise<{ bytes: Uint8Array; ms: number }> {
  const t0 = performance.now();
  let bytes: Uint8Array;
  if (kind === "response") {
    const ab = await invoke<ArrayBuffer>("get_window_response", { offset, len });
    bytes = new Uint8Array(ab);
  } else if (kind === "channel") {
    bytes = await new Promise<Uint8Array>((resolve, reject) => {
      const ch = new Channel<number[] | Uint8Array>();
      ch.onmessage = (m) => resolve(m instanceof Uint8Array ? m : new Uint8Array(m));
      invoke("get_window_channel", { offset, len, onWindow: ch }).catch(reject);
    });
  } else {
    const res = await fetch(`tiqwin://window/${offset}/${len}`);
    bytes = new Uint8Array(await res.arrayBuffer());
  }
  return { bytes, ms: performance.now() - t0 };
}
```

- [ ] **Step 2: Implement Zustand store** `src/state/benchStore.ts` (selected transport/path, metric arrays, p99 helper). Commit.

```bash
git add -A && git commit -m "feat(ui): transport client (3 kinds) + bench Zustand store"
```

---

## Task 10: DataGrid with windowed source

**Files:**
- Create: `src/components/DataGrid.tsx`

- [ ] **Step 1: Implement** `DataGrid.tsx` using `@glideapps/glide-data-grid`: a `DataEditor` with `rows={totalRows}`, columns from the first decoded window, and `getCellContent([col,row])` that reads the cache; on cache miss it triggers `fetchWindow` for the page containing `row` and returns a loading cell until it arrives (glide re-requests). Import glide CSS. Provide the full component code (≈70 lines).

- [ ] **Step 2: Smoke test in dev** — `pnpm tauri dev`, confirm the grid renders and scrolls and cells fill in. Commit.

```bash
git add -A && git commit -m "feat(ui): glide-data-grid windowed data source"
```

---

## Task 11: Bench harness (scripted scroll + metrics)

**Files:**
- Create: `src/components/BenchView.tsx`

- [ ] **Step 1: Implement** `BenchView.tsx`:
  - Controls: pick path (materialize/cursor) × transport (response/channel/protocol), "Run".
  - On Run: programmatically scroll the grid from row 0 → 1,000,000 over a fixed number of animation frames; record fps via `requestAnimationFrame` deltas (fps = 1000/avgDelta), record JS heap via `performance.memory?.usedJSHeapSize` (note fallback if undefined), record per-window-fetch latency (from `transport.ts`), compute p99.
  - Render a results table (path | transport | fps | heap MB | fetch p99 ms) and `Download bench.json`.
  - Document in a comment that fps is measured under a fixed-cadence scripted scroll.

- [ ] **Step 2: Wire into `App.tsx`** (tab between Grid and Bench). `pnpm tauri dev` and run each combination; save `bench/bench.json`. Commit.

```bash
git add -A && git commit -m "feat(bench): scripted-scroll fps/heap/p99 harness + results table"
```

---

## Task 12: Run the gate + record results

- [ ] **Step 1:** Ensure Docker PG is up (Task 5). Launch `pnpm tauri dev`.
- [ ] **Step 2:** Run the bench for all 6 combinations (2 paths × 3 transports). Export `bench/bench.json`.
- [ ] **Step 3:** Record measured numbers vs the gate (60/120fps, heap < N, p99 < 16ms) in `docs/finalize/` (the Finalize phase will format this). Pick the winning transport.
- [ ] **Step 4:** Commit any config tweaks: `git commit -am "chore(bench): record Phase 0 gate results"`.

---

## Self-Review (completed during writing)

- **Spec coverage:** scaffold (T1), three traits + shared types (T2), materialize path (T3-T4), cursor path + bounded RAM (T5-T6), three transports (T7), arrow-js decode + cache (T8), transport client + store (T9), glide grid (T10), fps/heap/p99 harness (T11), gate run (T12). All Phase 0 spec items mapped.
- **Type consistency:** `FetchWindow`/`RowKey`/`KeyBy`/`RawCell`/`CursorId`/`SqlDriver`/`WindowSource`/`window_ipc`/`encode_ipc`/`decodeWindow`/`fetchWindow` used consistently across tasks.
- **Known caveats carried from spec:** `performance.memory` may be unavailable in WKWebView → fall back to Rust-side `bench::process_rss` reported via a command; document the method in the report. Decimal/timestamp are carried as exact text/typed Arrow to avoid float round-trip (spec #3).
- **Risk:** custom protocol (`tiqwin`) wiring is the fiddliest; if it underperforms or is complex, record and prefer response/channel.
```
