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

### Running tests

```bash
cargo test
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
