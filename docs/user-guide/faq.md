# FAQ

## Does Smriti upload my photos?

Processing is local by default. If enabled, the GPU bridge uploads face
crops and the provider-backed assistant sends conversation and library
context to your configured service. Maps and photo-location minimaps
fetch tiles; optional model/asset downloads and update checks also use
the network.

See [PRIVACY.md](../../PRIVACY.md) for the full disclosure.

## Where is my database stored?

On the indexed drive inside `.photovault/photovault.db`. That keeps
the library fully portable — unplug the drive, plug it into another
machine, and Smriti can resume.

## Can I use Smriti without internet?

Yes, after the optional assets are installed. Once the asset pack
is in place, everything — scanning, face recognition, clustering,
search, map (for previously-cached tiles), insights — works offline.

## How do updates work?

Smriti can check for new releases automatically, but the check
is **opt-in** — disabled by default. On first run a prompt asks
whether you want it; you can change the answer later in **Settings
→ Advanced**.

When an update is available and you click **Download** in the
banner, what happens depends on how you installed Smriti:

- **AppImage** or **portable Windows zip** — Smriti downloads
  the new artifact, verifies its SHA256 checksum, and atomically
  replaces the running binary. You'll be prompted to relaunch.
- **Windows MSI** — triggers the Windows installer with a UAC
  prompt; relaunch from the Start menu when it completes.
- **macOS .dmg** — opens the downloaded .dmg; drag the new `.app`
  into `/Applications` as you did for the first install.
- **Package manager (apt, brew, flatpak, winget)** — the banner
  shows the matching upgrade command instead of self-replacing.
  Smriti won't interfere with your package manager.

No photo data or telemetry leaves your machine during the check —
only a standard User-Agent and your IP, which are implicit in any
HTTP request. See [PRIVACY.md](../../PRIVACY.md).

## Why does my face-recognition pass take a long time?

Face detection runs ONNX models on the CPU by default. A 10K-photo
library typically takes 5-20 minutes on a modern laptop. Progress
is shown in the People view. The pass is resumable — close and
re-open the app, it picks up where it left off.

## Why don't I see results for very large libraries (>250K photos)?

The v1.0 release ships with a load cap of 250,000 photos per drive
to keep memory use bounded. Libraries larger than that are on the
post-v1.0 roadmap (full cursor-based streaming from the database);
for now, you can split the library across multiple indexed drives
to stay under the cap.

## I built from source — is auto-update still safe?

Smriti detects source builds by looking at the executable path
(anything under a `target/debug` or `target/release` directory).
For source builds, the update banner won't try to self-replace the
binary; it just suggests `git pull && cargo build --release`.

## Something's broken. Where do I report it?

Use the [issue tracker](https://github.com/ChivukulaVirinchi/photovault/issues)
with the bug-report template, which asks for the info that helps
most (OS, version, install method, logs). For questions or feature
ideas, prefer
[Discussions](https://github.com/ChivukulaVirinchi/photovault/discussions).

Security vulnerabilities go through
[GitHub Security Advisories](https://github.com/ChivukulaVirinchi/photovault/security/advisories/new)
privately — see [SECURITY.md](../../SECURITY.md).
