# Third-Party Licenses and Attributions

Smriti uses the following third-party assets and dependencies.

## Data

### GeoNames
Geographical data used for offline reverse geocoding.
- License: [Creative Commons Attribution 4.0 (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/)
- Attribution: GeoNames, https://www.geonames.org/
- Used in: location lookups, trip detection, map metadata

### OpenStreetMap
Map tiles used in the Map view.
- License: [Open Database License (ODbL)](https://www.openstreetmap.org/copyright)
- Attribution: OpenStreetMap contributors
- The map view includes attribution text in-app.

## ML Models

Smriti does **not** bundle ML model weights in the installer.
The optional asset-pack installer downloads them from the upstream
projects linked below, on the user's machine, after install.
Whatever terms the upstream projects publish govern how you're
allowed to use those weights — Smriti is a neutral integrator.

### SCRFD-10G-BNKPS (face detection)
- Upstream: [InsightFace](https://github.com/deepinsight/insightface)
- Paper: *Sample and Computation Redistribution for Efficient Face
  Detection* (Guo et al., CVPR 2021)
- Please refer to the InsightFace model-zoo README for the terms
  that apply to you.

### GLinT-R100 (face recognition)
- Upstream: [InsightFace](https://github.com/deepinsight/insightface)
- Trained on the GLINT360K dataset
- Please refer to the InsightFace model-zoo README for the terms
  that apply to you.

### ONNX Runtime
The shared library that loads and executes the above models.
- License: [MIT License](https://github.com/microsoft/onnxruntime/blob/main/LICENSE)
- Attribution: Microsoft Corporation
- Distributed via official GitHub releases; Smriti's setup
  script downloads the matching platform binary on demand.

### libheif (optional, `heic` Cargo feature)
Decoder for HEIC / HEIF photos (every iPhone export since iOS 11).
- License: [LGPL-3.0](https://github.com/strukturag/libheif/blob/master/COPYING)
- Attribution: Dirk Farin / Struktur AG
- System library — install via `apt install libheif-dev` (Linux),
  `brew install libheif` (macOS), or vendor binaries (Windows).
  When the `heic` feature is off Smriti still indexes HEIC files
  but reports a clear "HEIC support not compiled in" error on decode.

## Fonts

### Inter
UI text font.
- License: [SIL Open Font License 1.1](https://openfontlicense.org/)
- Attribution: Rasmus Andersson, https://rsms.me/inter/
- Bundled at `assets/fonts/Inter-{Regular,Medium,SemiBold}.ttf`.

### JetBrains Mono
Monospaced font (used for the loading spinner, braille glyphs).
- License: [SIL Open Font License 1.1](https://openfontlicense.org/)
- Attribution: JetBrains s.r.o., https://www.jetbrains.com/lp/mono/
- Bundled at `assets/fonts/JetBrainsMono-Regular.ttf`.

### Lucide
Icon font.
- License: [ISC License](https://github.com/lucide-icons/lucide/blob/main/LICENSE) (with Feather-derived icons under the MIT License)
- Attribution: Lucide Icons and Contributors, https://lucide.dev/
- Bundled at `assets/fonts/lucide.ttf`.

## Software dependencies

Smriti depends on many Rust crates. Run the following to
generate a full per-crate license list:

```bash
cargo install cargo-license
cargo license --json > THIRD_PARTY_LICENSES.json
```

Headline dependencies and their licenses:

| Crate      | License                     |
|------------|-----------------------------|
| iced       | MIT                         |
| rusqlite   | MIT                         |
| tokio      | MIT                         |
| ort        | MIT OR Apache-2.0           |
| image      | MIT OR Apache-2.0           |
| chrono     | MIT OR Apache-2.0           |
| reqwest    | MIT OR Apache-2.0           |
| rayon      | MIT OR Apache-2.0           |
| sha2       | MIT OR Apache-2.0           |
| ndarray    | MIT OR Apache-2.0           |
| self_update| MIT OR Apache-2.0           |
| semver     | MIT OR Apache-2.0           |

License compliance is enforced in CI by `cargo deny check licenses`
(see `deny.toml` for the allow-list).
