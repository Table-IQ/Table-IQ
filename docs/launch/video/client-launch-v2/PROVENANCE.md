# Current launch-film provenance

The [approved 75-second script](APPROVED-SCRIPT-v4.md) targets Mac backend/full-stack developers doing ordinary database work. It opens on TablePlus’s documented free-tab limit, invites DBeaver users to try a familiar workflow, then covers optional BYOK AI and MCP. Neither AI/MCP nor free/open-source licensing is presented as unique against every competitor. No speed, memory or general accuracy comparison is claimed.

## Product footage

Argo recordings show the actual TableIQ 0.8.17 development interface talking to real Rust command handlers and a disposable synthetic SQLite database. The manual investigation enters the SQL in `production-media/manual-query.sql`, clicks the real Run button, and verifies the actual Arrow result: 102/Northstar Demo/4200 and 104/Acme Demo/3100. Input starts 0.806s, query ready 1.780s, Run 5.105s, result ready 5.734s relative to that capture. The continuous source range 0–6.066667s is cropped and its genuine final result frame held to 16s. Later reload frames are excluded. The development capture and edited time are labelled. `manual-query-proof.json` records the decoded result and source hash.

The optional AI segment is a labelled native 0.8.17 still from [one passed Gemini example](../../research/2026-11-16-native-ai-acceptance.md). It is not a new live model call or a general accuracy rate. The MCP footage shows real local-protocol-client calls in a separate sample session; it does not invent a coding-agent conversation or blocked-write event. `production-media/manifest.json` and `mcp-calls.json` preserve the capture references.

## v5 sound

The founder chose soft, very quiet music with clear narration. Original local Kokoro af_heart narration and approved words are preserved, processed with pitch-preserving0.96 tempo and gentle tonal softening. Current utterances target−17 LUFS. The original TableIQ instrumental is filtered and separately levelled to −36 LUFS, with 6 dB additional attenuation around speech; the motion-only cut targets−30 LUFS. No reference-video soundtrack is used. `prepare-audio-v5.py` is the reproducible recipe and `production-media/audio-v5/mix-plan.json` records settings. Captions follow the original measured word times scaled for the new pace. Audio is measured and compared, not independently listened to.

## Composition and delivery

The approved v4 picture payload is preserved exactly in all three v5 files. HyperFrames supplies the editable visual compositions. The guided film also uses official Argo 0.42.1 recording/import/alignment/export; alignment must report zero overflow. The final picture stream is retained after Argo export to avoid an intermediate color shift. Argo’s aligned sound is muxed without a final loudness boost. Huashu Motion and Take One remain creative references; those services were not connected.

Historical drafts, duplicate backups and old review renders were removed from the active repository. Earlier source packages remain on the GitHub release and in Git history. Older public media URLs redirect to archived originals, preserving their bytes. Current evidence is in [technical-verification-v5.json](delivery/technical-verification-v5.json).
