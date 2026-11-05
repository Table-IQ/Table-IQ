Canonical page: https://tableiq.ideolyst.com/guides/postgres-investigation

# Investigate a Postgres data issue with TableIQ

This walkthrough uses synthetic rows and the existing desktop MCP integration. It does not require a TableIQ AI-provider key: your external agent uses its own provider settings.

## Connect and investigate

1. Install [TableIQ for Mac](https://tableiq.ideolyst.com/#download).
2. Prepare a disposable PostgreSQL database and a dedicated login with only the read access needed for your task. Do not share an owner or superuser connection. For real data, have the administrator restrict accessible tables/functions and apply appropriate query limits. The synthetic query below needs no application tables or schema changes.
3. In TableIQ, open a connection with ⌘O. Enter the test connection details, test, save and connect. Never paste credentials into an agent conversation or a bug report.
4. Open Settings → MCP. Check Enable MCP server. Under Shared connections, select only the test connection and leave Allow writes unchecked. Wait for Running at …. The default endpoint is `http://127.0.0.1:7431/mcp`; use the displayed address if the port differs.
5. Under Connect an agent, copy the configuration for your client using the app's Copy button. The displayed token is masked, while the copied configuration contains the real token. Store it only in the client's local configuration; do not commit or publish it. The current UI supplies Claude Code and Claude Desktop configurations. Keep TableIQ running.
6. In your agent, list the TableIQ connections and confirm that only the intended test connection is exposed. Ask it to run the following query through TableIQ and explain which paid orders have not shipped. Require the exact SQL and result in its answer.

```sql
WITH orders (id, payment_status, shipping_status, total_cents) AS (
  VALUES
    (101, 'paid', 'shipped', 2500),
    (102, 'paid', 'pending', 4200),
    (103, 'unpaid', 'pending', 1800),
    (104, 'paid', 'pending', 3100)
)
SELECT id, total_cents
FROM orders
WHERE payment_status = 'paid' AND shipping_status = 'pending'
ORDER BY id;
```

Expected finding: orders 102 and 104, totaling 7,300 cents, are paid but pending shipment. These rows are synthetic; the query does not create a table or change the database.

7. Open MCP Activity through ⌘K. Find the agent call and inspect its SQL and outcome. Paste the SQL into a new query tab, run it yourself and compare the result grid. The agent's explanation must agree with the returned rows; do not treat its prose as database evidence.
8. For your own sanitized investigation, replace this example with a bounded read of approved tables. Identify the specific finding, inspect the SQL/results and record how this compares with your prior workflow. Do not send secrets or unrelated personal data to the agent.

### Demonstrate a refusal after the useful result

On the same disposable, read-only shared connection, request this harmless probe through MCP:

```sql
CREATE TEMP TABLE tableiq_launch_probe (id integer);
```

Expected: TableIQ returns a write refusal, and MCP Activity records it. If the agent declines to call the tool, that demonstrates agent behavior, not a TableIQ guard result. If the database returns an error instead, record that distinction. Stop and investigate an unexpected success; do not enable writes to make the demo pass. The probe intentionally avoids a destructive DELETE.

This is one observable check, not a security audit. See the [engine list](https://tableiq.ideolyst.com/#engines) and [MCP permissions](https://tableiq.ideolyst.com/guides/mcp-permissions). Read-only access can still disclose data or run expensive queries. External agents may send received data to their model provider.

## Troubleshoot MCP access

|  Symptom |  Check and recovery |
| --- | --- |

|  Cannot reach the endpoint |  Keep the native app running; confirm Settings → MCP says Running. Use its actual port. A client on another machine/container cannot reach your Mac through its own `127.0.0.1`. |
|  Unauthorized or rejected token |  Use the app's Copy button, not the masked display. After regenerating the token, replace old client configuration. Never attach a token to an issue. |
|  Empty connection list |  Check the saved connection under Shared connections. A connected GUI tab alone does not share it. Verify the agent is using the intended TableIQ endpoint. |
|  Connection cannot be shared |  Read its explanation and check the engine matrix. Database browsing support does not imply MCP support. Do not edit policy files to bypass the restriction. |
|  Write refused |  Expected with Allow writes unchecked. Keep it unchecked for this walkthrough; do not weaken access to resolve a read task. |
|  An ordinary read is refused |  The guard is conservative. Reduce the query to a minimal synthetic example and report the app/database versions and exact sanitized error. Do not enable writes as a workaround. |
|  Read returns a database permission error |  Verify the login has only the required read grants. Have the database administrator inspect access; do not switch to a superuser to bypass the error. |
|  Result is truncated or a query times out |  Inspect the tool outcome. Narrow selected columns and predicates, and use a bounded result. A row cap does not itself bound database scan cost. Do not repeatedly retry an expensive query. |
|  Claude Desktop bridge cannot start |  Its copied configuration uses `npx` and `mcp-remote`; check the client's local process error and that its environment can launch `npx`. This is not a standalone TableIQ MCP package. |
|  GUI works but the agent does not |  Test connection access and MCP configuration separately. After reconnecting or changing settings, retry a simple `SELECT 1` and inspect MCP Activity. |

For a report, include app/OS/database/client versions, redacted settings, a minimal synthetic query, the exact sanitized error and reproduction steps. Do not include passwords, bearer tokens, connection URLs with credentials or customer data. For a possible access-control bypass, contact the maintainer privately before posting sensitive details.
