# Linux Release Guide (Ubuntu + AppImage)

This is the canonical maintainer flow for Linux releases.

## Scope

Supported Linux artifacts:
- `Smriti-ubuntu-amd64.deb`
- `Smriti-x86_64.AppImage`

Published by tag-driven CI workflow:
- `.github/workflows/release.yml`

CI builds each package on its native runner. A clean-machine launch remains
the manual smoke test before publishing the draft release.

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
- `target/release/bundle/deb/*.deb`

Install test:

```bash
sudo dpkg -i target/release/bundle/deb/*.deb
smriti-tauri
```

### 2.2 Build AppImage

```bash
./scripts/release_local.sh linux-appimage
```

Expected output:
- `target/release/bundle/appimage/*.AppImage`

Run test:

```bash
chmod +x target/release/bundle/appimage/*.AppImage
target/release/bundle/appimage/*.AppImage
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

Note: Draft release creation is blocked unless the native package build jobs
pass.

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
- verify `SMRITI_ASSET_DIR` is set in AppImage AppRun
- verify runtime path resolution handles `/usr/lib/smriti` (and the
  legacy `/usr/lib/photovault` fallback for upgrading users)
