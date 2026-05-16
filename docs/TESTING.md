# Testing in Smriti

This document explains how Smriti is tested and how *you* should test
the code you add. Tests are the contract: if your PR's tests pass,
your code lands; if they fail, the failure is the conversation.

## Pyramid

```
                ┌──────────────────────────────┐
   Level 4      │  Frontend unit (Vitest)      │   per util / per store
                └──────────────────────────────┘
                ┌──────────────────────────────┐
   Level 3      │  Workflow integration (Rust) │   one per critical user journey
                └──────────────────────────────┘
                ┌──────────────────────────────┐
   Level 2      │  DTO snapshots + service     │   wire-format + business logic
                │  integration (Rust)          │
                └──────────────────────────────┘
                ┌──────────────────────────────┐
   Level 1      │  Unit tests (Rust + TS)      │   per pure function
                └──────────────────────────────┘
```

All four levels run on every PR. CI blocks merge if any test fails.

## What we don't test (and why)

- **The Tauri runtime itself.** Commands like `library_close` are
  thin wrappers: `state.library.write().await; *guard = None;`.
  Testing through Tauri's IPC layer needs `mock_app()`, which on
  some platforms (Windows specifically) hits Tauri runtime
  initialization bugs in the test binary. Instead, we test:
  - The engine service the command wraps (Level 3 workflow tests)
  - The DTO it returns (Level 2 snapshot tests)
  - The args struct it takes (free via serde tests if anything funky)

  This means commands should be **thin**: the body is one call to a
  service. If you find yourself writing branching logic inside a
  `#[tauri::command]`, push that logic down into a service and call
  the service from the command. The service tests cover it.

- **End-to-end browser flows.** No tauri-driver / WebDriverIO yet.
  The pyramid above is robust enough that runtime bugs are caught
  at Level 3 before they hit the UI. We may add browser tests
  later for the most-trafficked workflows, but they're not a
  contributor obligation today.

## Where tests live

```
tests/                                # Engine integration tests
├── common/mod.rs                     # Shared helpers (image gen, library factory)
├── common_smoke.rs                   # Smoke tests for the helpers themselves
├── db_integration.rs                 # DB schema, photo repo, queries
├── face_clustering_integration.rs    # Face pipeline w/ synthetic embeddings
├── thumbnail_pipeline.rs             # Thumbnail gen across formats
├── timeline_scale.rs                 # 50k-photo SLA test (ignored by default)
└── workflow_*.rs                     # One file per critical user journey

src-tauri/tests/
└── dto_snapshots.rs                  # Wire-format contract — every From<E> for Dto

src-ui/src/lib/
├── slideshowQueue.test.ts            # Vitest — pure logic
└── thumbnailQueue.test.ts            # Vitest — pure logic
```

## Conventions

### Hard rules (CI-enforced)

1. **`cargo fmt --all --check` passes.**
2. **`cargo clippy --all-targets -D warnings` passes.**
3. **`cargo test` passes** (engine + Tauri integration tests).
4. **`npm run check`** passes (Svelte + TypeScript type-checking).
5. **`npm run test`** passes (Vitest).
6. **DTO snapshot changes are intentional.** When `dto_snapshots.rs`
   fails, you must either revert the unintended change or run
   `cargo insta review` to accept the new wire format. The diff must
   be reviewed by a maintainer.

### Soft rules (reviewers apply)

- **Test names follow `<subject>_<scenario>_<expected>`.** E.g.
  `geocoding_rural_photo_returns_none`,
  `slideshow_next_after_last_when_loop_off_pauses`. A reviewer
  reading test names alone should understand what's being checked.
- **Arrange / Act / Assert with comments** separating the three.
  Long setup gets its own helper in `tests/common/mod.rs`.
- **Hermetic.** No shared mutable state. No tests that depend on
  test order. `tempfile::TempDir` for the filesystem; in-memory
  SQLite or per-test tempdir for the DB.
- **No `thread::sleep` for synchronisation.** Use channel sends,
  atomic flags, `tokio::time::timeout`. A test that sleeps will
  flake under CI load.
