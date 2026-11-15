# Independent review: lead with the database client

Checked 15 November 2026. This review independently reads current vendor sources, first-person community discussions and the TableIQ repository. Vendor statements establish advertised capability, not comparative quality. HN comments are a small, self-selected qualitative sample. Stars and launch votes do not establish retention, market size or demand. No other agent's report informed this review.

## Recommendation

Lead with **a free, open-source database client for Mac**. Demonstrate **browse → query → review changes**. Present bring-your-own-key AI as an optional convenience, with provider charges and data sharing explained. Put external-agent/MCP integrations in a lower, optional disclosure. A developer should understand the product without knowing what MCP means.

Suggested hero:

> **A free, open-source database client for Mac.**
>
> Browse your tables, write SQL and review changes before you commit. Connect to PostgreSQL, MySQL, SQLite and more. Add your own AI provider when you want help with SQL.

Primary action: **Download developer preview**. Secondary: **Watch the everyday workflow**. Adjacent qualification: **Apple silicon Mac · unsigned developer preview · start with local or disposable data**. No account required. Do not imply AI is required to connect or query.

Use “An open-source alternative to TablePlus and DBeaver” in the comparison introduction, search description or launch body, after defining the category. DBeaver is itself open source; that sentence must not imply otherwise. Do not use “the better/faster alternative” without evidence.

## What the market actually overlaps

