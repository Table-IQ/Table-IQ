# Delivery provenance — visual revision v3, November 16, 2026

## Current visual revision

V3 replaces hook, problem, brand and BYOK screenshot scenes with editorial typography. Only workflow, result and MCP scenes contain screens; AI query excerpts remain plainly editorial. The curated browse/MCP assets are continuous selected source ranges with consistent framing. Camera focus/hold/return is keyed to recorded UI control/state changes, not fabricated subframe click telemetry. Exact source ranges, observed event windows and SHA-256 hashes are in the updated manifest. V2 assets remain historical and keep their public URLs.

## Real product material

- `production-media/manifest.json` records selected source ranges, framing crops, output dimensions, durations, actual/still classification and SHA-256 hashes.
- Client/settings recordings use TableIQ source at the then-current public main, real Rust IPC handlers and local synthetic SQLite data. Argo 0.42.1 records real interactions in the development webview. No UI/backend response is mocked. The selected client range ends before the blocked new AI request.
- MCP Activity records `list_connections`, `list_tables`, `describe_table` and a bounded `run_query` from a local protocol client against a read-only connection. It returns synthetic orders 102/104. `mcp-calls.json` excludes credentials/headers. A local protocol client is not evidence of a newly recorded LLM conversation.
- `assets/ai-generated.png` and `assets/ai-result.png` are genuine native v0.8.17 before/after images from the passed Gemini case. The SQL excerpts in the films match `production-media/native-generated.sql`. The source preserves the original stills. Result 102/Northstar Demo/4200 and 104/Acme Demo/3100 totals 7,300 cents. No production/customer data.
- A new native AI recording attempt blocked at macOS Keychain access. Its spinner/failure is excluded. These films do not assert a new successful live generation, continuous recording, general accuracy, latency or comparative performance.
- No Keychain ACL bypass, copied private credential, fabricated refusal row or fake agent activity is part of this delivery. Visible provider configuration has an empty stored-key placeholder. MCP tokens/AI keys are absent from the portable source.

## Tools and audio

- Motion/story: HyperFrames 0.9.56, editable HTML/GSAP sub-compositions. Actual Huashu Motion and Take One generators were unavailable; these are corresponding creative treatments made with HyperFrames.
- Walkthrough: official `@argo-video/cli` 0.42.1, source [shreyaskarnik/argo](https://github.com/shreyaskarnik/argo), inspected commit `5f715496522c4e682f4dc4aeffae5528e866ad5e`. Actual recording/import/cache/align/export APIs are used. Its lockfile pins the exact dependencies. Narration overflow is zero.
- Narration: locally generated Kokoro82M q8 CPU, `af_heart`, speed 1.05, through `kokoro-js` 1.2.1. No voice cloning, paid voice service or account signup. [Model card](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX) identifies Apache-2.0. Measured local Whisper word times inform captions; brand/acronym spelling and homophones are corrected against the script.
- Score: reused original local TableIQ launch score from the previously delivered 56-second motion film. Its local speech transcription contains only “upbeat music”; no old spoken narration is layered into the new films. Beat analysis indicates 120 BPM. No LinkedIn reference video/music is redistributed. Fade at 53–56 seconds with a silent CTA tail on the longer versions.
- Sound is measured and locally transcribed, not independently heard or human-rated. Exact current encoded levels are recorded in `delivery/technical-verification-v3.json`; the original v2 report is retained separately.

## Brand and notices

TableIQ wordmark/colors are supplied project assets. Instrument Sans and IBM Plex Mono include their SIL OFL notices. Relevant project/Argo license texts accompany the source package. The product stays AGPL-3.0-only; media production does not change app licensing. No new endorsement, comparative benchmark or user-count claim is implied.
