# Try one task beside your current database client

Draft, November 15, 2026. Source-inspected setup route; verify on the exact release build before advertising it as a tested migration guide. Keep your current client configured. This is a low-risk evaluation, not a requirement to move every connection or abandon a working tool.

## Choose a small task

Use disposable PostgreSQL data or a sanitized copy. Pick a task you already do: filter records and run a saved query, or investigate a known discrepancy with your existing agent. Use the same starting dataset for comparisons. Keep admin/superuser credentials and real customer data out of recordings and feedback.

Record which current tool you use, what feels slow or awkward, and the one behavior you cannot lose. Persistent per-tab transactions/session state are currently absent in TableIQ; a workflow depending on that behavior is not a successful migration candidate today.

## Bring one connection

1. Download the actual available TableIQ release and read its platform/signing/preview limitations. A public signed release is not established by this draft; use the README's current verified route.
2. In TableIQ's connection setup, enter the test database details manually or paste a supported connection URL locally. Never paste it into a chat, issue or shared document. Confirm host/database/TLS/SSH settings and test the connection before saving.
3. Re-enter secrets using TableIQ's normal password/keychain path. Do not extract or upload another application's credential store. Check the exact authentication and tunnel options you need; not every existing client's configuration has an equivalent.
4. TableIQ's own connection-bundle import/export is for its supported JSON format. It is **not** a verified importer for `.tableplusconnection` or DBeaver exports. Recipients re-enter passwords. Bundles may still contain infrastructure metadata, so inspect/redact before sharing.
5. Bring only the saved SQL needed for the task using ordinary SQL text and the editor's supported workflow. Check database/schema names, parameters, encodings and dialect differences rather than assuming an imported file behaves identically.

## Compare an ordinary-client task

On synthetic data: find/filter a known record, run the relevant SQL, stage a harmless value change, inspect the generated SQL before commit, commit and verify the result. Check the keyboard actions, grid/data types and recovery behavior that matter to you. Record correct completion and elapsed time; record any coaching.

Use the same query limits and task conditions in the old client. Do not interpret server execution time alone as client responsiveness. If TableIQ requires more work or lacks something essential, record that as a reason to stay with the incumbent.

## Compare an agent investigation

Follow [agent setup and permissions](POSTGRES-WORKFLOW.md) on one explicitly shared connection, with writes disabled and a least-privilege role. The existing CTE example is a transport check. For a meaningful investigation, use the release's tested seeded case and let the agent inspect schema and form SQL.

Inspect the recent Activity entry and independently verify the finding in the query grid. Activity holds bounded, redacted metadata—not historical full result sets. Grid editing is a separate human review flow; MCP writes are not automatically staged for approval.

## Decide whether to return

At the end of the first task, record:

- Did you complete it correctly without help? Where did setup or unfamiliar behavior cost time?
- Which exact task, if any, would you choose TableIQ for tomorrow?
- What prevents using it again: authentication, session semantics, editor, data types, trust or no meaningful benefit?

After 8–14 days, check actual repeat use rather than the first impression. Keep your old client available throughout. Send minimal synthetic reproduction steps for bugs, without credentials or private rows. Keeping both clients is a valid evaluation outcome.
