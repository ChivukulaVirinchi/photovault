GeoNames Data

Place bundled `geonames.db` in this directory for offline reverse geocoding.

Expected files for building the database:
- `cities1000.txt` (from GeoNames `cities1000.zip`)
- `country_codes.txt`

Then run:

`cargo run --bin build_geonames`

or compile and execute `tools/build_geonames.rs` manually.
