{
  mkShell, rustc, cargo, rustfmt, clippy, cargo-tauri,
  nodejs_22, pkg-config, glib, glib-networking, gtk3,
  webkitgtk_4_1, libsoup_3, libheif, librsvg, dbus, onnxruntime, gst_all_1,
}:
mkShell {
  packages = [ rustc cargo rustfmt clippy cargo-tauri nodejs_22 pkg-config ];
  buildInputs = [
    glib glib-networking gtk3 webkitgtk_4_1 libsoup_3 libheif librsvg dbus
  ] ++ (with gst_all_1; [ gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav ]);
  shellHook = ''
    export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
    # Don't run setup_assets.sh here: its prebuilt ORT can't dlopen on NixOS.
    # runtime.rs checks ORT_DYLIB_PATH first, so the engine uses nixpkgs' build;
    # install the face/places pack from inside the app like on other platforms.
    export ORT_DYLIB_PATH="${onnxruntime}/lib/libonnxruntime.so"
    echo "Dev: npm ci --prefix src-ui && cargo tauri dev --no-watch (smart-features pack: install in-app)"
  '';
}
