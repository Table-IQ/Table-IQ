# TableIQ launch films

Three 75-second films use the approved visuals and [script](APPROVED-SCRIPT-v4.md), with the selected v5 audio revision: soft, very quiet background music and clear, gentler narration.

| Film | Treatment |
| --- | --- |
| [Guided walkthrough](https://tableiq.ideolyst.com/video/tableiq-launch-walkthrough-v5.mp4) | Primary film; actual Argo recording/alignment/export workflow |
| [Story](https://tableiq.ideolyst.com/video/tableiq-launch-story-v5.mp4) | Narrated editorial investigation |
| [Motion](https://tableiq.ideolyst.com/video/tableiq-launch-motion-v5.mp4) | Quiet music and concise headings |

[Watch the gallery](https://tableiq.ideolyst.com/launch-films/). Guided is first in the gallery and remains the homepage demo. The app download is unchanged at 0.8.17.

## Audio

The founder chose **soft, very quiet music with clear narration** from three directions: quiet music, voice only, or gentle piano. We retained the approved speaker and words, slowed delivery 4% without changing pitch, softened the tonal edge, and levelled each utterance to −17 LUFS. The backing track is filtered and levelled separately to −36 LUFS, then reduced another 6 dB around speech. The measured music/voice separation over active speech is approximately 25 dB. The music-only cut targets−30 LUFS,14LU below its previous master. No final normalization raises the bed again.

Sound is measured and compared; it has not received an independent human listening assessment. The founder’s listening feedback remains authoritative. [Current review](REVIEW.md) and [technical evidence](delivery/technical-verification-v5.json) record the checks.

## Genuine product evidence

Real development UI connects to the actual Rust backend and a synthetic SQLite database. The manual query is entered and Run is clicked; the actual Arrow response contains exactly the two expected orders. The AI segment is a separately labelled still from one previously verified native 0.8.17 case. MCP is a real local-client session, separately labelled. No general AI accuracy or speed advantage is claimed. [Provenance](PROVENANCE.md) and [native acceptance](../../research/2026-11-16-native-ai-acceptance.md) explain the limits.

The v5 picture payloads are identical to the approved v4 masters. The new delivery changes sound only. Huashu Motion and Take One were creative references; those services were not connected. Motion/story are editable HyperFrames projects. The guided film uses the actual pinned official Argo package.

## Edit and reproduce

This directory is a collection of three projects: `walkthrough/`, `story/`, and `motion/`. The [release](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17) includes the self-contained v5 source ZIP with the curated media omitted from Git. Requirements: Node22+, FFmpeg, Python3, Chromium.

From a project directory:

```sh
npx hyperframes@0.9.56 preview --background
npx hyperframes@0.9.56 check --json
npx hyperframes@0.9.56 render --quality delivery -o ../renders/picture.mp4
```

The HyperFrames desktop app can edit these same files. Stop the preview before opening that project in the desktop app.

To rebuild sound from the original approved narration and music, from this directory:

```sh
python3 prepare-audio-v5.py
python3 produce-films.py
python3 build-captions.py --version v5
```

For the guided film, from `walkthrough/`:

```sh
npm ci --ignore-scripts
node finish-argo.mjs ../renders/picture.mp4 v5
```

Argo must report zero overflow. It supplies the aligned/exported sound; the reviewed HyperFrames picture is preserved to avoid an intermediate transcode color shift. From the parent, mux the final 75-second delivery without a loudness boost:

```sh
python3 mux-audio-v5.py walkthrough/videos/tableiq-walkthrough-v5.mp4 renders/tableiq-launch-walkthrough-v5.mp4 --treatment walkthrough
```

Use `--treatment story` or `motion` with that project’s rendered picture for the other cuts. Do not apply the old loudness-mastering recipe: it would undo the intentionally quiet music.

## Cleanup

Superseded prototype boards, duplicate source backups, intermediate review screenshots and old encoded-review files have been removed from the current tree. Earlier source packages remain on the release and in Git history. Old public video URLs redirect to their archived original release files through `website/public/_redirects`; current pages reference only v5 plus the still-relevant advanced MCP examples. Only current production source, evidence and assets belong in new commits.
