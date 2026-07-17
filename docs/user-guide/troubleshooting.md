# Troubleshooting

Common issues and how to resolve them. If something here doesn't
match what you're seeing, open a [GitHub
Discussion](https://github.com/ChivukulaVirinchi/photovault/discussions)
with your OS, Smriti version, and the symptom.

## Face features are disabled

**Symptom:** The People view says face recognition is unavailable, or
the asset-health banner on the Welcome screen flags missing face
models.

**Causes & fixes:**

1. **Asset pack not installed.** Click **Set up assets** on Welcome or
   **Settings → Download Assets**.
   This downloads the ONNX runtime, face detection + embedding
   models, and the GeoNames database. Requires internet for the
   one-time download.
2. **Asset download was interrupted.** Run **Download Assets** again.
   Installation is staged, so an interrupted attempt does not replace
   previously working assets.
3. **Disk space.** Allow roughly 1 GB of free temporary space while the
   ~300 MB pack is downloaded, validated, and unpacked.
4. **Source build without assets.** Run
   `./scripts/setup_assets.sh` (Linux/macOS) or
   `.\scripts\setup_assets.ps1` (Windows) at the repo root.

## The scan is stuck

**Symptom:** Progress bar hasn't moved in several minutes.

**Causes & fixes:**

1. **Face detection is the slow step.** Once metadata extraction
   completes, face detection runs as a separate pass that can take
   30 minutes to several hours on CPU for a large library. The
   progress reads "0 / N faces" while it's working its way through.
   Don't close the app — close-and-resume works but the pass starts
   over for the current photo. See
   [the GPU bridge doc](../face-gpu-bridge.md) for a 10–70× speedup
   via free Kaggle/Colab.
2. **A single huge file is hanging the decoder.** If progress
   restarts on a specific file every time, that file may be corrupt
   or unsupported. Move it out of the library and rescan.

## Map tiles aren't loading

**Symptom:** The Map view shows a grey background or "Failed to load
tile" placeholders.

**Causes & fixes:**

1. **No internet.** Map tiles come from `tile.openstreetmap.org` on
   first view. Without internet, only previously-cached regions
   render.
2. **Cache size cap reached.** **Settings → Map → Tile cache size**
   limits cache size; older tiles are evicted. If you've heavily
   panned, the most-distant tiles got dropped. Increase the cap or
   re-pan to reload.
3. **OSM rate-limiting.** Excessive panning in a short window may
   trigger temporary throttling. Wait a few minutes.

## Photos aren't showing up after I added them

**Symptom:** New photos copied into the library folder don't appear
in the Timeline.

**Fix:** Smriti doesn't watch the filesystem for new files. Run
**Settings → Maintenance → Rescan Library** (full rescan) or
**Check for Changes** (faster incremental).

## A face is in the wrong cluster

**Fix:**

1. Open the Person view that has the wrong photo.
2. Click the photo's face thumbnail in the person's face strip.
3. Choose **Reassign** → pick the correct person, or
   **Remove from this person** to send it back to the unknown queue.

For "the same person is split across multiple clusters," open one of
the clusters and click **Merge person** to combine them.

## The wrong city shows for a photo

**Fix:** Smriti resolves GPS → city/country via the local GeoNames
database. If the resolved city looks wrong:

1. Check the photo's EXIF GPS — it may be wrong at the source.
2. If GPS is correct but the city is wrong, the GeoNames data may
   not have the specific village/town. Smriti falls back to the
   nearest admin-seat city by population. Open an issue if the
   fallback is unhelpful.

## Build fails on Linux due to missing system libraries

Install the GUI toolkit headers:

```bash
sudo apt-get install libxkbcommon-dev libwayland-dev \
                     libxcb-shape0-dev libxcb-xfixes0-dev
```

See [BUILD.md](../BUILD.md) for the full list.

## Windows SmartScreen warned me before launching Smriti

Smriti's installer isn't code-signed (code-signing certs are paid).
SmartScreen warns on any unsigned executable.

**Workaround:** Click **More info** → **Run anyway**. The
`SHA256SUMS` file alongside the release verifies the download is
genuine.

## macOS Gatekeeper warned me before launching Smriti

Smriti isn't notarized (Apple Developer ID costs $99/year, currently
deferred). Gatekeeper warns on any non-notarized app.

**Workaround:** Right-click the app → **Open** → **Open anyway**. Or
in **System Settings → Privacy & Security**, click **Open Anyway**
after the first launch attempt.

## My library is huge — Smriti only shows 250K photos

**Symptom:** Smriti caps display at 250,000 photos per library.

**Workaround for now:** Split the library across multiple indexed
drives, each under the cap. Cursor-based streaming for arbitrarily
large libraries is on the post-1.0 roadmap.

## Updates aren't checking

**Symptom:** **Settings → Check for updates now** does nothing
visible.

**Causes:**

1. The update check is opt-in and off by default. Enable
   **Automatically check for updates** in Settings first.
2. If you're on a source build, Smriti detects this from the
   executable path and won't try to self-replace. The update banner
   suggests `git pull && cargo build --release` instead.
3. If you installed via a system package manager (apt, brew,
   flatpak, winget), the update banner shows the upgrade command
   instead.

## See also

- [FAQ](faq.md) — quick answers to common questions
- [Settings](settings.md) — the maintenance actions
- [GitHub Issues](https://github.com/ChivukulaVirinchi/photovault/issues) — bug reports
