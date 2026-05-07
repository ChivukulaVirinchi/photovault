# Privacy

Smriti is offline-first. Here is exactly what that means.

## What stays local

**All of your photos.** Smriti never uploads, syncs, or
transmits your photos anywhere. They stay on your drive.

**All of your metadata.** Face data, locations, dates, EXIF —
stored in a SQLite database on the same drive as your photos.
Nothing leaves.

**All of your activity.** No analytics, no usage tracking, no
telemetry.

## What touches the network

Four things, all by design and clearly scoped:

### 1. Map tiles (OpenStreetMap)

When you open the **Map** view, Smriti downloads map tiles for
the regions you pan/zoom to. Tiles are cached locally; subsequent
views of the same region are served from cache. To limit or
disable: avoid the Map view, or cap the cache size in
**Settings → Map**.

The requests go to `tile.openstreetmap.org` and include only the
standard headers any HTTP client sends (User-Agent, Accept).

### 2. Optional asset pack (one-time, opt-in)

ONNX face-recognition models and the GeoNames geocoding database
can be downloaded once from the project's GitHub releases if not
already installed. You see a prompt on first run asking to install
them; declining it doesn't block any core app functionality (face
recognition and reverse-geocoding are the features that rely on
them).

Re-triggered from **Settings → Advanced → Reinstall Assets**.

### 3. Update check (opt-in)

When enabled, Smriti queries `api.github.com` at most once
every 24 hours to see if a new release has been published. This is
**opt-in** — disabled by default. On first run a prompt asks
whether you want it; you can change the answer later in
**Settings → Advanced → Automatically check for updates**.

What a single update check sends:

- A GET request to
  `https://api.github.com/repos/ChivukulaVirinchi/photovault/releases/latest`.
- Headers: `User-Agent: photovault/{version}` (GitHub requires a
  User-Agent on all API requests) and an `Accept: application/vnd.github+json`.
- Your IP address, as with any HTTP request. GitHub logs this per
  [GitHub's own privacy policy](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).

What it does **not** send: photo data, thumbnails, EXIF, face
embeddings, library stats, install telemetry, or anything else.

You can also trigger a single manual check via
**Settings → Check for updates now**, or turn the feature off
entirely.

### 4. In-app update download (only when you click Download)

If you enable update checks and click **Download** in the banner
when a new version is available:

- Smriti downloads the matching installer for your platform
  (AppImage, MSI, .dmg, or portable zip) from `github.com/…/releases`.
- Downloaded bytes are verified against the signed `SHA256SUMS`
  published alongside the release.
- Only the installer artifact is fetched; no metadata about your
  install or library is sent.

If you installed via a system package manager (apt, Homebrew,
winget, Flatpak), Smriti shows the matching upgrade command
instead of self-replacing. No download happens in that path.

## Where your data lives

- **Photo database** — `.photovault/photovault.db` on the indexed
  drive itself.
- **Thumbnails** — `.photovault/thumbnails/` on the indexed drive.
- **Application config** — OS user config directory:
  - Linux: `~/.config/photovault/`
  - macOS: `~/Library/Application Support/photovault/`
  - Windows: `%APPDATA%\photovault\`
- **Map tile cache** — OS user cache directory.
- **Logs (if any)** — OS user data directory.

## What we do not do

- No accounts, no sign-in.
- No telemetry, no analytics, no "anonymous usage statistics".
- No cloud backup.
- No "shared" features that would require a server.
- No third-party trackers, ads, or A/B experimentation frameworks.

## Reporting a privacy concern

Privacy-related bugs (data unexpectedly leaving your machine,
unexpected network traffic) go through our security-disclosure path:
[GitHub Security Advisories](https://github.com/ChivukulaVirinchi/photovault/security/advisories/new).
See [SECURITY.md](SECURITY.md) for details.

For non-urgent questions or comments on this policy, open a
[Discussions](https://github.com/ChivukulaVirinchi/photovault/discussions)
thread.
