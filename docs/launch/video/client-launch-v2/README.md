# Client launch v2 — script and storyboard

The founder requested the script, headings and phase-by-phase images before animation, using [this Router launch](https://www.linkedin.com/posts/ajay-misra_today-were-launching-router-for-wallet-ugcPost-7526705352344588288-t3kl/) as a visual reference. The selected route is a free database client first, followed by optional BYOK AI and visible MCP activity.

- [Exact sequence, narration, evidence and capture pickups](SCRIPT.md)
- [Production specification](SPEC.md)
- [Nine-frame plan](STORYBOARD.md)
- [Visual storyboard](boards/storyboard.html), with individual full-size images
- [Contact sheet](boards/stills/contact-sheet.jpg)
- [Screen provenance and checksums](asset-provenance.json)
- [Review](REVIEW.md)

This is a **posed storyboard**, not a new rendered film. Existing website videos are unchanged. Some poses use real native 0.8.15 footage; the AI before/after captures are genuine 0.8.17 stills. Missing continuous recordings, multi-tab actions and the current MCP entry-point shot are named explicitly. The LinkedIn video and soundtrack are not included.

## Preview and editing

Run from this directory:

```sh
npx hyperframes@0.9.56 preview --background
```

The current CLI's editor does not expose a Storyboard tab. Its regular player displays a review cover until the film is animated; individual poses are available under Compositions. The desktop app can be used for subsequent editing.

The complete image board works independently of that editor. Serve this directory with a static server, then open `/boards/storyboard.html`:

```sh
python3 -m http.server 4177 --bind 127.0.0.1
```

The shipped `compositions/*.html` files are editable poses. `build-poses.py` regenerates them from its shot list, so do not rerun it over manual scene edits. To regenerate every image and the board intentionally:

```sh
python3 build-poses.py
npx hyperframes@0.9.56 check --json
npx hyperframes@0.9.56 snapshot --at 1.5,5,9.5,16.5,24,32,40.5,47,53 --no-end -o boards/stills
python3 build-board.py
```

After a storyboard-only update, preserve the posed index locally and restore the review cover before handoff. Once animation begins, the actual film replaces that cover. Do not export the static poses as though they were a working-product demo.

Brand artwork comes from the TableIQ brand kit. Instrument Sans uses the included OFL license. App images retain original colors; specified crops are framing only. Future music and narration need their own source/license entries.
