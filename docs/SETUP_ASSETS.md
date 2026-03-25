# Asset Setup

PhotoVault needs external assets for two features:

- Face detection/recognition models (`models/*.onnx`)
- Offline geocoding data (`data/geonames.db`)

Use this one-shot setup script from the repo root:

```bash
./scripts/setup_assets.sh
```

What it does:

1. Downloads GeoNames source data (`cities1000.zip`, `countryInfo.txt`)
2. Generates `data/country_codes.txt`
3. Builds `data/geonames.db` via `cargo run --bin build_geonames`
4. Downloads InsightFace model pack (`antelopev2.zip` by default)
5. Installs the required files as:
   - `models/scrfd_10g_bnkps.onnx`
   - `models/glintr100.onnx`

## Alternate model source

If you want to use a different model pack URL:

```bash
INSIGHTFACE_MODEL_URL="https://example.com/models.zip" ./scripts/setup_assets.sh
```

The script searches extracted files for matching ONNX names and copies them into `models/`.
