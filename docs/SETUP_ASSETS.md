# Asset Setup

PhotoVault needs external assets for two features:

- Face detection/recognition models (`models/*.onnx`)
- Offline geocoding data (`data/geonames.db`)

Use this one-shot setup script from the repo root:

```bash
./scripts/setup_assets.sh
```

Notes:
- First run can take a while because `build_geonames` imports ~167k city rows.
- Re-running is fast: existing files are reused and DB rebuild is skipped if valid.

What it does:

1. Downloads GeoNames source data (`cities1000.zip`, `countryInfo.txt`)
2. Generates `data/country_codes.txt`
3. Builds `data/geonames.db` via `cargo run --bin build_geonames`
4. Downloads the required ONNX models directly
5. Installs as:
   - `models/scrfd_10g_bnkps.onnx`
   - `models/glintr100.onnx`

## Alternate model URLs

If you want to use different model sources:

```bash
SCRFD_MODEL_URL="https://example.com/scrfd_10g_bnkps.onnx" \
GLINTR_MODEL_URL="https://example.com/glintr100.onnx" \
./scripts/setup_assets.sh
```
