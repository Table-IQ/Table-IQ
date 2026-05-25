# Finalize — Slice 18: AI breadth (explain / optimize / fix-error)

**Branch:** `slice-18-ai` → merged to `main`. **Date:** 2026-05-25.

## Task-done summary
Three AI actions over the editor SQL, alongside the existing NL→SQL generate:
- **Explain** — plain-English explanation (shown in a dismissible panel).
- **Optimize** — equivalent optimized rewrite (inserts SQL into the editor for review).
- **Fix** — corrects the last failing query using its error message (enabled only when there's an error; inserts SQL).

All keep the generate-not-execute safety (output goes into the editor; the user runs it). Reuses the BYOK/Ollama provider, keychain key, and schema-card egress.

## Files
- Changed: `src-tauri/src/ai/mod.rs` (`build_explain_prompt`/`build_optimize_prompt`/`build_fix_prompt` + systems), `src-tauri/src/ipc/connect.rs` (`ai_setup` helper + `ai_explain_sql`/`ai_optimize_sql`/`ai_fix_sql`), `lib.rs`, `src/lib/commands.ts` (wrappers + mocks), `src/components/QueryView.tsx` (Explain/Optimize/Fix buttons + explanation panel).

## Verification (checked thoroughly)
- **Rust unit (deterministic):** `cargo test --lib ai::` → **4 passed** — the three prompt builders carry the schema card, the SQL, and (for fix) the error message; optimize/fix systems request bare SQL.
- **Frontend:** `npm test` → 57 passed; `npm run build` green (tsc verifies the new actions + panel).
- Optimize/Fix run output through `extract_sql` (strips fences); Explain returns prose.

## Honest gating
- **Live answer quality is not verified** — needs prerequisite **P3** (a BYOK key in AI Settings, or a running Ollama). The commands, prompts, egress preview, and UI are complete and work against the browser-mock; real-model quality is an eval task once a key/Ollama is provided.
- **Chat panel** and **agentic self-verifying AI** (pending item C) remain follow-ups — this slice delivers the one-shot explain/optimize/fix actions.
