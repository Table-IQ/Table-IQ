# Connect your coding agent to TableIQ

TableIQ hosts a local MCP server inside the desktop application. It supports Streamable HTTP with a bearer token. Claude Code, local Codex, OpenCode and compatible MCP clients can use it; the agent does not need database passwords. This guide and the in-app setup snippets were checked against current client documentation on November 15, 2026. Client configuration shapes can change.

## Start in TableIQ

1. Open the native TableIQ app and connect to a disposable database using a least-privilege role. The browser frontend mock is not a database or MCP server.
2. Open **Settings → MCP**, enable the server, and explicitly select a connection. Keep **Allow writes** unchecked for the first task. Wait for **Running at …**; use the displayed address if you changed the default port.
3. Under **Connect an agent**, choose your client and **Copy** its setup. The on-screen preview masks the token; the clipboard contains the real token. Keep it in private user configuration, never a shared repo, screenshot or issue. Merge with your existing settings rather than replacing the entire file.
4. Restart/reconnect the client. Keep TableIQ running throughout the session. Setup copying does not edit the client's files or prove a successful connection.

There is no separate TableIQ npm MCP server to install. Claude Desktop currently uses the third-party `mcp-remote` bridge and therefore needs Node.js/npx and an initial package download. The other clients below can use HTTP directly.

## Claude Code

Select **Claude Code** and run the copied command in your local terminal. It uses the documented HTTP transport and private user scope:

```sh
claude mcp add --transport http --scope user tableiq http://127.0.0.1:7431/mcp --header 'Authorization: Bearer YOUR_TABLEIQ_TOKEN'
```

The command writes the token to your local Claude Code configuration. Copied literal-token commands may also remain in shell history; use your preferred private secret-handling approach. If `tableiq` already exists, inspect it first with `claude mcp get tableiq`; update/remove that entry deliberately rather than deleting unrelated settings. Run `/mcp` inside Claude Code to inspect the connection.

## Codex app, CLI or IDE extension on this Mac

Select **Codex**. Merge the copied block into your private `~/.codex/config.toml`, preserving all existing settings:

```toml
[mcp_servers.tableiq]
url = "http://127.0.0.1:7431/mcp"
http_headers = { Authorization = "Bearer YOUR_TABLEIQ_TOKEN" }
```

This uses native HTTP and does not require a Node bridge. The local app, CLI and IDE extension share Codex configuration. Inspect with `codex mcp get tableiq` / `codex mcp list` and restart the relevant client after changes. The CLI configuration shape was accepted by the installed Codex CLI using a dummy token and read-only overrides; that check is not a real connection test with your database.

Codex may ask for approval before `run_query`, even when TableIQ has writes disabled. Approve an intended read in the client; do not turn on TableIQ writes to resolve a client approval request. Noninteractive runs with an approval policy of never can refuse the call before TableIQ receives it.

For an environment-based setup instead of storing the token in TOML, use the supported CLI option:

```sh
codex mcp add tableiq --url http://127.0.0.1:7431/mcp --bearer-token-env-var TABLEIQ_MCP_TOKEN
```

Provide `TABLEIQ_MCP_TOKEN` in the environment of the process that starts Codex. A GUI application launched from Finder does not necessarily inherit a terminal's exports; use the private-header setup when that is more appropriate. Neither approach uses OAuth: TableIQ's token comes from its own MCP settings.

## OpenCode 2

Select **OpenCode 2**. Merge the copied configuration into your private `~/.config/opencode/opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "tableiq": {
        "type": "remote",
        "url": "http://127.0.0.1:7431/mcp",
        "oauth": false,
        "headers": { "Authorization": "Bearer YOUR_TABLEIQ_TOKEN" }
      }
    }
  }
}
```

OpenCode's `remote` type means the HTTP transport; the endpoint is still on your own Mac. V2 nests named servers under **`mcp.servers`**. Keep `oauth: false` because TableIQ uses its bearer token. Restart OpenCode and check `opencode mcp list` or `/mcps`.

You can replace the literal header value with `Bearer {env:TABLEIQ_MCP_TOKEN}` if that environment variable is supplied to OpenCode. Do not paste a real token into project-level `opencode.json` that will be committed.

### OpenCode 1 (legacy)

Select **OpenCode 1 (legacy)** only if you are still using v1. Its named server goes directly under `mcp.tableiq`, not `mcp.servers.tableiq`. The in-app choice generates that older shape explicitly; do not use it for v2. The current installed OpenCode checked during preparation was v2.2.7.

## Claude Desktop

Select **Claude Desktop** and merge the copied entry into its local `claude_desktop_config.json`. The bridge configuration has this shape:

```json
{
  "mcpServers": {
    "tableiq": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://127.0.0.1:7431/mcp", "--allow-http", "--header", "Authorization:${AUTH_HEADER}"],
      "env": { "AUTH_HEADER": "Bearer YOUR_TABLEIQ_TOKEN" }
    }
  }
}
```

The env placeholder preserves the space in the bearer header when the bridge parses arguments. The copied configuration contains the actual token, not the masked preview. Ensure the desktop client's environment can run `npx`; an initial download may need network access. Restart Claude Desktop and inspect its MCP connection status.

## Verify the connection before investigating real data

Ask your agent to list TableIQ connections. Confirm it sees only the connection you selected. Then ask it to run a bounded read against your synthetic data and explain the answer. Open **MCP Activity** to inspect the call's redacted SQL, row count, timing and outcome, and independently verify the finding in the query grid. A write refusal should be demonstrated only on disposable data with a harmless probe from the [Postgres walkthrough](POSTGRES-WORKFLOW.md).

Activity is bounded process-local metadata, not a saved full result set or permanent audit trail. Manual grid edits are staged for human review; MCP writes are not automatically placed in that queue. Read-only access can still disclose sensitive information or run expensive queries. An external agent may send what it reads to its model provider.

After rotating the token in TableIQ, update your client's private configuration and reconnect. Old tokens must be refused. If an ordinary read is refused, report a minimal sanitized example rather than enabling writes as a workaround.

## Cloud, SSH and container limitations

`127.0.0.1` refers to the machine/network namespace running the agent. A hosted Codex task, a remote OpenCode server, SSH host or isolated container cannot automatically reach TableIQ on your Mac. These setup snippets are for clients running locally alongside TableIQ. They do not create a cloud connector, tunnel or remotely reachable service. Do not advertise a hosted installation or expose the local server publicly to make the snippet work.

## Documentation sources

- [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp): HTTP headers, bearer environment configuration and shared local configuration.
- [Claude Code MCP](https://code.claude.com/docs/en/mcp): HTTP transport, headers and user scope.
- [OpenCode MCP](https://opencode.ai/docs/mcp-servers/): current v2 nesting, headers, disabled OAuth and connection checks.

No client login, global configuration mutation or real provider query was performed by adding these helpers. Generated setup is ready to copy; each actual installation should complete the connection checks above.
