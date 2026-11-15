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

Open contribution pull requests against **`main`**. Use a separate branch for each
change, such as `fix/sql-file-opening`, `feat/query-history` or `docs/connection-setup`.
External contributors should fork the repository, create the branch in their fork
and open a PR from that branch into `Table-IQ/Table-IQ:main`. Maintainer work follows
the same branch-and-PR process. There is no permanent `dev` integration branch.

`main` is protected by the [Protect main ruleset](https://github.com/Table-IQ/Table-IQ/rules/27317240):

- Changes must go through a pull request; direct pushes are blocked.
- The branch must be current with `main`, and GitHub Actions checks `web`, `rust`,
  `http-engines` and `search-engines` must pass.
- Review conversations must be resolved before merging.
- PRs use squash merge. Force pushes and branch deletion are blocked, with no
  configured bypass actors. Merged source branches in this repository are deleted
  automatically; your fork remains under your control.

The maintainer reviews external contributions and controls merging. The repository
currently has one maintainer, so a second person's approval is not mechanically
required: GitHub does not allow authors to approve their own PRs. CI and the PR
requirement still apply to the maintainer. When another trusted maintainer gains
write access, enable at least one required approval before broadening merge access.
`CODEOWNERS` requests the current maintainer's review on ready external PRs.

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
