# tableX contributor guide

## Project overview

tableX is a Rust 2024 desktop database-management application built with GPUI
and `gpui-kit`. PostgreSQL is the first supported database. The application is
designed to support multiple PostgreSQL server versions through the connection
and provider boundaries rather than version-specific UI code.

Keep this file repository-portable. Machine-specific paths, credentials, local
tool configuration, and personal workflow instructions do not belong here.

## Project structure

- `src/domain/` contains database-independent models and application concepts.
- `src/infrastructure/` contains PostgreSQL access, connection persistence,
  credential storage, TLS setup, and infrastructure errors.
- `src/ui/` contains GPUI views, screens, forms, notices, and interaction
  state. UI code should depend on domain/infrastructure interfaces, not embed
  PostgreSQL connection logic.
- `Makefile` contains the supported local development and verification
  commands.
- `.zed/` contains checked-in editor/project configuration.

When adding a feature, place code in the narrowest appropriate layer. Do not
move database access into views just to simplify a screen implementation.

## Architecture rules

- Keep PostgreSQL-specific behavior behind the infrastructure/provider layer.
- Keep blocking or network database work off the GPUI UI thread.
- Prefer typed domain values and explicit error types over stringly-typed state.
- Do not introduce a mock provider into the production application graph. Test
  doubles belong in tests or explicit development-only wiring.
- Preserve support for multiple PostgreSQL versions. Avoid relying on a server
  feature unless its minimum supported version and fallback behavior are clear.
- Keep connection testing separate from saving connection metadata. A failed
  test must not overwrite a working saved configuration.

## Credentials, TLS, and connection safety

- Never store passwords in the connection JSON/file store.
- Use the operating-system credential store for passwords and keep secrets out
  of logs, errors, screenshots, fixtures, and test output.
- Treat the selected SSL mode as an explicit user choice. The supported modes
  must have clear labels and descriptions in the UI.
- `require` must validate the TLS certificate. Do not silently downgrade to
  plaintext after a certificate or TLS handshake failure.
- Surface certificate failures, such as an unknown issuer, without suggesting
  that validation can be bypassed accidentally. Explain the likely remediation
  in user-facing copy where possible.
- Quote identifiers and bind values safely. Never concatenate user-provided
  values into SQL when parameters or identifier quoting are available.
- Do not add destructive database operations without explicit product scope,
  confirmation UX, and tests for the failure path.

## Query and mutation boundaries

- Read operations should use a single statement where practical, a bounded
  result set, a bounded query/input size, and a timeout.
- Keep the UI responsive while loading schemas, tables, query results, or
  connection metadata. Represent loading, empty, success, and failure states
  explicitly.
- Mutations require explicit confirmation and must have clear transaction
  behavior. Make commit, rollback, and partial-failure behavior observable.
- Avoid presenting fabricated recent activity, table counts, or connection
  state. Empty states should say what action will produce real data.

## UI and UX conventions

- Use `gpui-kit` components as the base for controls, forms, alerts, buttons,
  selects, and layout primitives. Extend the existing visual language before
  creating a one-off control.
- Prefer a focused desktop workspace: clear page title, connection context,
  primary action, and useful empty state without unnecessary navigation chrome.
- Connection forms should make host, port, database, user, password, and SSL
  mode easy to scan. Group related fields and preserve entered values after a
  failed test.
- The primary connection actions are distinct: `Test connection`, `Save`, and
  `Cancel`. Testing must not imply that the connection was saved.
- Notifications should have a short human-readable summary and optional
  technical detail. Use inline alerts for actionable form errors and reserve
  transient notices for completed actions.
- Error copy should identify the failed action, explain the likely cause, and
  provide the next safe action. For example, distinguish an invalid certificate
  from invalid credentials or an unreachable host.
- Data-grid headers should distinguish column names from type metadata, show
  the active sort direction clearly, and expose sort controls only when the
  current result source can apply the sort. Truncated names must retain a way
  to discover the full value.
- Keep header cells, row cells, row-number gutters, and trailing action columns
  aligned at every horizontal scroll position. Header and row rendering must use
  the same column widths and visibility rules.
- Keep table previews responsive with virtualized row rendering and bounded
  eager result views. Cache column layout derived from a result set instead of
  rebuilding it on every render; keep database work off the UI thread.
- Keep destructive or security-sensitive actions visually and semantically
  distinct from routine navigation.
- Preserve keyboard access, visible focus, readable contrast, and clear labels
  for every interactive control.

## Development and verification

Use the Makefile targets where available:

```text
make fmt
make fmt-check
make check
make test
make verify
make build-release
```

Before handing off a Rust change, run formatting, `cargo check`, relevant
tests, and Clippy when available. Also run `git diff --check` to catch whitespace
errors. Report each check separately; do not describe an unrun check as passed.

The PostgreSQL smoke test is opt-in and requires a dedicated test server:

```text
make test-postgres \
  TABLEX_TEST_PG_HOST=... \
  TABLEX_TEST_PG_PORT=... \
  TABLEX_TEST_PG_DATABASE=... \
  TABLEX_TEST_PG_USER=...
```

Keep local unit-test results, live PostgreSQL results, and manual UI/browser
verification clearly separated. Never claim a live connection was verified
without configured server credentials and observable test output.

## Change discipline

- Preserve unrelated worktree changes. Inspect the current diff before editing
  overlapping files.
- Keep patches narrow and update tests when behavior changes.
- Prefer `apply_patch` for focused source and documentation edits.
- Do not commit credentials, database dumps, generated build artifacts, or
  machine-specific editor state.
- When changing connection behavior, test success, invalid credentials,
  unreachable host, TLS handshake failure, unknown certificate issuer, and the
  selected SSL mode.
- When changing notification or error UX, verify that the summary is concise,
  technical details remain available, and the form state is recoverable.

## PostgreSQL feature checklist

For new PostgreSQL functionality, confirm:

1. The provider boundary supports the required server-version behavior.
2. Connection and query work remains off the UI thread.
3. Credentials and TLS handling preserve the rules above.
4. Loading, empty, success, and failure UI states are implemented.
5. Errors are actionable and do not expose secrets.
6. Unit tests cover pure logic and integration tests are opt-in when they need
   a real PostgreSQL server.
