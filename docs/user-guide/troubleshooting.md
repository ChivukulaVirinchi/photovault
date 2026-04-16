# Troubleshooting

## App starts but face features are disabled
Run the asset setup scripts and verify `models/` and `libs/onnxruntime/`.

## Build fails on Linux due to missing system libraries
Install `libxkbcommon-dev`, `libwayland-dev`, `libxcb-shape0-dev`, and `libxcb-xfixes0-dev`.

## Map tiles not loading
Check internet access and map cache limits in Settings.
