# TableIQ — three client-first launch films

The approved audience-focused 75-second revision is delivered as three 1920×1080, 30 fps H.264/AAC films. [Compare all three](https://tableiq.ideolyst.com/launch-films/).

| Film | Duration | Treatment |
| --- | --- | --- |
| [Motion](https://tableiq.ideolyst.com/video/tableiq-launch-motion-v4.mp4) | 75s | Music and concise headings; tab-limit hook |
| [Story](https://tableiq.ideolyst.com/video/tableiq-launch-story-v4.mp4) | 75s | Narrated paid-but-unshipped investigation |
| [Walkthrough](https://tableiq.ideolyst.com/video/tableiq-launch-walkthrough-v4.mp4) | 75s | Narrated workflow; official Argo alignment/export |

Every film leads with a free, open-source Mac database client, then optional BYOK AI and visible MCP calls. The light brand canvas alternates editorial typography with selected actual app proof. Recorded clicks lead to a focused zoom, readable hold and return to context. Browse/MCP clips preserve continuous source views without baked crop cuts. Rust is a supporting fact; no relative speed claim is made. Mac Apple silicon developer preview, unsigned/not notarized; Windows and Linux coming soon.

Huashu Motion and Take One were requested creative references, but their generators were not connected. The motion and story treatments use editable HyperFrames compositions. The third film actually uses the pinned official Argo package for recording, import, narration alignment and export; it is not merely labelled Argo.

The original v2 and v3 cuts stay available at their existing URLs. The current gallery and source package use revision v4. The [approved script and audience evidence](APPROVED-SCRIPT-v4.md) define the seven exact slots. [Visual revision notes](VISUAL-REVISION.md) record the exact camera timings.

## What the viewer sees

Real development-build React UI talks to the real Rust command handlers and a disposable synthetic SQLite database. These are not browser mocks. The source recordings include multi-tab, table, schema, AI settings and MCP interactions recorded through Argo. The revised films use multi-tab, table/schema, manual SQL/Run and MCP recordings. Hook, offer, BYOK and CTA use editorial typography between product proof. The native AI result comes from the separately verified v0.8.17 case. The primary investigation now shows real SQL entry, manual Run and two returned orders through the actual development interface and Rust backend. Its Arrow result was decoded and checked. The optional AI segment uses the previously verified native example, prominently labelled as a still. No simulated generation, click, model output or refusal row is presented as live. The MCP clip is a real local protocol client in a separate session, not a newly recorded coding-agent conversation.

The attempted new native AI recording waited on macOS Keychain access. Its failed/spinning section is excluded. One earlier successful Gemini case supports the displayed result, not a general model accuracy claim. See [provenance](PROVENANCE.md), [review](REVIEW.md), [technical evidence](delivery/technical-verification-v4.json) and [native acceptance report](../../research/2026-11-16-native-ai-acceptance.md).

## Edit and reproduce

Each of `motion/`, `story/` and `walkthrough/` is a separate editable HyperFrames project with its own scenes, shot plan and assets. The source ZIP on the [preview release](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17) includes the media omitted from Git. Node 22+, FFmpeg, Python 3 and a Chromium-compatible browser are required.

From a film directory:

```sh
npx hyperframes@0.9.56 preview --background
npx hyperframes@0.9.56 check --json
npx hyperframes@0.9.56 snapshot --review
npx hyperframes@0.9.56 render --quality delivery -o ../renders/film-unmastered.mp4
```

The HyperFrames desktop app can edit these same sources. Stop a background preview before opening that project in the desktop app. Render only after inspecting the changed frames. For motion/story audio, run from this parent directory:

```sh
python3 master-audio.py renders/film-unmastered.mp4 renders/film-final.mp4
python3 build-captions.py --version v4
```

For the Argo walkthrough, render its picture first, then from `walkthrough/`:

```sh
npm ci --ignore-scripts
node finish-argo.mjs ../renders/tableiq-walkthrough-picture.mp4 v4
```

The result is `walkthrough/videos/tableiq-walkthrough-v4.mp4`. The official Argo export replaces picture audio with aligned narration and music. The final v4 delivery is then mastered to48kHz stereo with `python3 master-audio.py walkthrough/videos/tableiq-walkthrough-v4.mp4 renders/tableiq-launch-walkthrough-v4.mp4 --true-peak -3`, allowing AAC encoding headroom. `argo-placements.json` records actual timing and must show zero overflow. The parent caption script uses those same scene offsets.

`produce-films.py` regenerates all compositions and overwrites manual scene edits. `prepare-captures.py` needs the original local recording workspace; the portable source already contains curated, key-free assets and does not need those raw recordings to re-render. The original historical [script](SCRIPT.md), [nine-frame board](boards/storyboard.html) and [storyboard review](boards/STORYBOARD-REVIEW.md) remain historical planning artifacts.

## v4 production inputs

`generate-v4-voice.mjs` uses the existing pinned Argo/Kokoro dependency and phonetic spellings for brand/acronyms. `production-media/voices/transcripts-v4.json` records measured local word timings; caption spelling follows the approved script. The 75-second score reuses the original TableIQ instrumental: 0–48s plus16–45s, with a2s crossfade and71–75s fade. No reference-video soundtrack was copied. Audio is measured/transcribed, not independently listened to.

`prepare-manual-capture.py` preserves the continuous crop of the successful actual UI recording and holds its genuine last frame. `production-media/manual-query.sql` is the formatted manual query; the original native AI output remains separate. `manual-query-proof.json` records the synthetic case, expected rows and actual action marks. The full source ZIP includes the curated media needed to re-render without private local app state or raw failed recordings.

The final walkthrough preserves the reviewed HyperFrames picture stream after official Argo import/alignment/export. A decoded-frame comparison caught a color shift in Argo’s intermediate picture transcode; retaining the original picture restores the same brand colours as the other cuts while keeping Argo’s aligned/exported sound. This final remux does not invent product frames or change timing.
