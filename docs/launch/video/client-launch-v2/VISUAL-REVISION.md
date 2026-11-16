# Visual revision — November 16, 2026

Source-only revision after feedback requesting fewer screenshots, better transitions, and zooms around actual clicks. No new delivery render, narration, music, captions, or native AI success is claimed by this note.

## Changes

- `produce-films.py` is the source of truth. It regenerates the three treatments, their scene HTML, shared styles, film plans and storyboards.
- Hook/problem/brand/BYOK scenes are now editorial typography. The story opener stages “Paid.”, “Still unshipped.” and its question against the existing narration. The brand lockup is larger and does not carry a redundant tab screenshot. BYOK uses a large typographic graphic, a plain-language expansion, and the optional-AI/provider-charge qualifier.
- Only workflow, native result and MCP scenes contain product screens: 22/56 seconds in motion, 21/57.5 in story, and 22.5/57 in walkthrough. The SQL scene retains exact, clearly labelled editorial excerpts from the verified native output.
- Directional type handoffs connect hook → problem → brand and BYOK → SQL. Their outgoing movement accelerates and the incoming movement settles in the same direction. Proof shots use direct cuts with the evidence visible immediately; the old repeated panel wipes are removed. Registry `text-match-cut` was inspected as a technique reference; its unused installed source was removed.
- Product panels are wider, square-edged and keep the original UI colors. `prepare-captures.py` now exposes `prepare_focus_captures()`, which regenerates only browse/MCP visual media independently of audio. Both assets have one consistent wide source crop with no baked crop cuts. The production manifest records selected ranges, dimensions, action-reference windows and new SHA-256 hashes. These files are copied into each editable project's assets by the generator.
- Workflow zooms start after the real order selection (local 0.12s, 1.40×), hold, and return (1.35–1.67s). The camera focuses the Structure state (1.8s, 1.24×), holds, and returns (2.95–3.25s). It reframes the actual schema opening at 3.4s, then smoothly focuses payments/shipments/orders at 4.05–4.65s (1.55×), holds, and returns to the wider schema in the final 1.1s. Structure/schema timings are observed source-state windows rather than nonexistent subframe click telemetry. No cursor/click effects are simulated.
- MCP focus starts at local 1.6s, matching the recorded Show full SQL expansion: source `expanded=2.959s`, selected source starts at `1.359s`. It settles at 1.35× over 0.55s, holds the tool names/SQL/status region, and returns to context in the final 1.1s. Client/time columns move outside the focus crop deliberately; the entire log returns at the end.
- Result labels explicitly identify native v0.8.17 still evidence. Rows 102/104, values 4200/3100 and total 7,300 cents are unchanged. No typing, generation stream, Run click, or result population was fabricated.

## Verification

All three final `hyperframes check --json` reports are in each project's `review/revision-check.json`: `ok=true`, `browserSkipped=false`, zero errors, warnings or info findings across lint/runtime/layout/contrast/craft. Motion sidecar assertions are not enabled; camera motion was separately inspected with `hyperframes keyframes` and snapshots.

Scene ID/start/duration/voice tuples and every audio HTML tag were compared with Git HEAD and match exactly. Durations remain motion 56s, story 57.5s, walkthrough 57s. Generator SQL substring assertions passed. Existing captions and audio files were not edited.

Final `snapshot --review` completed for all three projects into each `review/revision-v3/`: fresh beat, 360px phone and seam sheets (motion 58 frames/12 sheets; story 51 frames/9 sheets; walkthrough 58 frames/12 sheets). These are the final overview sets for independent review.

Inspected frames include all scene types, story narration stages, exact result rows and final CTA. Final motion review has 36 dense camera samples in `motion/review/continuous-focus` and four eight-frame scene-boundary sequences (32 frames total) in `motion/review/eight-frame-seams`. Narrated treatments have additional focus/hold/return frames in `{story,walkthrough}/review/continuous-focus-final`. These supersede the earlier product framing in `review/revision-beats` and `review/revision-motion`. Final BYOK timing/native-result labels are in motion `review/revision-final` and story `review/revision-byok-final`.

`hyperframes keyframes` also reported the authored camera trace. Its `revision-camera-strip.png` is an earlier bounding-box diagnostic, not final pixel evidence; the snapshot sequences above are the final evidence.

## Limits

Browse and MCP now preserve continuous selected source ranges; each holds its genuine final frame to fill the existing scene slot. The overall film still uses separate recorded/evidence scenes and does not assert a continuous end-to-end session. MCP and schema detail still need larger playback than a phone-width contact sheet. Hook/brand/BYOK are intentionally editorial graphics and demonstrate no new app interaction. Delivery encoding, audio listening, final-frame encode checks and release packaging remain with the supervising task.

## Delivery follow-through

All three v3 MP4s were rendered after the source review. Root corrected the hook's supporting line to “A tab limit should not interrupt an investigation.” and rechecked/resampled both affected treatments. Final encoded frames and stream/audio/hash evidence are in `delivery/technical-verification-v3.json` and `delivery/v3-*-encoded-review.jpg`. The actual Argo export has zero narration overflow. Sound was measured, not heard. Website header/dialog and the current gallery were checked separately; the guided walkthrough is the primary website demo.
