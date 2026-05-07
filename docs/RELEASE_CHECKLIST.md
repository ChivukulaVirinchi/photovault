# Smriti Release Checklist

A tick-off list of everything that needs to happen outside the code
before tagging a release. The pipeline is Tauri-bundler based —
`cargo tauri build` produces `.deb`, `.AppImage`, `.msi`, `.dmg` from
the same `tauri.conf.json` and the GitHub Actions workflow at
`.github/workflows/release.yml` runs that on every `v*` tag push.

Work top-to-bottom. The first release adds setup steps that don't
need to be repeated.

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

---

## Per-release checklist

### 1. Pre-flight

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --all-targets -p smriti -p smriti-tauri`
- [ ] `cargo test -p smriti -p smriti-tauri`
- [ ] `cd src-ui && npm run check && npm run build && cd ..`
- [ ] `cargo tauri build` produces a working bundle locally.
- [ ] Manual smoke test of the dev build: open a library, scan a
      folder, verify thumbnails / faces / map / search all render.

### 2. Bump the version

Bump `version` in three files in lock-step:

- [ ] `Cargo.toml` (root) — `[package] version`
- [ ] `src-tauri/Cargo.toml` — `[package] version`
- [ ] `src-tauri/tauri.conf.json` — `"version"`

Commit with `chore(release): bump to vX.Y.Z`.

### 3. Tag and push

- [ ] `git tag -a vX.Y.Z -m "Smriti vX.Y.Z"`
- [ ] `git push origin master vX.Y.Z`

The `v*` tag triggers `.github/workflows/release.yml`, which:
- Builds Linux (`.deb`, `.AppImage`), Windows (`.msi`, portable zip),
  macOS (`.dmg` / tar.gz) via the Tauri bundler.
- Builds the optional `Smriti-Assets.zip` (face models + GeoNames + ORT).
- Drafts a GitHub release with all artifacts attached.

### 4. Verify the draft release

While the workflow runs (~15 minutes):

- [ ] Watch the workflow at **Actions → Release → run for vX.Y.Z**.
      Every job must be green.
- [ ] Once drafted, open **Releases → vX.Y.Z (Draft)** and confirm:
  - All five artifacts are attached: `Smriti-ubuntu-amd64.deb`,
    `Smriti-x86_64.AppImage`, `Smriti-Setup-x64.msi`,
    macOS bundle, `Smriti-Assets.zip`.
  - The `SHA256SUMS` file is attached and lists every artifact.
- [ ] Smoke test at least one installer on a clean VM. The CI
      smoke tests cover binary launch; eye-on verification catches
      first-run UX regressions.

### 5. Publish

- [ ] Edit the draft release. Write release notes summarising the
      changelog (see `CHANGELOG.md` if maintained).
- [ ] Click **Publish release**.

### 6. Announce

- [ ] Post in GitHub Discussions.
- [ ] (Optional) `/r/rust`, `/r/selfhosted`, Hacker News.

---

## Future polish (not blocking)

- **Homebrew tap** — `gh repo create ChivukulaVirinchi/homebrew-smriti
  --public`. Auto-maintained once the first release is published.
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
