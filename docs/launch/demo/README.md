# Synthetic SQLite demo

This fixture contains no customer data or credentials. Create a **new** disposable database:

```sh
sqlite3 /tmp/tableiq-storefront.sqlite < docs/launch/demo/seed.sql
```

## Try the database client first

1. Add a SQLite connection to the new file in TableIQ.
2. Inspect the tables and columns in the sidebar.
3. Open `investigate.sql` in the SQL editor, or use Finder's Open With → TableIQ in v0.8.15 or later. Opening a script never executes it.
4. Review the query, then choose Run. The expected finding is orders **102 and 104**, totalling **7,300 cents**.

This path needs neither AI nor MCP. Use it beside your current client to judge one familiar database task. Settings → AI is optional; provider setup alone does not verify a generated answer.

## Optional: investigate with a coding agent

Ask your local coding agent, through TableIQ MCP: “Inspect the schema, then find paid orders without a shipped shipment. Return their IDs, customer and total in cents. Use reads only.” The answer query is supplied in `investigate.sql` for independent verification; do not hand it to the agent as its investigation prompt.

Share only this disposable connection and keep MCP writes disabled. Inspect recent call metadata in MCP Activity and run the answer query yourself to verify the grid. A harmless `CREATE TEMP TABLE tableiq_launch_probe (id INTEGER)` request through MCP should be refused. An agent declining to call the tool does not prove the TableIQ guard. Do not run write or fault-injection probes on production.
