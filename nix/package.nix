{
  lib,
  fetchFromGitHub,
  rustPlatform,
  buildNpmPackage,
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
  # Optional integrations (see module.nix):
  smriti-assets ? null,   # patched asset pack -> SMRITI_ASSET_DIR
  onnxruntime ? null,     # e.g. pkgs.onnxruntime on aarch64 -> ORT_DYLIB_PATH
  extraWrapperArgs ? [ ],
}:

let
  version = "0.5.0";

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
    npmDepsHash = "sha256-S/sxP8vpDj4D+S/TiRYpwfhovfPw2+2K4q/lONpk2vk="; # TODO: fill
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

  cargoHash = "sha256-sI84P5eZD51oLQXx4Z09GpGGn9Ixu91VDKZD246eHCU="; # TODO: fill

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
  ''
  + lib.optionalString (smriti-assets != null) ''
    gappsWrapperArgs+=(--set SMRITI_ASSET_DIR ${smriti-assets})
  ''
  + lib.optionalString (onnxruntime != null) ''
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
