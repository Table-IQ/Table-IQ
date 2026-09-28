# Connected home: capsule titlebar, lobby, status-bar icons, native View menu

**Approved design:** `~/.gstack/projects/RudreshNarwal-Table-IQ/designs/connected-home-v2-20260927/final.html`. The same folder holds `brief.md` and the state screenshots (`lobby.png`, `table.png`, `settings.png`, `dark.png`, …). Rudy approved it on 2026-09-27 with the phrase "A plus C, all built features, desktop-native, TablePlus capsule toolbar".

## Goal
Make the workspace a Mac app that matches the approved design:
1. The titlebar becomes TablePlus-style **glass capsule groups** with 20px icons and an environment-tinted status pill.
2. When no tabs are open, the content area shows a **database lobby** instead of today's "Connected to X" placeholder.
3. The table and workspace **status bars get icons**.
4. A native macOS **View menu** is added.

## In scope
### 1. Titlebar (`MainShell.tsx` header), left to right after the traffic lights
| Group | Controls |
|---|---|
| G1 | Toggle sidebar ⌘[ · Open connection ⌘O (plug icon) |
| G2 | Discard · Review (popover listing staged edits) · Commit ⌘S. These act on the **active table tab's staged edits** and are disabled when there are none. |
| G3 | Open database ⌘D (hidden for file engines) · **SQL** text glyph = New query ⌘T · ERD · "+" menu (New Table…, Import CSV / JSON…, Diff with another connection…) |
| Pill | `ENV \| ENGINE VERSION? : SSL : SSH : connection : database[ : schema.table]`. Clicking the connection name opens ⌘O and clicking the database opens ⌘D (same as today's capsule). The pill is tinted by environment. While connecting it shows a spinner and "Loading schema…". |
| G4 | Cancel running queries ⌘. (stop icon) · Refresh schema · Command palette ⌘K |
| G5 | Settings ⌘, · Toggle inspector ⌘] · Disconnect (power icon) |

- The **secondary toolbar strip is removed**. Everything on it now lives in G3/G4, the "+" menu, or the palette.
- ThemeToggle and Help leave the titlebar. Appearance moves to View ▸ Appearance and Settings; Show Tour moves to Help ▸ Show Tour.
- Capsule styling:
  - 36px tall, full radius
  - glass fill `rgba(255,255,255,.55)` with blur(20px) saturate(180%), 1px hairline border, soft shadow
  - dark mode uses `rgba(255,255,255,.06)`
  - icons 20px in a 36×32 hit area
  - active toggles get a filled circle, disabled ones render at 35% opacity
- The titlebar grows to 52px, and the macOS traffic lights are re-centred with `trafficLightPosition`.

### 2. Environment
- `ConnConfig` gains an optional `env: "local" | "development" | "testing" | "staging" | "production" | null`.
  - Rust uses `#[serde(default)]`, so old saved configs still load.
  - Connection Manager gets an "Environment" select.
- Pill tint follows TablePlus:

  | Env | Tint |
  |---|---|
  | none | neutral glass |
  | local | green |
  | development | pale yellow (the design) |
  | testing | blue |
  | staging | orange |
  | production | red |

### 3. Lobby (`!activeTab`)
- **Header:** engine tile, connection name (600, 20px), and `engine · host:port · ENV`.
- **Quick actions:** New Query ⌘T, Open Table ⌘P (opens the palette), ERD, Import CSV. They are large borderless tiles.
- **Stats row:** N tables · N views · N functions (from the loaded `SchemaModel`), then total size and largest table when sizes are known.
- **Largest tables:**
  - top 8 by bytes (or by rows when bytes are unknown), with proportional bars; clicking a row opens the table
  - the data comes from a **new `table_stats(id, schema)` command** returning `[{name, rows?, bytes?}]`
  - hidden when the engine returns nothing
- **Pinned:** the pinned tables (pins now **persist per connection in localStorage**), each with a 3-row preview fetched with `fetchTableWindow(…, 0, 3)`.
- **Recent queries:** the last 3 history entries for this connection, showing time ago and a Run button that opens a query tab with that SQL.
- **Schema at a glance:** up to 4 of the most-connected tables with their FK edges, drawn small by reusing `computeErdLayout`. "Open ERD →" opens the ERD tab. Hidden when there are no FKs.
- **Ask bar:** the prompt goes to `aiGenerateSql`, then `openQuery(sql, prompt)`. If AI isn't configured it opens Settings ▸ AI instead.

### 4. Status bars with icons
- **Table bar:**
  - 14px icons on Data | Structure, + Row, pager chevrons, rows/page, Filters, Columns and editable
  - a "N staged" chip when there are pending edits
- **Workspace bar** (lobby and ERD): `● Connected · Engine`, a tab count, and shortcut chips with icons (⌘K palette · ⌘T new query · ⌘[ sidebar · ⌘] inspector · ⌘, settings · ⌘. cancel).

### 5. Native menu (Tauri)
- Keep the default macOS app, Edit and Window menus, so copy, paste and undo keep working.
- **View** menu:
  - Toggle Sidebar ⌘[, Toggle Inspector ⌘], Show Filter Bar ⌘F
  - Data ⌥⌘1, Structure ⌥⌘2
  - Command Palette ⌘K
  - Appearance ▸ Light / Dark / System
  - Enter Full Screen
- **Help** menu: Show Tour.
- Menu events emit `tiq:menu` with the item id. The frontend `listen`s and forwards each one to the handler that already exists: the same functions the shortcuts call.

## Out of scope (already built, or not built and not faked)
- **Sidebar right-click menu:** it already exists in MainShell with Open, Copy, Pin, Export, Truncate/Drop-as-query and more, so there are no changes.
- **Not shown, because nothing measures them:** TLS version, network bytes/s and connection latency. The pill shows "SSL" when `sslmode` is require/verify-* and "SSH" when an SSH tunnel is enabled.
- **Recent-query row counts and durations:** history doesn't store them.
- **The live "query running" dot on the cancel button:** there is no cheap running signal.

## Success criteria
- Every control above works in the real app over the ipc_bridge: toggles reflect state, G2 commits and discards real staged edits, the lobby loads real stats on Postgres and SQLite, and View menu items work in the Tauri build.
- vitest and `cargo test` pass, including the new tests for `table_stats` (SQLite always, PG/MySQL gated by URL), env tint mapping, pill segments, lobby helpers, pin persistence and the staged-edits store.
- The UI matches the approved screenshots within the design system, in light and dark mode.
