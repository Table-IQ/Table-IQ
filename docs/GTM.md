# TableIQ Go-To-Market

_Last updated 2026-09-30 · companion doc: [ROADMAP.md](ROADMAP.md)_

## 1. What we learned from dbx

[dbx](https://github.com/t8y2/dbx) was created on 2026-04-29. By 2026-09-30 it had:

- **22.3k stars** and 2.1k forks, still accelerating (about 3k stars in the last two weeks);
- **2.69M** release downloads, peaking at about 800k in July;
- 305 contributors, 1,278 open issues;
- a #2 spot on GitHub Trending overall (2026-08-28);
- v0.6.28 shipped 2026-09-29, still on a near-daily cadence.

**Why it worked**

1. **A one-line hook it can keep upgrading.** The tagline is *"100+ databases in 25 MB."* Both numbers were bumped repeatedly: 40+ → 100+ databases, and 15 → 20 → 25 MB. Each bump was a new headline.
2. **A named enemy in every README and article.** "DBeaver needs Java. TablePlus is freemium. Navicat costs ¥1,398." Articles titled "Goodbye Navicat" did the distribution.
3. **Shipping as marketing.** 148 releases in 154 days (almost daily) and about 50 commits a day, mostly AI-assisted. The repo always looks alive.
4. **Owning one home community.** HelloGitHub feature, a 1Panel partnership, QQ/WeChat/Feishu groups, CSDN articles, and a long list of domestic Chinese databases (Dameng, Kingbase, GaussDB…). **Hacker News and Product Hunt played almost no part.** The PH page has one upvote.
5. **Riding the AI-agent wave.** A built-in agent that can drive Claude Code or Codex, an `npx @dbx-app/mcp-server` (also a native Rust binary), a CLI "agent skill", and an MCP-directory badge. **Its MCP already has** `read_only` / `safe_write` / `high_risk_write` modes, a connection allowlist and a central policy. We must not present those as our invention.
6. **Free, with no catch.** Apache-2.0, no paid tier. It is funded by README sponsor slots (mostly Chinese cloud and AI-API resellers) and donations. That income does not transfer to a Western audience.
7. **More surface than a desktop app.** Desktop, Docker and Web builds with the same feature set, a plugin marketplace (`dbx-store`, Go and TS SDKs), a Discord, and packages for 1Panel, fnOS and even Windows 7.

**Where dbx is weak (our openings)**

- **Quality debt:** 1,278 open issues, 3,655 labelled bug, grid lag (#9246), slow metadata on high-latency links (#6718), IME double input (#982).
- **The Western developer audience is under-served.** Its community, sponsors and roadmap all lean toward China.
- **The size claim is soft.** 25 MB is true only for the Windows NSIS installer. The macOS dmg is 36 MB and the AppImage 111 MB.
- **Breadth over depth.** More than 100 profiles, many of them thin.
- **Agent safety is a setting, not a guarantee.** Its modes are app-level policy. Nothing we found enforces read-only at the database transaction level or fails closed when the policy file is missing.

**The rest of the market moved too (researched 2026-09-30)**

- **DataGrip 2026.2** embeds Claude Agent and Codex, ships an MCP server and three "database skills". $229/user/year.
- **DBeaver PRO 26.0** added MCP; **Ultimate 26.1** lets you attach external MCP servers. Subscription only.
- **Beekeeper Studio** (23.7k★, GPLv3 community + commercial Ultimate) gates engines and multi-device on paid tiers ($9–18/user/month). Profitable, no investors.
- **DbGate** (7.3k★, GPLv3) charges $120/year Premium and $150/user/year Team.
- **TablePlus** is $99 single, $129 for two seats, $59/device/year to renew, with a 2-tab free tier. No MCP or agent features as of its latest release notes.
- **DB Pro's Show HN** (Dec 2025) got 33 points. "Another database client" does not win HN; the top comment demanded a one-time purchase over a subscription.

**Takeaway:** "AI agent + database client" is now table stakes at the top of the market. The open lane is *free and provably safe*, sold to the English-speaking agent-builder audience that dbx doesn't reach.

## 2. Positioning

> **Give Claude Code your production database. Safely.**
>
> TableIQ is the free, open-source database client built for the agent era: every query your agent runs is visible, and writes are refused unless you say otherwise.

One wedge, not three. The agent-safe story is the headline; polish and scale are the proof that it's a real client and not an MCP wrapper.

| Pillar | Proof we already have | What dbx has instead |
|---|---|---|
| **Agent-safe** (the wedge) | Fail-closed guard, `READ ONLY` transactions enforced at the DB, keychain-held token, MCP Activity log | App-level modes and an allowlist |
| **Scale** (the credibility post) | Keyset paging to 1T rows (2.3 ms at row 15M vs 2.3 s with OFFSET), bounded and cancellable page fetches | Grid lag issues (#9246) |
| **Polish** (why you keep it open) | macOS-native chrome, staged edits with SQL review, ⌘K parity, SQL log | Vue UI the maintainers say "can't sustain growth" |

We still need engine breadth, as table stakes: roadmap P1 (+22 profiles) and P3 bring us to about 50 without growing the binary. **We publish size honestly, per OS.**

**We do not claim "safer than dbx" until the guard test suite (§4) proves it.**

**The enemies we name:**

| Competitor | What we say |
|---|---|
| TablePlus | "Same feel, $0, and your agent can use it." |
| DBeaver | "No Java, opens in under a second. MCP without the PRO licence." |
| DataGrip | "Agent access without the $229/year." |
| dbx | We never attack it by name. Our comparison table is enough. |

**Comparison table columns** (README): price · size per OS · engines · MCP (free / paid only / none) · agent write protection · 1T-row paging · open source.

## 3. Who we're for (ICP, in order)

1. **AI-agent builders** who want Claude Code, Cursor or Codex to query real databases without the risk of `DROP TABLE`. This is the wedge. **Where to reach them:** r/ClaudeAI, r/cursor, MCP directories, the Anthropic/Cursor Discords, Claude Code and Cursor docs.
2. **Mac developers** who find TablePlus's paywall ($99, then $59/device/year) or DBeaver's weight annoying. They use Postgres/MySQL daily. **Where to reach them:** r/macapps, r/PostgreSQL, HN, X. SEO for "TablePlus alternative".
3. **Data folks on DuckDB, Parquet and ClickHouse** who want a fast local grid. **Where to reach them:** the DuckDB Discord, r/dataengineering, the benchmark post.
4. **(Later) Platform and security teams** whose developers already run agents against databases and who need to see and govern that. They are the buyers for the Team tier (§5). **Where they come from:** company domains that appear in our stars and issues.

## 4. Launch plan

### Gate: roadmap P0
The repo is private today with no LICENSE. We launch only once all of these are true:

- LICENSE (Apache-2.0) is in place and the UI string matches;
- the repo is **public**, with the "what stays free forever" pledge in the README (§5);
- builds are signed and notarized;
- the auto-updater works;
- the Homebrew cask exists;
- the README has real per-OS sizes;
- the guard test suite passes on our MCP (below).

A first-run crash on an unsigned build kills a launch. So does a guard bypass found in the HN comments.

### Assets (build before launch day)
- [ ] **The refusal demo** (30 s, hyperframes): Cursor runs a report through TableIQ's MCP, then tries a `DELETE`; the Activity log shows the refusal. This is the Show HN.
- [ ] **A public guard test suite** (`tableiq/mcp-guard-tests`): SQL-injection, multi-statement, CTE-write, function-call and encoding cases for MCP database servers, with a runner for ours, dbx, the DBeaver MCP and the official Postgres MCP. **Any case we fail gets fixed before launch. We publish the table only if we win it.**
- [ ] **A README that sells in 5 seconds:**
  - a hero GIF;
  - the hook line (*"Give Claude Code your production database. Safely."*) with the sub-line (*"Free, open source, 1-trillion-row tables."*);
  - one-click MCP install snippets for Claude Code, Cursor and Codex;
  - an engine badge row;
  - the comparison table (§2 columns) against TablePlus, DBeaver, DataGrip, Navicat and dbx;
  - `brew install --cask tableiq`.
- [ ] **A one-page site:** hero, download buttons per OS, the comparison table, the guard results and the benchmarks.
- [ ] **A benchmark post:** "Paging a trillion-row table in 2 ms", with reproducible scripts from `docker/postgres/big.sql`.
- [ ] **An MCP listing** on mcp.so, Smithery, Glama, PulseMCP, awesome-mcp-servers, and in the Claude Code and Cursor docs PRs where accepted.

### Sequence
| Week | Channel | Angle |
|---|---|---|
| L-2 | Private beta: 20–30 developers from our own network, plus the Discord | Find first-run bugs, collect quotes |
| L-0 Tue | **Show HN** | "Show HN: TableIQ – open-source DB client whose MCP server can't be talked into a DELETE" (lead with the guard, not "TablePlus alternative") |
| L-0 | X/Bluesky thread + refusal demo | Tag the Tauri, DuckDB and Anthropic dev-rel accounts |
| L+1 | r/ClaudeAI, r/cursor, r/macapps, r/PostgreSQL, r/dataengineering (one post each, spread over days, tailored to each) | Agent-safety for the first two; polish and scale for the rest |
| L+1 | dev.to/Hashnode "Why I built…" and the benchmark post | SEO for "TablePlus alternative" and "DBeaver alternative" |
| L+2 | **Product Hunt** | Only once we have at least 500 stars and reviewers lined up |
| L+2 | Awesome-lists: awesome-tauri, awesome-rust, awesome-postgres, awesome-mcp-servers, awesome-mcp-clients, awesome-macOS | Permanent backlinks |
| Later | **zh launch** (needs zh-CN i18n and a community partner) | Moved out of the 90-day plan. dbx owns this market with 1Panel, QQ groups and a CNB mirror; a week-4 HelloGitHub submission would be noise. Revisit once we have 5k stars |

### Keep the momentum (the dbx engine without the churn)
- **A weekly release**, each with a changelog post whose headline is a number or a feature ("+11 MySQL-wire databases", "Mongo editing is here").
- **One roadmap phase = one announcement.** P1 → "TableIQ now speaks 27 databases". P4 → "We cut the binary by X MB".
- **"Good first engine" issues.** The P1 profile table is a place contributors can add a database in about 30 lines. This is how dbx reached 305 contributors.
- **A public roadmap** with `ROADMAP.md` linked from the README. Let users 👍 the next engine.

## 5. Monetization

**Decision: open core.** Apache-2.0 desktop client, free for individuals forever, plus a commercial Team tier added after launch.

**Why the core is open (Apache-2.0, not FSL/BSL/AGPL)**

- The free floor is dbx: 22k★, 100+ engines, AI and MCP, Apache-2.0. A closed freemium client from an unknown team has no distribution against that.
- People will read the guard code before pointing an agent at production. That is only a selling point if they can.
- FSL and BSL protect against someone hosting your software as a service. Nobody hosts a desktop client, so they cost OSI status and goodwill for no protection.
- AGPL scares enterprise buyers and needs a CLA (Beekeeper's situation). Apache-2.0 contributions already permit proprietary reuse, so the Team layer needs no CLA.

**What stays free forever** (pledge in the README, PostHog-style)

All engines, the MCP server and guard, BYO-key AI, staged edits, SSH tunnels, unlimited connections, and every feature that exists on launch day. We never gate engines or device count (Beekeeper does, and it is their most resented decision; dbx gives them away).

**The Team tier** (commercial licence, separate `ee/` tree, built at day 90+ only after the OSS launch has traction)

Enterprises in 2026 buy governance around agents, not database features. The tier is the guard and the Activity log extended to a team:

- shared connection vault with SSO;
- central MCP policy: which agents, which connections, which mode;
- audit export to SIEM, and an approval flow for agent writes;
- web/Docker self-host for teams (roadmap P7);
- offline licensing.

**Pricing shape:** per seat, minimum 5 seats. Offer a perpetual licence with one year of updates alongside annual; HN attacked DB Pro for subscription-only until the founder added exactly that. Reference points: DbGate Team $150/user/year, TablePlus $99 + $59/device/year, DataGrip $229/user/year. Set the number with design partners, not before.

**Enterprise motion (no sales team):** watch for company domains in stars and issues, offer three to five of them the Team tier free as design partners, price from what they use. The pitch: *your developers already run agents against your databases; this is the only client where security can see every agent query and set the policy.*

**Community motion:** public roadmap, contributor credits in every release note, "good first engine" issues, a Discord, and the pledge above. When the Team tier ships, the announcement says "here is what we built for teams; everything you use today stays free".

**Side income, not a plan:** GitHub Sponsors from day one. README slots only if a developer-relevant sponsor asks; dbx's slot income comes from Chinese AI-API resellers and does not transfer.

## 6. Metrics

We ship with zero telemetry, so we measure from the outside.

| Metric | Source | 90-day target |
|---|---|---|
| Stars/week | GitHub, star-history | 5k total (1k in launch week is the realistic bar for a good Show HN; DB Pro's 33-point post is the floor to beat) |
| Downloads/release | GitHub Releases API + Homebrew analytics | 20k cumulative |
| Contributors | GitHub | 25 |
| MCP installs | npm downloads (standalone MCP), directory listings | 2k |
| Guard suite | public repo, CI badge | 0 failing cases; results table cited by at least one third party |
| Design-partner leads | company domains in stars/issues | 5 conversations, 3 partners |
| Qualitative | HN/Reddit comments, issues opened by non-team users | Top-3 complaints fixed within 2 weeks |

Opt-in only, never default: a "send anonymous usage" toggle, if we ever need retention data.

## 7. 30 / 60 / 90 days

- **Day 0–30:**
  - Finish P0 (licence, public repo, signing, updater, sizes).
  - Guard test suite, refusal demo, README, site, benchmark post.
  - Private beta.
  - Show HN, X, Reddit.
  - MCP directories and one-click install docs.
- **Day 31–60:**
  - Ship P1 (profiles) and P2 (Mongo and Redis) as two announcements.
  - Product Hunt.
  - Awesome-lists.
  - Open "good first engine" issues.
  - Discord.
  - Start logging company domains from stars and issues.
- **Day 61–90:**
  - Ship P3 (ClickHouse, Turso, Elastic, vector databases).
  - Start P4 (the agent runtime, with DuckDB moved out) and write the "we cut X MB" post.
  - Three to five design-partner conversations for the Team tier.
- **Day 90+:**
  - P7 Team tier with design partners; announce with the pledge.
  - zh-CN i18n and the zh launch, if stars justify it.

## 8. Risks

| Risk | Mitigation |
|---|---|
| dbx out-ships us on engine count | Don't fight on count. Lead with polish, scale and agent safety; use profiles for cheap breadth |
| Launching with bugs burns our one HN shot | The P0 gate plus a private beta. QA through the IPC bridge, never the browser mock |
| Daily releases turn into churn (dbx has 3.6k bug issues) | Release weekly, not daily. Every release passes CI and bridge QA |
| MCP safety incident | The guard stays fail-closed. Any relaxation needs its own review. Read-only is the default. The public guard suite runs in CI |
| We claim "safer than dbx" and someone finds a bypass on HN | No comparative safety claim until the guard suite is public and we pass every case. Fix before publish |
| DataGrip and DBeaver PRO make "agent + DB client" feel solved | We don't sell the feature, we sell free + provable safety. Comparison table shows their MCP as "paid only" |
| Open-core suspicion ("they'll paywall it later") | The "what stays free forever" pledge in the README from day one; Team tier gates only team-governance features |
| License confusion (the UI says MIT today, the repo is private with no LICENSE) | Fix both in P0 before any public post |
