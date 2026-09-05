<!--
Thanks for the PR! Filling out this template carefully is the
biggest single thing that gets your change merged. The maintainer
uses it as a checklist — if a box isn't ticked but should be, the
PR gets a "please add tests" comment and waits.

Reference: CONTRIBUTING.md + docs/TESTING.md.
-->

## Type of change

- [ ] Bug fix (changes existing behaviour to match what it always
      should have been)
- [ ] New feature (adds new user-visible functionality)
- [ ] Refactor (no behaviour change, structure only)
- [ ] Documentation only
- [ ] Test-only

## What changed

<!-- One short paragraph: what is now different. Skip the "why" if
     it's obvious from the linked issue. -->

## Why

<!-- One short paragraph or a link to the issue. Closes #123 if
     applicable. -->

## Tests added

Pick everything that applies. If you can't tick any, explain at the
bottom.

- [ ] Unit test for a new pure function (Rust `#[cfg(test)] mod
      tests`, or TypeScript sibling `*.test.ts`)
- [ ] DTO snapshot for a new or changed `From<EngineType> for
      SomethingDto` (`src-tauri/tests/dto_snapshots.rs`, accepted via
      `cargo insta review`)
- [ ] Workflow / integration test for a service contract change
      (`tests/workflow_*.rs`)
- [ ] Regression test attached to a bug fix (must fail before your
      fix and pass after — reviewer will verify by reverting your
      fix locally)
- [ ] N/A — reason: <fill in>

## Verification

- [ ] `scripts/ci_local.sh ci` passed locally
- [ ] `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
      all green
- [ ] If you changed UI behaviour: manually smoke-tested the affected
      flow in the dev app (`scripts/dev.ps1` or `scripts/dev.sh`)
- [ ] Updated `CHANGELOG.md` under `[Unreleased]` if user-visible
- [ ] Updated relevant documentation (docs/, agents.md, code comments)

## Screenshots

<!-- For UI changes: before/after side-by-side. -->

## Notes for the reviewer

<!-- One paragraph: what's the risk, where should the reviewer look
     first, anything weird about the diff that the commit message
     doesn't already cover. -->
