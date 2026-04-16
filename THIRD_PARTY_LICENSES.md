# Third-Party Licenses and Attributions

PhotoVault uses the following third-party assets and dependencies.

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

### SCRFD 10G BNKPS (face detection)
- Source: downloaded by `scripts/setup_assets.sh` / `scripts/setup_assets.ps1`
- URL default: HuggingFace mirror configured in setup scripts
- License: check upstream model card before redistribution

### GLinTR-100 (face recognition)
- Source: downloaded by `scripts/setup_assets.sh` / `scripts/setup_assets.ps1`
- URL default: HuggingFace mirror configured in setup scripts
- License: check upstream model card before redistribution

## Software dependencies

PhotoVault uses many Rust crates. Run the following to generate a full list:

```bash
cargo install cargo-license
cargo license --json > THIRD_PARTY_LICENSES.json
```

A summary of major dependencies and their licenses:

| Crate | License |
|---|---|
| iced | MIT |
| rusqlite | MIT |
| tokio | MIT |
| ort | MIT/Apache-2.0 |
| image | MIT/Apache-2.0 |
| chrono | MIT/Apache-2.0 |

For full versions and transitive dependencies, generate and review
`THIRD_PARTY_LICENSES.json` in release CI.
