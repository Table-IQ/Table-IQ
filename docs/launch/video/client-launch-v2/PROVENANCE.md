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

## v4 audience-focused production — 16 November 2026

The [approved seven-slot, 75-second script](APPROVED-SCRIPT-v4.md) supersedes the earlier narration. Its primary audience is Mac developers blocked by TablePlus’s two-tab free limit, with an invitation to DBeaver users to compare an ordinary task. Neither competitor is presented as universally worse. No comparative speed, RAM, accuracy or unique-AI claim is made.

New `capture-manual-v4` footage uses the actual TableIQ0.8.17 development interface, the existing Rust IPC harness and a disposable synthetic SQLite database. SQL is inserted through real browser keyboard input, then the actual Run button is clicked. Marks: input0.806s, queryready1.780s, Run5.105s, resultready5.734s. The actual Rust `run_query` metadata reports two rows and no error; the `get_result_window` Arrow response decodes exactly to `(102, Northstar Demo,4200)` and `(104, Acme Demo,3100)`. The footage is labelled development capture. It is not a native-window recording and does not imply a new AI request.

The source range0–6.066667s is retained with one continuous crop `[430,110,2110,800]`, resized to1920×728. Its genuine final two-row frame holds to16s. Later navigation/reload frames are excluded. `prepare-manual-capture.py` makes this derivative; `manual-query.sql` is the formatted manual input, separate from the preserved native AI output. No placeholder result is composited over the app.

The optional AI segment uses the existing labelled still from the single passed native0.8.17 case. The MCP segment remains the genuine, separately recorded local-protocol-client session. New native-window attempts exposed no usable automation window; no failed attempt is substituted as successful proof.

Narration uses the pinned local Argo/Kokoro q8 CPU engine, af_heart at1.05, with phonetic brand/acronym inputs. Local Whisper word timings are saved in `production-media/voices/transcripts-v4.json`; captions use the approved spelling. The original TableIQ instrumental is extended to75s from source0–48s plus16–45s with a2s crossfade and final71–75s fade. The reference film's audio is not reused. Audio is measured and transcribed, not independently listened to.

All three treatments retain editable HyperFrames scenes. Motion is music-only; story has narration; walkthrough passes through official Argo0.42.1 import, alignment and export with an overflow assertion. These are implementations of the requested creative treatments, not claims that Huashu Motion or Take One services were connected.