- **Rust: prefer `assert!(... , "<why>")`** — the message shows up
  in the failure output and saves a reviewer from `git blame`.
- **TS: prefer `expect(...).toEqual(...)` with descriptive `describe`
  + `it` text** that reads like a sentence.

## Decision tree: "what kind of test do I write?"

```
Did you add a pure function?
├── yes → unit test next to it (same file's `#[cfg(test)] mod tests`
│         for Rust, sibling `.test.ts` for TypeScript).
└── no  → continue
Did you add a `#[tauri::command]`?
├── yes → make sure the body is a single call to a service. Test
│         the service in `tests/workflow_*.rs` or via direct
│         service-level unit test. If the command returns a new DTO
│         shape, add a snapshot in `src-tauri/tests/dto_snapshots.rs`.
└── no  → continue
Did you add a new service or change an existing service's contract?
├── yes → integration test in `tests/` that constructs a tempdir
│         library, exercises the service, asserts DB / file state.
└── no  → continue
Did you add or rename a `From<EngineType> for SomethingDto`?
├── yes → add a snapshot test in dto_snapshots.rs. Construct the
│         engine type with deterministic values, convert, snapshot.
└── no  → continue
Did you fix a bug?
└── yes → add a regression test that fails *before* the fix and
          passes after. Reviewer will check this is true by reverting
          your fix locally and re-running the test.
```

## Running tests

```bash
# Everything CI runs:
scripts/ci_local.sh ci

# Just the Rust suite (engine + tauri):
cargo test -p smriti -p smriti-tauri

# Just the engine:
cargo test -p smriti

# A single test file:
cargo test -p smriti --test workflow_scan_pipeline

# Just one test by name:
cargo test -p smriti scanner_picks_up_supported_extensions

# Vitest (frontend):
cd src-ui && npm run test            # one-shot
cd src-ui && npm run test:watch      # repl

# When a snapshot intentionally changes:
cargo insta review                   # interactive, per-snapshot
# OR:
INSTA_UPDATE=always cargo test       # accept all
```

## Fixtures

- **Synthetic images** are built in-memory by helpers in
  `tests/common/mod.rs`. The test runner generates a valid JPEG /
  PNG / TIFF in a tempdir. No license issues, no large files in
  git.
- **No real photo fixtures yet.** If you need to test a real-RAW
  edge case, prefer a synthetic NEF (via `common::write_nef`) over
  shipping a 30 MB binary in the repo. We may add a small set of
  CC0-licensed reference photos later if synthetic generators
  don't cover a real scenario.

## When tests are slow

If your test takes longer than ~1 second, ask:

- Is it doing actual image decoding? Try a 16×16 image, not 4000×3000.
- Is it spawning subprocesses? Don't.
- Is it sleeping? See "no `thread::sleep`" above.
- Is it the only test in its target? Consider grouping related
  tests into one integration file — each `tests/<name>.rs` is a
  separate binary that links the whole engine, and linking is
  expensive.

The 50k-photo `timeline_scale.rs` test is marked `#[ignore]` so it
doesn't run by default. Heavyweight benchmarks live in `benches/`
and run only nightly.

## Snapshot tests in detail

`src-tauri/tests/dto_snapshots.rs` contains one test per
`From<EngineType> for SomethingDto` impl in `src-tauri/src/dto.rs`.
The first time you run it, insta writes `.snap.new` files into
`src-tauri/tests/snapshots/`. Run `cargo insta review` to accept
them; the accepted files get committed.

When you change a DTO field (rename, type change, addition,
removal), the test for that DTO fails with a unified diff showing
old vs new. The contributor reviews their own change, runs
`cargo insta review` if intentional, and commits the updated
`.snap` file alongside the source change. The reviewer reads the
`.snap` diff in the PR to sanity-check the wire-format implications
at a glance.

If a DTO change *isn't* intentional (a typo, an accidental
field-rename during a refactor), the failing test catches it
before the frontend breaks at runtime.
