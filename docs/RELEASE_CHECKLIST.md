# Smriti v1.0 Release Checklist

A tick-off list of everything that needs to happen outside the code
before tagging `v1.0.0`. All items are free — no certificates, no
paid accounts. Designed to be doable by a solo maintainer in a
single afternoon.

Work top-to-bottom. Items are numbered so you can reference them in
commits or issues.

---

## 1. GitHub repository settings (10 minutes total)

Every item here is a toggle in the repo Settings UI, zero code needed.

### 1.1 · Enable branch protection on `master`

Open **Settings → Branches → Add branch protection rule** and match
against `master`.

- [ ] Require a pull request before merging (1 approval is fine for
      a solo project; the rule exists mostly to protect against
      accidental direct-to-master pushes).
- [ ] Require status checks to pass before merging. Select:
  - [ ] `Format`
  - [ ] `Quality (ubuntu-latest)`
  - [ ] `Quality (windows-latest)`
  - [ ] `Quality (macos-latest)`
  - [ ] `MSRV (1.75)`
  - [ ] `Security advisories`
  - [ ] `Dependency policy`
  - [ ] `Benches smoke test`
- [ ] Require branches to be up to date before merging.
- [ ] Include administrators (tick this — otherwise you can
      accidentally bypass the gate with your own admin role).

### 1.2 · Enable private vulnerability reporting

- [ ] **Settings → Code security and analysis → Private
      vulnerability reporting → Enable.**

`SECURITY.md` already points users at
`/security/advisories/new`, so the link starts working
immediately.

### 1.3 · Enable GitHub Discussions

- [ ] **Settings → Features → Discussions → tick the checkbox.**
- [ ] Create a welcome post pointing at `CONTRIBUTING.md` and
      `PRIVACY.md`.
- [ ] Pin the welcome post.

The README, landing page, FAQ, and issue templates already link
to `https://github.com/ChivukulaVirinchi/photovault/discussions` —
all of those start working the moment you enable this.

### 1.4 · (Optional) Enable GitHub Sponsors

Only if you actually want donations. `.github/FUNDING.yml` is
already committed, so the button lights up as soon as your account
is enrolled.

