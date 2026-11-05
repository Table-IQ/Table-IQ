Canonical page: https://tableiq.ideolyst.com/guides/mcp-permissions

# Troubleshoot TableIQ agent permissions

Use a synthetic or sanitized database and a dedicated least-privilege login. This guide covers the desktop-hosted MCP server. TableIQ must remain open while an agent uses it.

## Establish the connection

1. Connect in TableIQ and run `SELECT 1` in a query tab. A database login error occurs before MCP permissions are involved. Test Connection uses the saved password when its input is blank; a typed password overrides it.
2. Open Settings → MCP. Enable the server, select only the intended connection, and leave Allow writes off.
3. Review any role warning. Elevated database privileges remain available to SQL functions and extensions despite application validation. An unsuccessful privilege check means unknown permissions, not a safe role.
4. Copy the configuration for your agent from TableIQ. Keep the bearer token private. Do not put it in screenshots, a repository, or a support report.
5. Ask the agent to list connections, then run `SELECT 1` against the selected connection. Inspect MCP Activity to see the query and outcome.

## Diagnose the result

   Diagnose the result
|  Result |  Action |
| --- | --- |

|  Connection refused |  Keep TableIQ running; verify MCP is enabled and the configured loopback port matches. Check Settings for a port conflict. |
|  HTTP 401 |  Copy the current configuration again. Regenerating the token invalidates old configurations. Restart/reconnect the agent's MCP client. |
|  Connection not shared |  Check that exact connection in Settings. An unshared connection intentionally looks like an unknown connection. |
|  Shared before this engine was supported |  Turn sharing off and on to confirm access under the new engine rules. This resets write access. |
|  SQL validation refusal |  Read the specific error. Use a plain, bounded SELECT on an application table. Do not enable writes to work around a refusal. |
|  Database permission denied |  Ask an administrator to grant only the required SELECT/schema access. Do not substitute a superuser login. |
|  TiDB profile mismatch |  Use the correct engine profile. A genuine MySQL server saved as TiDB is not accepted as a TiDB agent connection. |
|  TiDB clock error |  Check TiDB/PD clock synchronization. Do not enable `tidb_enable_noop_functions`; it does not enforce read-only behavior. |
|  Timeout |  Cancellation is a request, not proof of termination. Inspect the outcome and reduce query cost before retrying. Never blindly retry a write. |
|  Truncated result |  Narrow the selected columns, filter, or aggregate. A row limit does not limit how much work the database scans. |
|  MySQL CALL or SQL Server multi-result response |  Do not infer completeness from `truncated: false`. Prefer one explicit SELECT; these drivers have result-set limitations. |

## Understand each engine's controls

- PostgreSQL and CockroachDB use read-only transactions plus SQL validation. Database roles still govern access to sensitive data and server functions.
- MySQL/MariaDB use read-only transactions plus SQL validation. DDL may implicitly commit; SQL validation is part of the boundary.
- TiDB always uses the guarded `AS OF TIMESTAMP NOW(6)` read path for agents, even if an old configuration requests writes. Snapshot transactions reject DML but do not prevent DDL by themselves. Share a SELECT-only application role. Internal/system database connections are refused.
- SQLite, DuckDB, and SQL Server use SQL validation plus rollback wrappers. These wrappers are not database-enforced read-only sessions. SQL Server agent calls use separate sessions from user query tabs.
- DuckDB's agent hardening disables external access for the shared instance. Reconnecting or restoring reopens the instance; the next agent call hardens it again.

A local MCP server does not keep query results away from an external agent's provider. Share only data that provider may receive.

## Verify refusal without changing application data

On the synthetic database, ask for `CREATE TEMP TABLE tableiq_permission_probe (id integer)` with writes disabled. Expect a refusal in MCP Activity. Then run `SELECT 1` again to confirm the connection remains usable. This checks one advertised behavior; it does not prove universal safety.

For a support report, include app/OS/database/agent versions, exact sanitized errors, and a minimal synthetic query. Exclude tokens, passwords, credential-bearing URLs, and customer data.
