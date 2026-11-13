# Site map and track (Stage 5)

| Route | Intent | Source |
|---|---|---|
| `/` | "mac database client for coding agents", "mcp database client" | Home, rebuilt |
| `/guides/postgres-investigation` | "claude code postgres mcp" how-to | Existing guide, wording verbatim |
| `/guides/mcp-permissions` | "mcp database read-only permissions" | Existing guide, wording verbatim |
| `/license` | "tableiq license agpl" | Existing page, wording verbatim |
| `/llms.txt`, `/llms-full.txt`, `/*.md` | AI assistants | Generated after build by `scripts/seo.py` |

Not built (paused, see decisions D-07): comparison pages ("TableIQ vs TablePlus", "alternative to Tabularis"). Pricing page: none, the product is free.

Track: **Motion**, chosen by the user (2026-11-13). Budget: one signature moment on the home hero (agent trace), micro-interactions elsewhere (heading reveals, tab indicator, step reveals). Guides and license stay still.

Stack: Next.js 16 App Router, static export to `out/`, Tailwind 4 with tokens as CSS variables, `motion` 14 (`motion/react`), Cloudflare Workers static assets. Mirrors the HealthPass Ideolyst site.
