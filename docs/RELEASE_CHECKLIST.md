# Smriti Release Checklist

A tick-off list of everything that needs to happen outside the code
before tagging a release. The pipeline is Tauri-bundler based —
`cargo tauri build` produces `.deb`, `.AppImage`, `.msi`, `.dmg` from
the same `tauri.conf.json` and the GitHub Actions workflow at
`.github/workflows/release.yml` runs that on every `v*` tag push.

Two ways to cut a release:

- **Automated (recommended for rc + stable):**
  `scripts/release.ps1 v0.2.0-rc.3` (Windows) or
  `scripts/release.sh v0.2.0-rc.3` (Linux/macOS).
  Bumps every manifest, commits, pushes, tags, pushes the tag.
  See [Automated release](#automated-release) below.
- **Manual:** follow the [Per-release checklist](#per-release-checklist).
  Use this when the automation isn't appropriate (e.g. you want a
  bespoke commit message, you're amending a previous release, or
  you're publishing from a non-master branch).

---

## Where the version lives

Smriti's user-facing version lives in **two** files. A third file
updates itself when cargo runs:

| File | Field | Purpose |
|---|---|---|
| `Cargo.toml` | `[package] version` (line ~16) | Engine crate version. Reported by `cargo --version`. |
| `src-tauri/tauri.conf.json` | `"version"` (line ~4) | **The Tauri app version.** Drives `.msi` / `.deb` / `.dmg` package metadata, the Windows "About" panel, macOS bundle info. Most user-visible. |
| `Cargo.lock` | (auto) | Rewritten by `cargo update -p smriti` when the engine bumps. Commit the diff. |

**Files that look like they have a version but don't matter for releases:**

- `src-tauri/Cargo.toml` — version is the placeholder `"0.0.0"`. The
  Tauri build reads `tauri.conf.json` for the real version.
- `src-ui/package.json` — version is the placeholder `"0.0.0"`. npm
  doesn't publish anything from here.

---

## Tag naming

Use **strict SemVer 2.0** for tags. The leading `v` is a tag
convention; it does NOT appear in the version field inside files.

| Release kind | Tag | Manifest version | Notes |
|---|---|---|---|
| Stable | `v0.2.0` | `0.2.0` | |
| Release candidate | `v0.2.0-rc.3` | `0.2.0-rc.3` | The dot before the number is SemVer-correct; cargo, npm, and `release.yml` all handle it. |
| Beta | `v0.2.0-beta.1` | `0.2.0-beta.1` | Same shape. |

Earlier tags in this repo used `v0.2.0rc-2` (no dot, dash on wrong
side). That format is not strict SemVer and confuses tooling — prefer
`-rc.N` going forward.

---

## Automated release

The fastest path. One command:

```powershell
# Windows
scripts\release.ps1 v0.2.0-rc.3
```

```bash
# Linux / macOS
./scripts/release.sh v0.2.0-rc.3
```

What it does, in order:

1. Validates the version format.
2. Validates working tree is clean, on master, in sync with origin.
3. Rewrites `[package] version` in `Cargo.toml`.
4. Rewrites `"version"` in `src-tauri/tauri.conf.json`.
5. Runs `cargo update -p smriti` to refresh `Cargo.lock`.
6. Shows the diff and pauses for confirmation.
7. Commits `chore(release): vX.Y.Z` and pushes to master.
8. Creates `vX.Y.Z` annotated tag and pushes it.
9. Prints the URL of the running `release.yml` workflow.

Flags:

- `-DryRun` (PS) / `--dry-run` (sh) — show what would change without
  modifying anything.
- `-NoTag` (PS) / `--no-tag` (sh) — bump + commit + push only,
  skip the tag (useful when you want to make additional changes
  before tagging).
- `-AllowDirty` — proceed even if the working tree has uncommitted
  changes. Default is to refuse.

After the script finishes, see [Post-tag steps](#post-tag-steps).

---

## Per-release checklist

Use this if you're doing it by hand.

### 1. Pre-flight

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --all-targets -p smriti -p smriti-tauri -- -D warnings`
- [ ] `cargo test -p smriti -p smriti-tauri`
- [ ] `npm run check --prefix src-ui` and `npm run test --prefix src-ui`
- [ ] `npm run build --prefix src-ui` produces a clean `dist/`.
- [ ] `cargo tauri build` produces a working bundle locally.
- [ ] Manual smoke test of the dev build: open a library, scan a
      folder, verify thumbnails / faces / map / search all render.
- [ ] **Local Rust matches CI's stable** — `rustup update stable` if
      you've fallen behind. Newer clippy versions catch new lints
      that older local toolchains miss; CI is on whatever stable was
      published last.

### 2. Bump the version

Update both files in lock-step:

- [ ] `Cargo.toml` — `[package] version`
- [ ] `src-tauri/tauri.conf.json` — `"version"`
- [ ] `cargo update -p smriti` → updates `Cargo.lock`

Commit with `chore(release): vX.Y.Z`.

### 3. Push the bump

- [ ] `git push origin master`

### 4. Tag and push the tag

- [ ] `git tag -a vX.Y.Z -m "Smriti vX.Y.Z"`
- [ ] `git push origin vX.Y.Z`

The `v*` tag triggers `.github/workflows/release.yml`, which:

- Builds Linux (`.deb`, `.AppImage`, `.rpm`), Windows (`.msi`,
  portable zip), macOS (`.dmg` / tar.gz) via the Tauri bundler.
- Builds the optional `Smriti-Assets.zip` (face models + GeoNames +
  ORT).
- Drafts a GitHub release with all artifacts attached.

### 5. Post-tag steps

While the workflow runs (~15–30 minutes):

- [ ] Watch the workflow at **Actions → Release → run for vX.Y.Z**.
      Every job must be green.
- [ ] Once drafted, open **Releases → vX.Y.Z (Draft)** and confirm:
  - All expected artifacts attached (matrix output × 3 OSes + assets pack).
  - `SHA256SUMS` file is attached and lists every artifact.

#### For release-candidate (`-rc.N`) tags

- [ ] **Manually toggle "Set as a pre-release"** on the draft release
      UI. The current `release.yml` has `prerelease: false` hardcoded
      for every tag — fine for stable but wrong for rc. Once toggled
      to pre-release, the website's `/releases/latest/download/...`
      links keep pointing at the previous *stable* (which is what
      you want — rc users come find rc explicitly).

#### Smoke test

- [ ] Install at least one artifact on a clean VM. CI smoke tests
      cover binary launch; eye-on verification catches first-run UX
      regressions.

### 6. Publish

- [ ] Edit the draft release. Write release notes summarising what
      changed (see `CHANGELOG.md`).
- [ ] Click **Publish release**.

### 7. Announce

- [ ] GitHub Discussions post.
- [ ] (Stable only) `r/selfhosted`, `r/privacy`, Hacker News if
      it's a significant release. Skip for rc.

---

## Interaction with release-plz

`release-plz` (configured in `.release-plz.toml`) watches master for
conventional-commit-prefixed commits (`feat:`, `fix:`, `perf:`,
`refactor:`, `chore:`) and opens a "Release PR" that bumps the
version + rewrites `CHANGELOG.md`.

That's the **stable** workflow. It cannot natively cut rc releases.

When you push an rc commit + tag manually:

- release-plz will open a Release PR proposing the *next stable*
  bump (because it ignores pre-release suffixes for semver math).
- **Close that PR.** Don't merge it; the manual rc has already
  shipped. When you're ready to cut the next stable, let release-plz
  open a fresh PR then.

To avoid the noise entirely on an rc, prefix the bump commit with
`chore(release):` — release-plz's `chore` group is grouped lowest
priority and won't trigger an automated semver bump on its own.

---

## One-time setup (skip if already done)

### Branch protection on `master`

**Settings → Branches → Add branch protection rule** for `master`.

- [ ] Require PR before merging (1 approval is fine for a solo project).
- [ ] Require status checks: `Format`, `Quality (ubuntu-latest)`,
      `Quality (windows-latest)`, `Quality (macos-latest)`,
      `Security advisories`, `Dependency policy`.
- [ ] Require branches up-to-date before merging.
- [ ] Include administrators.

### Private vulnerability reporting

- [ ] **Settings → Code security and analysis → Private vulnerability
      reporting → Enable.** `SECURITY.md` already points users at
      `/security/advisories/new`.

### GitHub Discussions

- [ ] **Settings → Features → Discussions → tick the checkbox.**
- [ ] Create + pin a welcome post linking to `CONTRIBUTING.md` and
      `PRIVACY.md`.

### Seed issue labels

- [ ] **Actions tab → Sync issue labels → Run workflow.** One-shot.
      Re-run any time `.github/labels.yml` changes.

### GitHub Pages

- [ ] **Settings → Pages → Source: GitHub Actions** (not "Deploy
      from a branch"). `.github/workflows/docs.yml` uses
      `actions/deploy-pages` and needs this setting.

---

## Future polish (not blocking)

- **Auto-detect rc tags** in `release.yml` and set `prerelease: true`
  for tags matching `*-rc.*` / `*-beta.*`. Removes the manual toggle.
- **Homebrew tap** — `gh repo create ChivukulaVirinchi/homebrew-smriti
  --public`. Auto-maintained once the first stable release is published.
- **winget** — fork `microsoft/winget-pkgs`, write the first manifest
  by hand. After acceptance `winget-create` can auto-submit PRs.
- **Flathub** — write `in.smriti.app.yaml`, open a PR against
  `flathub/flathub`. Review usually 1-2 weeks.
- **AUR package** — `PKGBUILD` against the `.tar.gz` artifact.
- **Code signing** — Authenticode ($80-400/yr), Apple Developer ID
  ($99/yr). Skip until donations cover the cost.

---

## Explicitly not doing

- **Microsoft Store / MSIX** — significant packaging effort, low
  reach vs. the MSI path.
- **Bundling InsightFace model weights** — they're downloaded by the
  user's own setup script. Each user's terms come from the upstream
  project, not Smriti.

---

## Related scripts

- `scripts/release.ps1` / `scripts/release.sh` — the automation
  described in [Automated release](#automated-release).
- `scripts/release_publish.ps1` — older, more thorough version that
  also drives `gh` to watch the workflow and verify artifacts. Does
  NOT bump versions; assumes you've already done that. Useful when
  you want to monitor the build from the terminal.
- `scripts/release_local.ps1` / `scripts/release_local.sh` — runs
  the release build locally for smoke testing before tagging.
- `scripts/ci_local.sh` — runs the full CI gate locally
  (`fmt + clippy + test + frontend`).
