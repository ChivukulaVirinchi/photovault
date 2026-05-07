# Linux Release Guide (Ubuntu + AppImage)

This is the canonical maintainer flow for Linux releases.

## Scope

Supported Linux artifacts:
- `Smriti-ubuntu-amd64.deb`
- `Smriti-x86_64.AppImage`

Published by tag-driven CI workflow:
- `.github/workflows/release.yml`

CI also runs package smoke checks on tag releases:
- `.deb` install + launch smoke test
- AppImage extract/layout check + launch smoke test

## 1) Pre-release checks

Run from repo root:

```bash
cargo fmt --all --check
cargo check
cargo test
```

Optional:

```bash
cargo clippy --all-targets
```

## 2) Local Linux packaging smoke tests

### 2.1 Build Ubuntu/Debian package

```bash
./scripts/release_local.sh ubuntu
```

Expected output:
- `target/debian/*.deb`

Install test:

```bash
sudo dpkg -i target/debian/*.deb
photovault
```

### 2.2 Build AppImage

```bash
./scripts/release_local.sh linux-appimage
```

Expected output:
- `Smriti-x86_64.AppImage`

Run test:

```bash
chmod +x Smriti-x86_64.AppImage
./Smriti-x86_64.AppImage
```

## 3) Publish Linux artifacts via CI

Create and push a release tag:

```bash
git checkout master
git pull origin master
git tag -a vX.Y.Z -m "Smriti vX.Y.Z"
git push origin vX.Y.Z
```

CI will create a draft release with assets.

## 4) Verify release assets

Ensure draft release contains at least:
- `Smriti-ubuntu-amd64.deb`
- `Smriti-x86_64.AppImage`
- `SHA256SUMS`

Note: Draft release creation is blocked unless Linux smoke-test jobs pass.

Verify checksums:

```bash
sha256sum -c SHA256SUMS
```

## 5) Publish release

Open draft release, review notes, publish.

## 6) Post-release checks

Validate website links:
- Ubuntu `.deb`: `/releases/latest/download/Smriti-ubuntu-amd64.deb`
- AppImage: `/releases/latest/download/Smriti-x86_64.AppImage`

## 7) Troubleshooting

If app starts but models/GeoNames are not found:
- verify assets are inside package/AppImage
- verify `PHOTOVAULT_ASSET_DIR` is set in AppImage AppRun
- verify runtime path resolution handles `/usr/lib/photovault`
