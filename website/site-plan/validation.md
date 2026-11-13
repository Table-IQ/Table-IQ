# Validation: market, USP, Core Message (Stage 2)

Mode: Site. Sources: `docs/GTM.md` (vendor research, 2026-10-19) plus a live hero check on 2026-11-13.
Comparison claims are not published on the site until re-verified (see decisions.md, D-07).

## Competitor teardown (live, 2026-11-13)

| Competitor | Hero headline and subhead | Primary CTA | Proof used | Pricing shown | Tone and motion | What they leave out |
|---|---|---|---|---|---|---|
| TablePlus | "Database management made easy." Native GUI for MySQL, PostgreSQL, SQLite and more | Download for Mac | "Trusted by 100k+ developers" | Yes, $99 one device | Calm, native screenshots, little motion | Agents, MCP, what an agent ran |
| TablePro | "A native database client." Built with each platform's toolkit | Download | Testimonials, sponsors | Free | Plain, feature tour | Leads with native, agent controls sit deep in features |
| DataGrip | "Work with databases your way" | Download / trial | "630,000+ developers" | Paid subscription | IDE family branding | Simple Mac app story; agent view not on hero |
| Tabularis | "The database client your AI agent can actually use." SQL workspace, 30+ databases, built-in MCP | Download | GitHub stars, "why teams switch" | Free, open source | Developer, energetic | Inspecting each agent call beside your own review flow is not the lead |
| DBeaver | "Universal Database Tool" | Download | Breadth of engines | Free CE, paid editions | Dense, Java era | Mac feel, agents |
| dbx (GitHub) | Repository page only | Star / clone | ~27k stars | Free | n/a | Native Mac app |
| Status quo | psql plus an agent given a raw connection string | n/a | n/a | Free | n/a | No record of what the agent ran, no write guard |

## Patterns
- Table stakes: native or fast, many engines, free/open source, "works with AI/MCP".
- Gaps: showing the agent's exact SQL, returned rows and refused writes in the same client where a human reviews their own staged SQL. Plain statement of limits (read-only still exposes data).
- Cannot claim: speed benchmarks, user counts, testimonials, signed download, "safe" production access, exclusivity of MCP or read-only.

## USP candidates (1 to 5: true / matters / unique)

| Candidate | True | Matters | Unique | Total |
|---|---|---|---|---|
| A. See every agent query: SQL, results and refusals in MCP Activity, inside your Mac database client | 5 | 4 | 3 | 12 |
| B. Read-only by default, per shared connection | 5 | 4 | 2 | 11 |
| C. Every engine and feature free forever (written pledge) | 5 | 3 | 2 | 10 |
| D. Native Rust core, no bundled browser | 5 | 2 | 2 | 9 |

Primary: A. Supporting: B, C, D.

Differentiator check: no candidate scores 5 on unique. A is the strongest because competitors log agent calls but do not lead with reviewing them beside your own staged SQL. This is a positioning hypothesis (GTM.md says the same), not proven exclusivity. TODO-HUMAN: confirm positioning against Tabularis before launch.

## Core Message (from the approved live site and brand README)

```
WHO IT IS FOR:     Mac developers who use PostgreSQL or MySQL with a coding agent such as Claude Code or Cursor.
WHAT WE SOLVE:     Giving an agent database access means not knowing what it ran or whether it could write.
HOW WE SOLVE IT:   Share chosen connections over local MCP, read-only by default, then inspect each call in MCP Activity.
WHAT WE DO:        A free Mac database client to browse, query and edit, plus a visible, guarded agent connection.
WHY US, NOT X:     Unlike a raw connection string, you see every agent query, result and refused write.
PROOF:             A real Claude Code session where an UPDATE was refused (captured screenshot). TODO-HUMAN: user stories.
NEXT STEP:         Read the Postgres walkthrough (download is coming soon).
```
