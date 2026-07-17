GeoNames Data

Place bundled `geonames.db` in this directory for offline reverse geocoding.

Expected files for building the database:
- `cities1000.txt` (from GeoNames `cities1000.zip`)
- `country_codes.txt`

Then run:

`cargo run --bin build_geonames`

The command always writes `data/geonames.db` beside these source files.
The platform setup scripts download the sources and run it for you.
