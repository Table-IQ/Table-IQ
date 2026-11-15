# Contributing to TableIQ

Thanks for helping make everyday database work more reliable. Focused fixes, reproducible bug reports, documentation and verification of database profiles are welcome.

## Before opening an issue

Search existing issues and include your TableIQ version, OS/architecture, database engine/version, expected behavior, actual behavior and minimal reproduction. Use synthetic data. Do not attach passwords, bearer tokens, connection URLs containing credentials or private results. For vulnerabilities, follow [SECURITY.md](SECURITY.md).

The Mac developer preview is unsigned and not notarized. Platform/engine support varies; consult the [README](README.md) and [engine matrix](docs/ENGINES.md). A compatibility profile does not guarantee every feature works on the hosted product.

## Development

Prerequisites and database fixtures are in [README development](README.md#development). The frontend uses React/TypeScript and the desktop backend uses Rust/Tauri.

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` alone uses frontend mocks; it is not proof that the native database workflow works. Marketing-site instructions are in [website/README.md](website/README.md).

## Pull requests

Keep each change focused. Explain the concrete problem, resulting behavior, meaningful validation and known limits. Discuss large feature/driver proposals in an issue before investing heavily. Do not reformat or rename unrelated code. Add regression coverage for behavior changes; avoid tests that merely duplicate implementation.

Run the relevant checks:

```sh
pnpm exec tsc --noEmit
pnpm test
pnpm build
```

For backend changes, from `src-tauri/`:

```sh
cargo test --locked --lib
cargo clippy --locked --all-targets -- -D warnings
```

Live integration suites need the documented disposable databases. State which engines actually ran and which were skipped. Never test destructive operations against production. File opening, keychain, menus, dialogs and packaging may require native macOS verification beyond browser mocks.

## License and attribution

Contributions to TableIQ-owned application code are made under AGPL-3.0-only unless the affected files explicitly carry another license. Only contribute material you have the right to license; retain required attribution for third-party work. This contribution policy does not assign your copyright or grant an unrestricted right to relicense it. See [LICENSING.md](LICENSING.md) and [LICENSE-EXCEPTIONS.md](LICENSE-EXCEPTIONS.md).

The maintainer may request changes or decline work outside the current focus. Response times depend on maintainer availability; there is no support SLA for the free preview. Follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
