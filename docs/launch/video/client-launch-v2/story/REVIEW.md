# Story v4 review

Approved 75-second audience script implemented in seven editable scenes. Exact slots: 0–8, 8–18, 18–27, 27–43, 43–56, 56–65, 65–75 seconds; 1920 × 1080 at 30 fps.

## Validation

HyperFrames 0.9.56 full check passed with zero errors and zero warnings across lint, runtime, layout, motion, contrast, craft and audio. Report: `review/v4-check.json`. GPU: Apple M4 / ANGLE Metal. No delivery MP4 was rendered in this production pass.

Inspected the fifteen final frames in `review/v4-final-frames/`, including every scene, the manual Run at 32.2 seconds, result at 33.5 and 37 seconds, result hold at 41.5 seconds, AI still label, and CTA at 70 seconds. Final scene-boundary and phone review evidence is in `review/v4-storyboard/`.

The initial source derivation held a connection screen after the result. Pixel review caught this despite clean automated checks. The capture pipeline corrected the source trim to a verified genuine two-row frame; the corrected 37-second film frame has been inspected at full resolution.

## Evidence and timing

The investigation uses one continuous actual TableIQ development capture, synthetic data, manual SQL and a real Run: input 0.806, query ready 1.780, Run 5.105, result 5.734 seconds relative to the scene. Query focus returns wide before Run. The result title appears after the actual result and a 1.6× camera focus follows, then returns to context. Result rows are 102 / Northstar Demo / 4200 and 104 / Acme Demo / 3100.

Tabs, browse and MCP are genuine recorded sources; browse and MCP camera changes follow their measured state changes. No cursor, prompt, SQL population or result is fabricated. AI footage is separately labelled “Verified native AI example · SQL before Run · Still.” AI is optional; BYOK and provider API charges are explicit. MCP is labelled a separate sample session.

The CTA states Mac / Apple silicon, developer preview, unsigned / not notarized, and Windows/Linux coming soon. No comparative speed, memory or paid-DBeaver claim is present.

Preview: http://127.0.0.1:3009/#project/story (background process verified HTTP 200). The HyperFrames desktop app can edit the source. Narrated captions, final audio mixing, official Argo finishing for the walkthrough, independent review and final delivery renders are owned by the delivery pipeline. Historical v3 source and review are retained under `review/v3-source/`.

## Independent review correction

The story MCP close-up initially clipped the left edge of the TOOL column. Its camera x offset was adjusted from −660 to −600 pixels, retaining the same scale, vertical offset and timing. The 60, 60.5 and 61-second frames were inspected; tool names, SQL, rows and status are fully visible. Focused full check passed with zero errors and warnings: `review/v4-mcp-check.json`. Corrected frame evidence: `review/v4-mcp-corrected/`.
