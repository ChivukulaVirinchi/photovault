{
  lib,
  fetchFromGitHub,
  rustPlatform,
  buildNpmPackage,
  importNpmLock,
  nodejs_22,
  cargo-tauri,
  pkg-config,
  wrapGAppsHook3,
  glib,
  glib-networking,
  gtk3,
  cairo,
  gdk-pixbuf,
  pango,
  webkitgtk_4_1,
  libsoup_3,
  libheif,
  librsvg,
  dbus,
  xdg-utils,
  gst_all_1,
  oniguruma,
  sqlite,
  # nixpkgs' ONNX Runtime (required): the wrapper sets ORT_DYLIB_PATH so the
  # in-app smart-features pack never needs its bundled prebuilt runtime.
  onnxruntime,
  extraWrapperArgs ? [ ],
}:

# ort 2.0.0-rc.11 binds the ONNX Runtime 1.23 C API (upstream ships 1.23.2).
# load-dynamic accepts any newer runtime (the C API is backwards compatible),
# but fail loudly if nixpkgs ever ships something older.
assert lib.assertMsg (lib.versionAtLeast onnxruntime.version "1.23")
  "smriti: ort 2.0.0-rc.11 requires ONNX Runtime >= 1.23 (nixpkgs has ${onnxruntime.version})";

let
  # The workspace root is the smriti crate (src-tauri is a 0.0.0 shell),
  # so the version tracks the source of truth instead of a hardcoded copy.
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSourceWith {
     src = ../.;
     filter = name: type:
       let base = baseNameOf (toString name); in
       !(builtins.elem base [
         ".git" ".direnv" "node_modules" "target" "result" "dist"
         "data" "website" "notebooks"
       ])
       && !(lib.hasPrefix "result" base);
   };

  # Svelte 5 + Vite 8 frontend. Tauri embeds these assets at *compile time*
  # (custom-protocol), so dist must exist before cargo runs.
  frontend = buildNpmPackage {
    pname = "smriti-ui";
    inherit version src;
    sourceRoot = "${src.name}/src-ui";
    nodejs = nodejs_22; # engines: ^20.19 || ^22.12 || >=24
    # node_modules is derived from package-lock.json — no hash to maintain.
    npmDeps = importNpmLock { npmRoot = ../src-ui; };
    npmConfigHook = importNpmLock.npmConfigHook;
    # default buildPhase runs `npm run build` (vite build)
    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
    doCheck = false; # vitest/jsdom suite; enable if you want it in CI
  };
in
rustPlatform.buildRustPackage {
  pname = "smriti";
  inherit version src;

  # Workspace lockfile at the repo root; importCargoLock vendors it without
  # a fixed-output hash. All dependencies are crates.io (no outputHashes).
  cargoLock.lockFile = ../Cargo.lock;

  # Cargo.lock lives at the workspace root; the Tauri crate is a member.
  buildAndTestSubdir = "src-tauri";

  # Upstream release builds enable HEIC (libheif-rs, API pinned to v1_17).
  cargoBuildFeatures = [ "heic" ];

  # Upstream's own CI override: { "build": { "beforeBuildCommand": "" } }
  # Path is relative to src-tauri/, where the hook runs `cargo tauri build`.
  tauriBuildFlags = [ "--config" "tauri.release.conf.json" ];

  postPatch = ''
    mkdir -p src-ui/dist
    cp -r ${frontend}/. src-ui/dist/
  '';

  nativeBuildInputs = [
    cargo-tauri.hook   # builds the deb bundle, installs usr/* into $out
    pkg-config
    wrapGAppsHook3
  ];

  buildInputs = [
    glib
    glib-networking  # TLS inside the webview (OSM tiles)
    gtk3
    cairo
    gdk-pixbuf
    pango
    webkitgtk_4_1
    libsoup_3
    libheif
    librsvg
    dbus
    oniguruma
    sqlite
  ] ++ (with gst_all_1; [
    gst-plugins-base
    gst-plugins-good
    gst-plugins-bad
    gst-libav
  ]);

  # wrapGAppsHook3 consumes gappsWrapperArgs; the tauri hook already adds
  # WEBKIT_GST_ALLOWED_URI_PROTOCOLS=asset, its gst plugin, and the NVIDIA fix.
  postFixup = ''
    gappsWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [ xdg-utils dbus ]})
    # runtime.rs resolves ORT_DYLIB_PATH first, so nixpkgs' ONNX Runtime
    # replaces the prebuilt one from the in-app pack (models stay in-app).
    gappsWrapperArgs+=(--set ORT_DYLIB_PATH ${onnxruntime}/lib/libonnxruntime.so)
  ''
  + lib.optionalString (extraWrapperArgs != [ ]) ''
    gappsWrapperArgs+=(${lib.escapeShellArgs extraWrapperArgs})
  '';

  # Engine tests are offline, but the full suite touches D-Bus/keyring and
  # opt-in network-share tests; keep the sandbox build deterministic.
  doCheck = false;

  passthru = { inherit frontend src; };

  meta = {
    description = "Smriti — offline-first desktop photo library with on-device face recognition";
    homepage = "https://chivukulavirinchi.github.io/photovault/";
    changelog = "https://github.com/ChivukulaVirinchi/photovault/blob/v${version}/CHANGELOG.md";
    license = lib.licenses.asl20;
    mainProgram = "smriti-tauri";
    platforms = lib.platforms.linux;
  };
}
