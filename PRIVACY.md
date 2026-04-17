# Privacy

PhotoVault is offline-first. Here is exactly what that means.

## What stays local

**All of your photos.** PhotoVault never uploads, syncs, or transmits
your photos anywhere. They stay on your drive.

**All of your metadata.** Face data, locations, dates, EXIF -- all stored
in a SQLite database on the same drive as your photos. Nothing leaves.

**All of your activity.** No analytics, no usage tracking, no telemetry.
Period.

## What touches the network

Three things, by design:

1. **Map tiles** (OpenStreetMap)
   When you open the Map view, PhotoVault downloads map tiles for the
   regions you view. These are cached locally. To opt out: avoid the
   Map view, or limit cache size in Settings.

2. **First-run asset download** (one-time, optional)
   ONNX face recognition models and GeoNames geocoding data can be
   downloaded once on first launch if not already present.

3. **Update check** (optional, opt-in)
   If enabled in Settings, PhotoVault checks GitHub Releases for new
   versions. No personal data is sent. Default: off.

## Where your data lives

- **Photo database**: in a `.photovault/` folder on the indexed drive itself
- **Application config**: in your OS user config directory:
  - Linux: `~/.config/photovault/`
  - macOS: `~/Library/Application Support/photovault/`
  - Windows: `%APPDATA%\photovault\`
- **Map tile cache**: in your OS user cache directory
- **Crash logs** (if any): in your OS user data directory

## What we do not do

- No accounts, no sign-in
- No telemetry, no analytics
- No "anonymous usage statistics"
- No cloud backup
- No "shared features" requiring servers
- No third-party trackers
- No ads

## Reporting a privacy concern

Open an issue at https://github.com/ChivukulaVirinchi/photovault/issues
