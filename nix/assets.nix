{
  lib,
  stdenv,
  fetchurl,
  unzip,
  autoPatchelfHook,
  zlib,
}:

stdenv.mkDerivation {
  pname = "smriti-assets";
  version = "0.5.0";

  # Hash from the official SHA256SUMS published with the v0.5.0 release.
  src = fetchurl {
    url = "https://github.com/ChivukulaVirinchi/photovault/releases/download/v0.5.0/Smriti-Assets.zip";
    hash = "sha256-4lydaWskajFo65jM60cqZF6ZlLS8g7bm3DJ+ADCGU/w=";
  };

  # Zip has multiple top-level dirs (models/, data/, libs/); stay in the
  # build dir instead of letting unpackPhase guess a single root.
  sourceRoot = ".";

  nativeBuildInputs = [ unzip autoPatchelfHook ];
  buildInputs = [ stdenv.cc.cc.lib zlib ]; # libstdc++/libgcc_s for the prebuilt ORT

  dontBuild = true;
  dontStrip = true; # models are data; leave the .so as shipped

  # Keep the layout bootstrap.rs expects: models/, data/, libs/onnxruntime/linux/.
  # Drop the Windows/macOS runtimes (~100 MB of dead weight on Linux).
  installPhase = ''
    runHook preInstall
    mkdir -p $out/libs/onnxruntime
    cp -a models data $out/
    cp -a libs/onnxruntime/linux $out/libs/onnxruntime/
    runHook postInstall
  '';

  meta = {
    description = "Smriti smart-features pack: SCRFD + AdaFace models, GeoNames DB, ONNX Runtime 1.23.0";
    homepage = "https://github.com/ChivukulaVirinchi/photovault";
    # InsightFace pretrained weights are licensed for non-commercial research
    # only; GeoNames is CC-BY-4.0; ONNX Runtime is MIT. See THIRD_PARTY_LICENSES.md.
    license = lib.licenses.unfreeRedistributable;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    platforms = [ "x86_64-linux" ]; # the pack's Linux ORT is x64-only
  };
}
