# Contributing to PhotoVault

Thanks for your interest. Contributions of all sizes are welcome.

## Quick start

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR-USERNAME/photovault.git`
3. Install Rust 1.75+: https://rustup.rs/
4. Set up assets:
   - Linux: `./scripts/setup_assets.sh`
   - Windows: `powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1`
5. Build and run: `cargo run`
6. Create a feature branch: `git checkout -b my-feature`
7. Make your changes, run checks, and open a PR

## Where to find work

- Good first issues: `good first issue` label
- Future feature ideas: `docs/FUTURE_SCOPE.md`
- Documentation improvements are always welcome

## Development

### Code style

- Run `cargo fmt` before committing
- Run `cargo clippy --all-targets` for linting (CI runs full strict profile)
- Write tests for non-trivial logic
- Keep commits focused -- one logical change per commit

### Commit message style

We follow [Conventional Commits](https://www.conventionalcommits.org).
The `release-plz` automation reads commit prefixes to decide version
bumps and to generate the changelog, so the prefix matters.

| Prefix      | Meaning                          | Example                                                 |
|-------------|----------------------------------|---------------------------------------------------------|
| `feat:`     | User-visible new feature         | `feat(timeline): virtualize scroll for 100K libraries`  |
| `fix:`      | Bug fix                          | `fix(reindexer): hash full file instead of 64 KB prefix` |
| `perf:`     | Performance improvement          | `perf(clustering): skip Stage B above 2000 faces`       |
| `docs:`     | Documentation only               | `docs(privacy): disclose update-check endpoint`         |
| `refactor:` | No behavior change               | `refactor(db): extract timeline_group into models`      |
| `test:`     | Test-only changes                | `test(scale): assert 50K compute_groups <100 ms`        |
| `chore:`    | Tooling, deps, CI, housekeeping  | `chore(deps): bump lru 0.12 → 0.16`                     |

Commits that don't match a prefix are still merged fine — they just
won't appear in the generated changelog. Breaking changes go in the
body of the commit message with a `BREAKING CHANGE:` token.

### Running tests

```bash
cargo test
```

### CI gates (required to merge)

Every PR must pass these checks before it can be merged into `master`:

| Check  | What runs                                  | Where                  |
|--------|--------------------------------------------|------------------------|
| Format | `cargo fmt --all --check`                  | `ci.yml` `fmt`         |
| Lint   | `cargo clippy --all-targets`               | `ci.yml` `quality` × 3 |
| Tests  | `cargo test`                               | `ci.yml` `quality` × 3 |
| MSRV   | `cargo check --all-targets` on Rust 1.75   | `ci.yml` `msrv`        |
| Audit  | `cargo audit` (RUSTSEC advisories)         | `ci.yml` `audit`       |
| Deny   | `cargo deny check` (licenses + bans)       | `ci.yml` `deny`        |

The `quality` job runs on Linux, Windows, and macOS. All six checks must
be green; the maintainer will not merge a red PR. Run them locally before
pushing — debugging in CI is slower than running locally.

```bash
cargo fmt --all --check
cargo clippy --all-targets
cargo test
cargo +1.75 check --all-targets   # MSRV
cargo audit                       # if installed
cargo deny check                  # if installed
```

### Architecture overview

- `src/db/` -- SQLite schema and repositories
- `src/services/` -- business logic and pipelines
- `src/ml/` -- ONNX model wrappers
- `src/app/` -- state machine and handlers
- `src/views/` -- UI rendering per screen
- `src/components/` -- reusable UI primitives

### Critical iced rules

- Never use `height(Length::Fill)` inside a vertical `scrollable`.
- A button without `on_press` is disabled and blocks child events.
- Photo detail has an early-return render path in `src/app/views.rs`; overlays must be handled there too.

## Pull request guidelines

- One feature/fix per PR
- Update docs when behavior changes
- Add tests where practical
- Update `CHANGELOG.md` under `[Unreleased]`
- Reference related issues (e.g. `Closes #123`)

## Reporting bugs

Use the bug report template and include:
- OS and version
- PhotoVault version
- Repro steps
- Expected vs actual behavior
- Relevant logs

## Code of conduct

This project follows [Contributor Covenant](CODE_OF_CONDUCT.md).
By participating, you agree to its terms.

## License

By contributing, you agree your contributions are licensed under
Apache-2.0 (same as the project).
