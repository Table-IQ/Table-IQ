# Synthetic SQLite demo

This fixture contains no customer data or credentials. Create a **new** disposable database:

```sh
sqlite3 /tmp/tableiq-storefront.sqlite < docs/launch/demo/seed.sql
```

Add a SQLite connection to that file in TableIQ. Ask your local coding agent, through TableIQ MCP: “Inspect the schema, then find paid orders without a shipped shipment. Return their IDs, customer and total in cents. Use reads only.” The expected finding is orders **102 and 104**, totaling **7,300 cents**. The answer query is supplied in `investigate.sql` for independent verification; do not hand it to the agent as its investigation prompt.

Share only this disposable connection and keep MCP writes disabled. Inspect recent call metadata in Activity and run the answer query yourself to verify the grid. A harmless `CREATE TEMP TABLE tableiq_launch_probe (id INTEGER)` request through MCP should be refused. An agent declining to call the tool does not prove the TableIQ guard.

You can also use Finder's Open With → TableIQ for `investigate.sql` in v0.8.15 or later, select a SQL connection if asked, and verify that opening the file does not execute it. The human chooses Run. Do not run fault-injection or write probes on production.
