# Contributing to Smriti

Welcome — and thank you for considering a contribution. Smriti is
maintained primarily by non-Rust developers, so the contribution
flow is built around **tests as the contract**. If your PR's tests
pass, your code lands. If they don't, the failure is the
conversation, not a code-style debate.

## Before you open a PR

1. **Read [TESTING.md](./TESTING.md).** It tells you what kind of
   test to write for what kind of change. The decision tree is the
   thing you should optimise for.
2. **Run `scripts/ci_local.sh ci` locally.** This mirrors the
   GitHub Actions pipeline. If it passes here, it will pass on CI.
3. **Set up the dev workflow:** see [agents.md](../agents.md) for
   one-time toolchain installation (Rust, Node, WSL on Windows).

## Repository structure quick-tour

```
src/                 # The engine crate (`smriti`). Pure Rust, no Tauri.
src-tauri/           # The Tauri shell crate (`smriti-tauri`). Thin
                     # wrappers around services — every command is
                     # just `let r = service::do_x(...); Ok(r.into())`.
src-ui/              # Svelte 5 frontend. Talks to Rust via Tauri IPC.
docs/                # CONTRIBUTING.md, TESTING.md (you are here).
tests/               # Engine integration tests + helpers.
benches/             # Criterion benchmarks. Not run on PRs.
```

## What CI runs on every PR

```
1. cargo fmt --check
2. cargo clippy --all-targets -D warnings
3. cargo test -p smriti -p smriti-tauri
4. npm run check --prefix src-ui     # svelte-check + typescript
5. npm run test  --prefix src-ui     # vitest
6. cargo audit + cargo deny           # supply-chain
```

Each is a hard gate. Total wall time on a runner: ~10-15 minutes.

## What "tests" you owe

The PR template asks you to tick which tests you added. Pick from:

- **Unit test** — for a new pure function. Lives next to the code
  (`#[cfg(test)] mod tests` in Rust, sibling `.test.ts` for TS).
- **DTO snapshot** — for a new or changed `From<EngineType> for
  SomethingDto`. Add to `src-tauri/tests/dto_snapshots.rs` and run
  `cargo insta review` to accept the new snapshot.
- **Service / workflow integration** — for new service logic or
  changes that affect a user journey (scan, geocode, face detect,
  search, trash, etc.). Add to `tests/workflow_<journey>.rs` if it
  doesn't already exist.
- **Regression test** — for a bug fix. The test must fail before
  your fix and pass after. Reviewer verifies by reverting your fix
  locally.

**If you can't tick any of these**, the PR template asks you to
explain why. Common valid reasons: "docs-only", "renamed an
internal symbol with no public effect". Suspicious reasons: "tests
were too hard to write" — that usually means the design is too
tangled and the fix is to detangle, not skip tests.

## Style: low-level

- **Rust**: `cargo fmt`. Clippy is configured to deny warnings. Match
  the surrounding code's idiom — comments explain *why*, not
  *what*. Match the existing async/await pattern (tokio, no
  blocking calls inside `async fn`).
- **TypeScript / Svelte**: follow svelte-check; type things
  precisely. Prefer `$state`/`$derived` over manual subscription.
- **Commit messages**: imperative, summary line < 70 chars. Body
  wrapped at 72. Explain the *why* in the body.

## Style: high-level

- **Add only what the task requires.** No premature abstractions,
  no half-finished alternatives. If you find yourself adding
  scaffolding "for the future feature", the future feature can add
  its own scaffolding when it lands.
- **Tauri commands are thin.** If you write branching logic inside
  a `#[tauri::command]`, push it down into a service. Commands
  exist for IPC plumbing; services exist for behaviour. Tests live
  with services.
- **Don't add dependencies casually.** Each new dep is a long-term
  maintenance burden + supply-chain surface. A standalone Rust
  module in `src/` is cheaper than a crate that does 90% of what
  you need but pulls in 30 transitive deps.

## Reviewer's playbook (for the maintainer)

1. **CI is green?** If no, the PR is blocked until it goes green.
2. **PR template ticked?** Confirm each ticked box is true by
   looking at the diff:
   - "Unit test added" → there's a new `#[test]` or a new
     `it(...)` in a `.test.ts`.
   - "DTO snapshot added" → there's a new `.snap` file under
     `src-tauri/tests/snapshots/`.
   - "Workflow test" → there's a new file or function under
     `tests/workflow_*.rs`.
3. **Read the diff in the test files first.** Tests describe
   intent; if you can't tell what the production code does after
   reading its tests, that's a review comment.
4. **Snapshot diffs** in `*.snap` files: read them as wire-format
   contracts. A field renamed in a snapshot is a frontend break
   waiting to happen — make sure `src-ui/src/lib/api/types.ts`
   updated to match.
5. **Merge** when the diff is small enough that you can hold it in
   your head + all tests are green. If it's not, ask for a split.

## Getting help

- Open an issue with the bug-report template (which asks for a
  failing test snippet — see `.github/ISSUE_TEMPLATE/`).
- For larger design questions, open a discussion first; PR after
  rough alignment.
