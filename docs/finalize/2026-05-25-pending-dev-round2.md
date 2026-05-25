# Consolidated report — "do the pending dev" (slices 15–19)

**Date:** 2026-05-25. **Branch:** all merged to `main`. Guided by repo-root `pending.md`.

## What shipped this round
| Slice | Item (pending.md ref) | Evidence |
|---|---|---|
| 15 | MSSQL inline edit — item A | live azure-sql-edge: edit round-trip (1 row) + stale-value conflict (0 rows) |
| 16 | **WASM plugin system (Extism)** — item D | sandboxed wasmtime host + ABI + embedded markdown-export sample (wasm32); 3 host tests |
| 17 | Result-set diff picker + ALTER-table UI — item B | `diff_results` picker + `AlterTablePanel`; +3 frontend tests |
| 18 | AI explain / optimize / fix — item C | 3 commands + prompts (unit-tested) + buttons; generate-not-execute |
| 19 | MSSQL windowed cursor — item A | live: page 3 rows at 2/page → 2,1,0; empty-batch guard |

## Final verification (whole repo, `main`)
- **Rust:** `cargo test` across all 6 engines + plugins → **86 passed, 0 failed**, clean build, no warnings.
- **Frontend:** `npm test` → **57 passed** (15 files); `npm run build` green.
- Working tree clean.

## pending.md status now
- ✅ Done: WASM plugins (D), result-diff + ALTER UIs (B), MSSQL inline edit + cursor (A), AI explain/optimize/fix (C partial).
- Prerequisite **P5 satisfied** (`wasm32-unknown-unknown` installed).

## Still remaining (and why)
| Item | Blocker |
|---|---|
| Signed macOS/Windows builds | **P1/P2** — Apple Developer ID + Windows code-signing certs (CI runners already in matrix) |
| Live AI answer quality | **P3** — a BYOK key or running Ollama (commands/UI all built) |
| AI chat panel + agentic self-verify | P3 (build is straightforward; quality needs P3) |
| Bundled llama.cpp sidecar | **P4** — model/weights + licensing decision |
| MSSQL cancellation | needs a connection pool first (tiberius exposes no TDS Attention; `KILL` severs the pinned conn) |
| MSSQL connection pool | design (deadpool-tiberius) — no external blocker |
| ALTER rename / type-change | extra backend (`RenameColumn`/`AlterColumnType`) — route via SQL editor for now |
| `/browse` QA of slices 5–19 | P6 for real-DB; can run against browser-mock now |

## Process note
Subagent dispatch was still unreliable, so all slices were implemented inline with TDD (RED→GREEN) and verified against the live containers where applicable.