| Product | Verified current evidence | Consequence for TableIQ |
|---|---|---|
| TablePlus | Official pricing lists a free trial limited to two open tabs, two windows and two advanced filters; $99 Basic is a perpetual license with one year of updates, not a mandatory subscription. Its shortcuts include SQL preview before committing changes. | Unrestricted free client use is a concrete reason for a trial user to try TableIQ. “Review before commit” is useful behavior, not a unique invention. Paid TablePlus users already own a capable tool. [Pricing](https://tableplus.com/pricing), [SQL preview shortcuts](https://tableplus.com/docs/utilities/shortcut-keys). |
| DBeaver Community | Free and Apache-2.0, broad JDBC support, SQL/data/schema tooling, import/export and AI Chat. Its repository explicitly lists OpenAI/custom-endpoint support for Community; additional native providers and tools are in PRO. DBeaver also supports persistent manual transactions and rollback. | Neither “free/open source” nor “AI included” uniquely displaces DBeaver. Do not label all its AI paid. TableIQ's missing persistent per-tab server sessions are material for developers who use session state or multi-run transactions. [Community repository](https://github.com/dbeaver/dbeaver), [AI documentation](https://dbeaver.com/docs/dbeaver/AI-Smart-Assistance/), [Transaction modes](https://dbeaver.com/docs/dbeaver/Auto-and-Manual-Commit-Modes/). |
| TablePro | The current official site offers free core database tools with paid Pro/Team additions and says its source, including paid features, is AGPLv3. It leads with the client and shows querying/data editing before optional AI/MCP. | Another free, open-source Mac client already occupies the broad positioning. Earlier descriptions of TablePro as MIT must not be repeated. Native platform UI is its stated focus; TableIQ uses a system webview with a Rust backend. [Current TablePro site](https://tablepro.app/). |
| Tabularis | Current official homepage describes an open-source SQL workspace; its docs describe SQL tooling, BYOK/local AI and built-in MCP, with connection selection/read-only/approval controls. | Rust, an open-source client and agent access are crowded claims, including their combination. MCP is an advanced capability, not an uncontested category TableIQ owns. [Homepage](https://tabularis.dev/), [MCP setup and controls](https://tabularis.dev/wiki/mcp-server). |

TablePlus also now documents a built-in AI assistant with API-key providers, local Ollama and custom endpoints, plus a built-in MCP server with token permissions. Therefore “competitors have no BYOK/local AI/MCP” is false. This does not prove equivalent behavior or price-tier availability for every operation; a public comparison should state only the verified scope. [TablePlus AI](https://tableplus.com/docs/llm-plugin), [TablePlus MCP](https://tableplus.com/docs/preferences/mcp).

## Community signals and their limits

Three recent first-person launch threads are more useful than generic “trending repository” imitation:

1. The Rustable launch explicitly targets TablePlus's two-tab free limit, but replies challenge its implementation-language justification, marketing tone and use of “native.” The lesson is not that Rust is disliked: implementation language does not establish a user benefit, and careless category claims distract from the workflow. [Rustable HN thread](https://news.ycombinator.com/item?id=50347524).
2. In TablePro's launch, a prospective switcher lists concrete blockers: whole-app keyboard operation, formatting controls and database debugging. They also question AI rewrites that cannot guarantee semantic equivalence. Other feedback is positive. This shows switching depends on the specific job, not a screenshot or an AI badge. Do not promise AI optimization preserves results without verification. [TablePro HN thread](https://news.ycombinator.com/item?id=50243653).
3. DBDelve's maker supplies environment and dataset details for performance claims, notes an intentionally unusual full-table benchmark, acknowledges immature feature parity, and offers existing-client connection import. The thread also contains skepticism about whether new projects will be maintained. For TableIQ, reproduce a useful workflow and show responsive maintenance; do not borrow DBDelve's timings, imply connection import exists, or infer Rust automatically wins. [DBDelve HN thread](https://news.ycombinator.com/item?id=50427934).

These threads are anecdotes, not a representative survey. Their recurring questions are credible evaluation prompts: Does my actual workflow work? Can I trust edits? Does keyboard navigation work? Is this maintained? What does it omit?

## Why would someone switch?

**A plausible initial audience:** an Apple silicon developer who mostly browses tables, runs ordinary SQL and makes occasional reviewed data edits; wants unrestricted free client use and source access; is willing to test a new preview on disposable data. A TablePlus free user hitting tab limits has an observable friction. A DBeaver user who prefers TableIQ's actual interface may have another reason, but this remains a preference hypothesis until repeated usage proves it.

**Reasons to stay:** a paid TablePlus user already has its mature workflow; a DBeaver/DataGrip user may require persistent transactions, session variables, a particular driver, advanced formatting, database debugging, enterprise authentication or operating systems TableIQ's preview does not ship. TableIQ does not currently justify a universal replacement claim. Its README already discloses no persistent per-tab server sessions and engine-dependent cancellation. Those are product limitations, not messaging problems.

**Candid assessment:** the product addresses a real, established need. The current evidence does not demonstrate a defensible moat or inevitable virality. Free access, understandable interaction, source availability and attentive support may win a small audience if the core experience holds up. Optional provider choice can improve that offer, but is also increasingly common. Ask people to **try one real task beside their current client**, then measure whether they voluntarily return.

Rust/Tauri is a factual architecture statement. It does not establish faster startup, lower memory, faster query execution, better accuracy or superior safety. Measure those under published comparable conditions before putting them in a comparison. Query timings also include server/network behavior.

## Website and README order

Use the same order so a launch visitor and a GitHub visitor recognize the same product:

1. Clear client category, browse/query/review promise, visible DMG action and preview limitations.
2. A real 45–60 second everyday-workflow recording, followed by a screenshot or interface preview.
3. The core workflow: connect to a disposable database; browse/filter; write/run SQL; stage a grid edit and review its SQL before applying it. Do not suggest an arbitrary SQL editor UPDATE is staged or that staging means a persistent manual transaction.
4. Free/open-source terms, no account required, unrestricted current desktop features. Be explicit that external AI provider use may cost money. Do not create a “free AI credits” impression.
5. Short factual TablePlus/DBeaver comparison. Acknowledge DBeaver Community's source license and AI support, and TablePlus's perpetual paid license and limited free trial.
6. Optional BYOK AI: choose provider/model in Settings → AI, add its API key or a local Ollama endpoint, and inspect generated SQL before running. Separate API billing from consumer chat subscriptions. Never promise guaranteed accuracy.
7. Supported engines with actual capability/verification boundaries, connection and SSH details, known limits.
8. Optional collapsed external-agent integrations: local MCP setup, chosen connections, read-only default and MCP Activity. Keep the existing MCP movies here. Mark illustrative traces and browser-interface screenshots as illustrations/previews; only actual recordings are real native footage.
9. First-task docs, contributing, issue reporting, security and license.

The existing agent-first hero and “Or use TableIQ on its own” framing reverse the hierarchy. Replace them. Remove MCP trace animation from the hero; it can remain in the advanced section with its illustrative nature stated. Keep existing design, logo, fonts and colors; this is a hierarchy/copy correction.

## Main demo: 56 seconds, one ordinary task

Use actual native TableIQ footage of a synthetic local SQLite database or verified disposable Postgres connection. Label it synthetic data and an edited demonstration. Confirm all visible actions against the build being recorded. The primary story works without AI or an external coding agent.

| Time | Actual shot | Short on-screen text / purpose |
|---|---|---|
| 0–3s | App already visible with the relevant orders table; show a real row/column. | “Find the order. Check the change.” A concrete database task is the hook. |
| 3–10s | Open the saved local connection and browse/filter the table. | “Browse your data.” Enough UI remains visible to identify the client. |
| 10–22s | Open the SQL editor or an existing `.sql` file, run a short SELECT and let the real returned rows settle. | “Write SQL. Read the results.” Keep SQL and result text readable for several seconds. |
| 22–37s | Return to the editable grid, change one synthetic value and open the actual staged SQL review. | “Review the SQL before you commit.” Show the exact predicate and value; do not conceal unrelated pending changes. |
| 37–45s | Apply the staged edit to the disposable database and refresh/select the row so its actual new value is visible. | “Apply the change. Verify it.” If this sequence fails, repair/re-record or omit it; never splice a pretend success. |
| 45–50s | Return to the normal client workspace. Optional very short view of real Settings → AI only if it helps the story. | “Optional AI. Your provider.” Skip configuration if it makes the main story rushed. |
| 50–56s | Product mark and download action, then a short readable hold. | “TableIQ · Free and open source.” “Mac developer preview · Apple silicon · unsigned.” |

**AI recording gate:** the observed native AI request returned 401, so a successful in-app completion is unverified. Show only genuine configuration if AI appears; do not animate a prompt into fabricated generated SQL or imply a previously typed query was produced by AI. Better: make the launch film entirely about the functioning client and publish a separate real AI workflow after a native provider request succeeds and its SQL is checked. Secrets must never appear on screen. The current MCP films can remain as separately labeled advanced demonstrations; they should not be the main “what is this?” movie.

## Validation that matters this week

Invite existing testers and a small number of new relevant developers to perform one ordinary task without needing a walkthrough. Record their current client, database, installation friction, first successful query, whether they understand review versus execution, missing must-have features and whether they return during the following week. Use consensual interviews or issue templates; do not add hidden telemetry. Five enthusiastic existing testers are useful feedback, not market validation.

The near-term launch claim can be modest and clear: “I built a free, open-source Mac database client. Here is a real browse/query/review workflow; it is an unsigned Apple silicon developer preview. What would stop you using it for one everyday task?” This is a defensible invitation. A trending promise is not.
