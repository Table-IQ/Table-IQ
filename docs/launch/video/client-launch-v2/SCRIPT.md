> Historical planning artifact. The current approved production script is [APPROVED-SCRIPT-v4.md](APPROVED-SCRIPT-v4.md).

# TableIQ launch film — client first

Direction selected November 16, 2026. This is the script and storyboard stage, **not a newly rendered video**. Target: 56 seconds, 1920 × 1080, 30 fps. The existing published films remain unchanged.

## The proposition

**A free, open-source Mac database client without a tab paywall, with optional bring-your-own-key AI and visible coding-agent activity.**

The viewer is a developer investigating a database. The example follows one question: which orders have a paid payment but no shipped shipment? The payoff is a real, checked result. MCP is a useful final extension, after the viewer understands the ordinary database client.

## Reference and theme

[Ajay Misra’s Router launch](https://www.linkedin.com/posts/ajay-misra_today-were-launching-router-for-wallet-ugcPost-7526705352344588288-t3kl/) was inspected as a 34.2-second, 1280 × 720, 30 fps video. Sampled frames show a light ground, oversized short titles, changing scale, provider-selection examples, a concrete output, and an end card. The public post names the problem before explaining the solution. These are observations of the reference, not evidence that its style caused traction.

Use **light Pearl for the editorial scenes and the actual captured app theme for product footage**. TableIQ’s Pearl #F1F6FB, Ledger Ink #0C1A2B and Lumen Deep #0B7480 give a consistent brand treatment. The verified AI captures are dark; preserve them. Avoid alternating the whole film between unrelated themes or recoloring native screens. Instrument Sans 500/600, from the supplied licensed fonts. The supplied wordmark stays intact.

Borrow the reference’s visual economy and concrete examples. All copy, artwork, music and product proof in the TableIQ film must be our own or licensed; no reference-video footage or soundtrack is included.

## Exact sequence and headings

| Time | Heading | What the viewer sees | Planned action and transition |
| --- | --- | --- | --- |
| 0–3 s | **Still counting query tabs?** | Large opening question; the actual TableIQ editor enters at the lower edge. | “Query tabs” lands on the first beat. A horizontal move carries the editor into the next scene. No logo-only intro. |
| 3–7 s | **Keep your investigation together** | Actual workspace. Production pickup: open orders, customers and a SQL query as three real tabs. | Show the third tab opening, then hold. The storyboard uses an existing native workspace still; it does not fabricate the missing multi-tab capture. |
| 7–12 s | **Free. Open source. Local first.** | Supplied TableIQ wordmark, then the workspace fills the frame. | Wordmark reveals once; the workspace becomes the next scene. “Local first” describes the client, not cloud AI requests. |
| 12–21 s | **Browse. Query. Review.** | Tables → schema → SQL editor, with the same synthetic storefront database. | Three actual user actions. Establish the workspace before pushing into a table or editor. Keep labels readable; no tiny UI window. |
| 21–27 s | **AI, with your own key** | Real provider settings, then the question’s business meaning: paid payments, no shipped shipment. | Provider selection hands off to the question. Never show a key, even partially. Small note: optional; provider API charges apply. |
| 27–37 s | **Ask. Review the SQL.** | The genuine generated-SQL capture, initially idle. Editorial summary: “Paid orders with no shipped shipment.” | Question → actual generated SQL. Focus the EXISTS and NOT EXISTS clauses. Generation waits may be condensed and must be identified. Do not fake streamed text or a prompt submission. |
| 37–44 s | **You choose when to run** | Manual Run, followed by the genuine result: 102 / Northstar Demo / 4200; 104 / Acme Demo / 3100. | Hold on Run before execution, then hold the two returned rows at normal speed. State “2 orders · 7,300 cents” as this sample’s result. |
| 44–50 s | **See your agent’s queries** | Actual MCP Activity from the recorded local Codex session. | Show discovery → schema → query calls. Production pickup: open Activity from the current app toolbar. No invented refused-write row. |
| 50–56 s | **Get the Mac developer preview** | Supplied TableIQ wordmark; tableiq.ideolyst.com; Mac Apple silicon; Windows and Linux coming soon. | Resolve to a stable end card. Supporting copy: built with Rust; no TableIQ account required; unsigned / not notarized. URL holds at least 3 seconds. |

## Narration draft

This is proposed copy, not generated audio. It can be recorded by the founder. The film must also make sense muted through its headings and actual UI.

> Still counting query tabs?
>
> When one investigation needs another query, a tab limit breaks the flow.
>
> TableIQ is a free, open-source database client for Mac.
>
> Browse your tables, inspect your schema, and keep your SQL together.
>
> When you want help, use your own AI provider key.
>
> Ask which paid orders have no shipped shipment.
>
> TableIQ generates SQL for you to review. You decide when to run it.
>
> Here are the two matching orders in our sample database.
>
> Working with a coding agent? Open MCP Activity to inspect its calls and queries.
>
> Built with Rust. No TableIQ account required.
>
> Download the Mac developer preview. Windows and Linux are coming soon.

## Proof and fair comparison

- [TablePlus’s current pricing FAQ](https://tableplus.com/pricing) specifies two open tabs and two open windows in its free trial. Its paid license is perpetual; do not call it a mandatory subscription or a per-tab charge. The hook addresses this specific trial constraint, not every competing client.
- [DBeaver Community](https://dbeaver.io/about/) is already free and open source. Do not suggest otherwise. No matched performance benchmark exists here, so “faster than DBeaver,” “fastest,” RAM savings and startup multipliers are excluded.
- “Built with Rust” is a supporting implementation fact. Show the product task before the technology. “Lightweight” needs a defined measurement; the current arm64 DMG is 27,385,007 bytes, which is download size only.
- [The native AI report](../../research/2026-11-16-native-ai-acceptance.md) records the exact longer question, Gemini model, native app version, generated SQL and manually executed rows. The short question in the film is an editorial summary, not a claim that a shorter prompt was tested. One example is not a general accuracy rate.
- Optional cloud AI sends requests to the selected provider and can incur that provider’s charges. Do not say “all data stays on your device” over an AI scene.
- The MCP recording is a separate real Codex session on the same base sample dataset. It is not the continuation of the Gemini session. Preserve an explicit “Separate Codex session” caption at that transition.

## Capture list before animation/render

1. Capture three real tabs opening in installed 0.8.17; show the actual interaction without a license overlay. The existing workspace still only establishes the planned framing.
2. Capture the full ordinary-client flow on the same app version and theme: table selection → schema → SQL editor.
3. Capture provider selection with no secret visible. Record the exact accepted natural-language question entering the app, generation opening the editor, the idle review state, and manual Run. Existing genuine before/after stills prove the outcome; they do not prove a continuous recording exists.
4. Capture current MCP Activity entry point and one local coding-agent call. The existing 0.8.15 activity recording can be reused only with its version and separate-session label.
5. Record the proposed voiceover and source music separately. Do not reuse the LinkedIn soundtrack. Quiet instrumental bed, a click on real actions, and one short pause before the result. Final audio must be listened to, not only metered.

## Build order

Script → headings → posed storyboard images → real capture pickups → timed animation → audio → frame review → export. At this delivery the script and posed images are ready for review. No new MP4 is claimed or substituted on the website.
