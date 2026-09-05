# Contributing to Smriti

## Setup

Use [Build from Source](docs/BUILD.md) for platform dependencies and
[agents.md](agents.md) for the development workflow. Start the app with
`scripts/dev.sh` or `scripts/dev.ps1`, not the unwrapped Tauri watcher.

## Changes and tests

Keep changes focused. Add a regression test for a bug fix and update the
relevant documentation when behavior changes. Do not add abstractions,
dependencies or benchmark machinery without a concrete need.

- Rust: run `cargo fmt` and address Clippy warnings.
- Svelte/TypeScript: use precise types and match existing runes-based state.
- IPC changes: update both sides of the contract in
  [COMMAND_SURFACE.md](docs/COMMAND_SURFACE.md) and test argument envelopes.
- Data changes: test failure/recovery paths, not just successful writes.

During development, run the narrowest relevant checks. Before pushing, run:

```bash
scripts/ci_local.sh ci
```

This runs formatting, frontend checks/tests/build, strict all-target Rust
Clippy and the Rust suites. Hosted CI additionally checks the declared
MSRV and dependency policies; see [.github/workflows/ci.yml](.github/workflows/ci.yml)
for the current matrix. See [Testing](docs/TESTING.md) for test locations.
Monitor disk space and follow the build-cache policy in [agents.md](agents.md).

## Architecture

- `src/`: Rust engine, database, processing and search.
- `src-tauri/`: desktop state, IPC and job orchestration.
- `src-ui/`: Svelte frontend and typed IPC clients.

Keep engine services independent of Tauri. Extract command/route logic when
it removes duplication or makes ownership testable, not to meet a line limit.

## Pull requests

Use a focused branch, explain the behavior change and record checks performed.
Use Conventional Commit prefixes (`fix:`, `feat:`, `perf:`, `refactor:`,
`test:`, `docs:`, `chore:`) and update the changelog for user-visible changes.
Passing CI is necessary, not a substitute for review or native verification.

Report bugs with the app version, OS, reproduction steps and relevant logs.
Do not include photo data or credentials in public reports.
Follow the [Code of Conduct](CODE_OF_CONDUCT.md); contributions are
licensed under Apache-2.0.
