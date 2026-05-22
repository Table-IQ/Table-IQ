# Spec — Command palette (Cmd-K) + FK navigation (Slice 8)

## Goal
Keyboard-first navigation: Cmd-K opens a fuzzy palette to jump to any table/view or run an action; foreign-key chips in the table header navigate to referenced tables.

## Design
- `src/lib/fuzzy.ts` — `fuzzyScore` (ordered-subsequence match with consecutive + word-boundary bonuses, `null` for non-match) and `fuzzyFilter` (rank + drop). Pure, unit-tested.
- `CommandPalette.tsx` — input + ranked list, ↑/↓ to move, Enter to run, Esc/backdrop to close. Commands are `{id,label,hint,run}` supplied by the shell.
- `MainShell` — global Cmd-K/Ctrl-K listener toggles the palette; builds commands from the schema (every table/view → openTable) plus actions (New Query, ERD, New Table). A "Search ⌘K" button mirrors it.
- FK navigation: `src/lib/fk.ts` `outgoingForeignKeys(fks, table)` (tested); `TableView` fetches `foreign_keys`, filters to the current table, and renders `→ <table>` chips that open the referenced table.

## Testing
- vitest: fuzzy non-match/subsequence/empty/ranking; fuzzyFilter ordering + drop; outgoingForeignKeys filtering. Build (tsc) verifies wiring.
