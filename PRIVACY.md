# Privacy

Smriti is offline-first. Here is exactly what that means.

## What stays local

Your original files and library database stay on your drive.
Local indexing and inference do not upload them. If you enable
the optional GPU bridge or provider-backed assistant, the data
described below is sent to your configured service.

**All of your activity.** No analytics, no usage tracking, no
telemetry.

## What touches the network

Network features are described below. Remote processing is optional.

### 1. Map tiles (OpenStreetMap)

When you open the **Map** view or a photo's location minimap, Smriti downloads map tiles for
the regions you pan/zoom to. Tiles are cached locally; subsequent
views of the same region are served from cache. To limit storage,
set the map tile cache limit in Settings. A cache
limit does not disable network access. Previously cached regions
remain usable offline.

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

Optional visual-search models are downloaded from Hugging Face.
These downloads do not send library contents.

### 3. Update check (opt-in)

When enabled, Smriti queries `api.github.com` at most once
every 24 hours to see if a new release has been published. This is
**opt-in** — disabled by default. On first run a prompt asks
whether you want it; you can change the answer later in
**Settings → Advanced → Automatically check for updates**.

What a single update check sends:

- A GET request to
  `https://api.github.com/repos/ChivukulaVirinchi/photovault/releases/latest`.
- Headers: `User-Agent: smriti/{version}` (GitHub requires a
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
  (AppImage, MSI or .dmg) from `github.com/…/releases`. Unsupported
  portable installations open the release page instead.
- Downloaded bytes are checked against the release's `SHA256SUMS`.
  These checksums detect mismatched downloads; they are not signed
  and do not provide independent publisher authentication.
- Only the installer artifact is fetched; no metadata about your
  install or library is sent.

If you installed via a system package manager (apt, Homebrew,
winget, Flatpak), Smriti shows the matching upgrade command
instead of self-replacing. No download happens in that path.

### 5. Optional remote face embedding

When you enable the GPU bridge, detected face crops are JPEG-encoded
and uploaded to your configured endpoint for embedding. This is
photo-derived image data. The default face-processing path is local.
The endpoint operator's retention and privacy practices apply.

### 6. Optional provider-backed assistant

When enabled, the assistant sends conversation messages, tool context
and library-derived information (such as album names, counts and
resolved people/places) to the configured provider. Disabling this
integration retains local library functionality. Provider credentials
are stored in the operating system credential store (Keychain on macOS,
Credential Manager on Windows, Secret Service on Linux). Legacy plaintext
keys migrate on the next successful settings save. If secure storage is
unavailable, saving a key fails rather than writing a new plaintext copy.

## Where your data lives

- **Photo database** — `.photovault/photovault.db` on the indexed
  drive itself. (The on-drive folder is named `.photovault/` for
  backwards compatibility with libraries indexed before the rename.)
- **Thumbnails** — `.photovault/thumbnails/` on the indexed drive.
- **Application config** — OS user config directory:
  - Linux: `~/.config/smriti/`
  - macOS: `~/Library/Application Support/smriti/`
  - Windows: `%APPDATA%\smriti\`
- **Map tile cache** — WebView Cache API storage, shared across libraries;
  clear it from Settings.
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
