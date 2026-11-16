# TableIQ — three client-first launch films

The approved storyboard is now delivered as three 1920×1080, 30 fps H.264/AAC films. [Compare all three](https://tableiq.ideolyst.com/launch-films/).

| Film | Duration | Treatment |
| --- | --- | --- |
| [Motion](https://tableiq.ideolyst.com/video/tableiq-launch-motion-v2.mp4) | 56s | Music and concise headings; tab-limit hook |
| [Story](https://tableiq.ideolyst.com/video/tableiq-launch-story-v2.mp4) | 57.5s | Narrated paid-but-unshipped investigation |
| [Walkthrough](https://tableiq.ideolyst.com/video/tableiq-launch-walkthrough-v2.mp4) | 57s | Narrated workflow; official Argo alignment/export |

Every film leads with a free, open-source Mac database client, then optional BYOK AI and visible MCP calls. The light brand canvas surrounds the actual dark app. Rust is a supporting fact; no relative speed claim is made. Mac Apple silicon developer preview, unsigned/not notarized; Windows and Linux coming soon.

Huashu Motion and Take One were requested creative references, but their generators were not connected. The motion and story treatments use editable HyperFrames compositions. The third film actually uses the pinned official Argo package for recording, import, narration alignment and export; it is not merely labelled Argo.

## What the viewer sees

Real development-build React UI talks to the real Rust command handlers and a disposable synthetic SQLite database. These are not browser mocks. Current multi-tab, table, schema, AI settings and MCP footage was recorded through Argo. The native AI result comes from the separately verified v0.8.17 case. Query scenes are plainly labelled editorial excerpts transcribed verbatim from that output; result images are prominently labelled stills. No simulated generation, click, model output or refusal row is presented as live. The MCP clip is a real local protocol client in a separate session, not a newly recorded coding-agent conversation.

The attempted new native AI recording waited on macOS Keychain access. Its failed/spinning section is excluded. One earlier successful Gemini case supports the displayed result, not a general model accuracy claim. See [provenance](PROVENANCE.md), [review](REVIEW.md), [technical evidence](delivery/technical-verification.json) and [native acceptance report](../../research/2026-11-16-native-ai-acceptance.md).

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
python3 build-captions.py
```

For the Argo walkthrough, render its picture first, then from `walkthrough/`:

```sh
npm ci --ignore-scripts
node finish-argo.mjs ../renders/tableiq-walkthrough-picture.mp4
```

The result is `walkthrough/videos/tableiq-walkthrough-v2.mp4`. The official Argo export replaces picture audio with aligned narration and music. `argo-placements.json` records actual timing and must show zero overflow. The parent caption script uses those same scene offsets.

`produce-films.py` regenerates all compositions and overwrites manual scene edits. `prepare-captures.py` needs the original local recording workspace; the portable source already contains curated, key-free assets and does not need those raw recordings to re-render. The original approved [script](SCRIPT.md), [nine-frame board](boards/storyboard.html) and [storyboard review](boards/STORYBOARD-REVIEW.md) remain historical planning artifacts.