- [ ] **Your profile → Sponsors → Join the waitlist** (or complete
      enrolment if you're already approved).

---

## 2. First Actions run (2 minutes)

### 2.1 · Seed issue labels

- [ ] **Actions tab → Sync issue labels workflow → Run workflow →
      Run.**

One-shot. Applies the full label taxonomy from `.github/labels.yml`.
Safe to re-run if you tweak labels later.

---

## 3. Release pipeline one-time setup (15 minutes)

### 3.1 · Run `cargo dist init`

This replaces the hand-written `release.yml` with one maintained by
`cargo-dist`. Benefits: tag push automatically publishes the
release (no more manual click in the UI), `.dmg` for macOS comes
for free, installers update themselves as cargo-dist upgrades.

- [ ] Install cargo-dist: `cargo install cargo-dist --locked`
- [ ] Run `cargo dist init` at the repo root. Answer:
  - **Installers**: `shell`, `powershell`, `homebrew`, `msi`
  - **Targets**: `x86_64-unknown-linux-gnu`,
    `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`,
    `aarch64-apple-darwin`
  - **Publish**: `false` for now (keep drafts until first run is
    proven), flip to `true` after the first good release.
- [ ] Verify the init wrote a `[workspace.metadata.dist]` block to
      `Cargo.toml` and regenerated `.github/workflows/release.yml`.
- [ ] Port the existing asset-pack job (the one that packages ML
      models + GeoNames + ORT into `Smriti-Assets.zip`) into
      cargo-dist's `extra-artifacts` hook. See the old workflow in
      git history if you need the exact commands.
- [ ] Test with a throwaway tag on a branch:
      `git tag v1.0.0-alpha.1 && git push origin v1.0.0-alpha.1`.
      Delete tag + branch after verifying the draft release looks
      right.

---

## 4. Package-manager distribution (as time permits)

Each of these is free and can happen whenever. Users who don't use
package managers just grab the installer from GitHub Releases — so
none of these block v1.0.

### 4.1 · Create the Homebrew tap repo

- [ ] `gh repo create ChivukulaVirinchi/homebrew-photovault --public --description "Homebrew tap for Smriti"`
- [ ] After the first cargo-dist release, the tap is auto-maintained —
      you just need the repo to exist.

Users install with:

```bash
brew tap ChivukulaVirinchi/photovault
brew install --cask photovault
```

### 4.2 · Submit to winget

- [ ] Fork `microsoft/winget-pkgs`.
- [ ] Manually write the manifest for your first release using their
      template. After it's accepted, `winget-create` can auto-submit
      PRs for subsequent versions from CI.

Users install with:

```powershell
winget install ChivukulaVirinchi.Smriti
```

### 4.3 · Submit to Flathub

- [ ] Write a Flatpak manifest at
      `com.chivukulavirinchi.photovault.yaml`.
- [ ] Fork `flathub/flathub`, add the manifest, open a PR.
- [ ] Flathub review usually takes 1-2 weeks. Address feedback in
      the PR as they land.

Users install with:

```bash
flatpak install flathub com.chivukulavirinchi.photovault
```

---

## 5. Tag v1.0.0-beta.1

When 1.1, 1.2, 1.3, 2.1, and 3.1 are all ticked:

- [ ] `git checkout master && git pull --ff-only`
- [ ] Wait for the first `release-plz` PR to appear on your
      master branch (it opens automatically after merges with
      conventional-commit prefixes). Merge it — this creates the
      tag automatically.
- [ ] Or, if you want to force a specific version tag manually:
      `git tag -a v1.0.0-beta.1 -m "Smriti 1.0.0-beta.1" && git push origin v1.0.0-beta.1`
- [ ] Wait ~15 minutes for the release workflow to build all
      artifacts. With cargo-dist set to draft mode, edit the draft
      and click **Publish** once it looks good.
- [ ] Smoke-test each installer on a fresh VM (or just on each OS
      you have access to) before announcing. The Phase 1 smoke
      tests in CI cover basic launch, but eyes-on verification
      once per release catches first-run UX regressions.

---

## 6. Tag v1.0.0

After running v1.0.0-beta.1 for a few weeks with no major bug
reports:

- [ ] Merge the next `release-plz` PR, which will bump to `1.0.0`.
- [ ] Post an announcement in Discussions.
- [ ] Post to `/r/rust`, `/r/selfhosted`, `/r/opensource`,
      Hacker News, whatever else seems right. Keep the post matter-
      of-fact; a cross-platform offline photo manager with face
      recognition in pure Rust is inherently interesting to those
      audiences.

---

## Optional polish — skip unless motivated

- **GPG-sign `SHA256SUMS`** — Free. Generate a key, publish the
  public half to `keys.openpgp.org`, set `GPG_PRIVATE_KEY` +
  `GPG_PASSPHRASE` as repo secrets, add a one-liner to the release
  workflow. Users who care about supply-chain can then
  `gpg --verify SHA256SUMS.sig SHA256SUMS`. Purely nice-to-have.
- **AUR package** — Arch users usually pick this up themselves
  once the `.tar.gz` asset is stable. You can seed one yourself
  with a `PKGBUILD` if you want.
- **Scoop bucket** — Windows CLI-first users. Very low-volume vs.
  winget, skip unless asked.
- **Discord / Matrix** — Skip until GitHub Discussions is
  genuinely overflowing.

---

## Explicitly not doing

These show up in discussions about "proper" OSS releases. They're
fine to skip for a hobby project:

- **Authenticode code-signing** — $80-400/year. Windows SmartScreen
  will warn on first install; users click "More info" → "Run
  anyway" once. Not worth the subscription for a hobby project.
- **Apple Developer ID + notarization** — $99/year. macOS
  Gatekeeper warns once; users right-click → Open. Same trade-off.
- **Microsoft Store / MSIX** — Significant packaging effort, low
  reach vs. the MSI path.
- **Resolving InsightFace model licensing** — Smriti doesn't
  bundle the weights. The setup script downloads them from
  upstream on the user's own machine. The terms that apply to
  each user are the ones the upstream project publishes; that's
  between them and InsightFace, not Smriti's concern.
