# Rust, lightweight and fast: independent assessment and decision

Checked 15 November 2026. The founder requested a new researcher without previous chat context. A separate agent received only the public repository URL and a neutral research brief. It was instructed not to read earlier launch reports, communicate with other agents, edit files or install comparison applications. The assessment below summarizes its findings; the decision and verification are the coordinating agent's work. No performance benchmark was run for this assessment.

## Independent agent's findings

Rust deserves a visible supporting proof line. The current download size is concrete evidence. Broad claims about low memory or speed should wait for native-app measurements.

- The [Rust manifest](https://github.com/Table-IQ/Table-IQ/blob/484d71d91af5ae41a567c10508f23730bb3513b4/src-tauri/Cargo.toml) and [frontend manifest](https://github.com/Table-IQ/Table-IQ/blob/484d71d91af5ae41a567c10508f23730bb3513b4/package.json) establish a Rust backend, Tauri 2 and a React/TypeScript interface. This supports “Built with Rust and Tauri,” rather than “100% Rust” or “no JavaScript.”
- The [v0.8.17 release](https://github.com/Table-IQ/Table-IQ/releases/tag/preview-v0.8.17) has an Apple silicon DMG of 27,385,007 bytes: 27.4 MB decimal. A small download does not establish idle/working memory, installed footprint or responsiveness.
- The [May 21 browser benchmark](https://github.com/Table-IQ/Table-IQ/blob/484d71d91af5ae41a567c10508f23730bb3513b4/bench/results-browser.json) uses a synthetic million-row source in headless Chromium. Its 14 MB JavaScript heap and 60 FPS result apply to that test. They do not measure total native-app RAM, startup, database transport performance or a competitor advantage.
- [TablePlus](https://tableplus.com/) already promotes native performance. Its [free trial](https://tableplus.com/pricing) limits opened tabs to two; paid licenses are perpetual. The tab limit is an observable switching friction, while a speed advantage remains unmeasured.
- [DBeaver Community](https://github.com/dbeaver/dbeaver) is already free and open source. Source availability alone will not explain why a DBeaver user should switch.
- [Tabularis](https://github.com/TabularisDB/tabularis) also uses Rust, Tauri and React and includes MCP. The stack and agent integration are category attributes, not exclusive differentiation.

The agent's qualitative community reading suggested that responsiveness draws attention, while workflow completeness and trust affect switching. This is a hypothesis from self-selected discussions, not a representative developer survey or evidence of TableIQ retention.

## Coordinating agent's verification

I separately checked the local manifests and benchmark, the GitHub release API, current competitor pages, Tauri documentation and a relevant launch discussion before accepting the recommendation.

| Check | Result and implication |
| --- | --- |
| Release asset metadata | `TableIQ_0.8.17_aarch64.dmg` is 27,385,007 bytes, matching the published release notes. Use “27.4 MB download” with release/architecture scope. |
| [Tauri webview documentation](https://v2.tauri.app/reference/webview-versions/) and [process model](https://v2.tauri.app/concept/process-model/) | macOS uses the system WKWebView. The core does not render the interface itself; webview processes execute HTML/CSS/JavaScript. Count attributable helpers when measuring memory. Avoid calling the whole application runtime-free or equating the webview with platform-native controls. |
| [TablePlus pricing](https://tableplus.com/pricing) | Confirms the two-tab free trial restriction and perpetual paid license. Do not describe the paid product as a mandatory subscription. |
| [Tabularis technology section](https://github.com/TabularisDB/tabularis#tech-stack) | Confirms the competing Rust/Tauri/React stack. Rust is useful implementation information but does not make this category unique. |
| [Rustable Show HN discussion](https://news.ycombinator.com/item?id=50347524) | The maker also targets the two-tab limit. Several replies dispute the language-led justification, marketing tone or meaning of “native.” This small thread supports caution about those claims; it does not establish that developers generally dislike Rust or that the product lacks users. |

## Final positioning decision

Lead with **a free, open-source database client for Mac**, then demonstrate browsing, SQL and reviewed edits. Give TablePlus free users a concrete reason to try it: **SQL and table tabs without a paid limit**. Invite DBeaver users to compare one actual workflow; a better fit remains something to validate through repeated use.

Place this technical proof line below the main offer:

> Built with Rust and Tauri, using macOS's system webview. The v0.8.17 Apple silicon preview is a 27.4 MB download.

Use “I built an open-source Mac database client with Rust and Tauri” as possible framing for a Rust-focused community, followed by a useful task and honest limitations. That channel fit is an inference, not a forecast of traction. General developer/Mac distribution should lead with the database workflow. BYOK AI remains optional, with MCP further down the explanation.

Do not yet publish “faster than TablePlus,” “lighter than DBeaver,” a speed multiplier, 14 MB native RAM, or trillion-row throughput. Rust's design and a small installer do not prove those outcomes. The main demo is edited and cannot serve as a timing benchmark.

## Evidence needed for stronger claims

The [November 16 roadmap](../../ROADMAP.md#tomorrow--monday-november-16-2026-ist) already schedules matched measurements. Start with at least five paired exploratory samples per operation, alternate app order and retain raw outcomes. Record hardware, OS, app versions/build mode, identical database/query/row limits, network and cache state. Measure time to usable interaction, first visible query results, idle and working memory including attributable webview/helper processes, and a representative navigation/scrolling task. Separate server execution from client display time.

Use median and range for the initial small sample. Before advertising a robust comparative advantage, broaden to at least 20 trials, state the percentile method, include failures and unfavorable results, and publish the protocol/raw samples. Do not turn a single favorable operation into an overall “fastest client” claim. No such measurements or advantage are claimed in this report.

## Branding action

The Table-IQ organization's default identicon was replaced with the existing `src/assets/brand/icon-1024.png` app mark. The resulting public avatar was downloaded and visually verified. No new app icon design or binary release was required.
