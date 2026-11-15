# Security policy

TableIQ is a desktop database client. A preview label does not reduce the importance of preventing credential disclosure, unauthorized access or data loss.

## Supported releases

The current Mac developer-preview release receives fixes on a best-effort basis. Older previews may not receive backports. No maintenance window or response SLA is promised. Check the version and known limitations before reporting.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/Table-IQ/Table-IQ/security/advisories/new) when enabled. If that route is unavailable, use a private contact route on the [maintainer profile](https://github.com/RudreshNarwal) or GitHub Support to arrange secure contact. Do not publish an exploit or credentials in a normal issue while trying to reach the maintainer.

Include affected versions, platform/database engine, expected/observed behavior, impact and a minimal synthetic reproduction. Redact credentials and customer data. If relevant, distinguish a refusal before execution, a database error, timeout/cancellation request, and an observed committed effect.

Do not test on systems or data you do not have permission to use. The project does not run a paid bug-bounty program. The maintainer will assess reports and coordinate disclosure as capacity permits.

## Important boundaries

- The developer preview is unsigned/not notarized; its release includes checksums and matching source. Checksums identify the downloaded bytes and do not establish publisher identity.
- MCP listens locally, requires a token and exposes selected connections. Use least-privilege database roles. Read-only access can still disclose sensitive information or run expensive queries.
- Agent/model providers may receive context/results. A local endpoint does not imply that all data stays on the Mac.
- MCP Activity is bounded local metadata, not a durable tamper-resistant audit log or stored historical result set.
- Safeguards differ by engine. See [permissions](docs/launch/MCP-PERMISSIONS.md), [engine matrix](docs/ENGINES.md) and [README limitations](README.md#known-limitations).

Routine bugs should use the issue templates with synthetic data.
