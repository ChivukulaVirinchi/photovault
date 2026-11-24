{ config, lib, pkgs, ... }:

let
  cfg = config.programs.smriti;
  finalPackage = cfg.package.override {
    inherit (cfg) extraWrapperArgs;
  };
in
{
  options.programs.smriti = {
    enable = lib.mkEnableOption "Smriti, an offline-first desktop photo library (no server component)";

    package = lib.mkPackageOption pkgs "smriti" { };

    # No smartFeatures option: the face/places pack and CLIP models are
    # installed from inside the app on every platform. The package wraps
    # ORT_DYLIB_PATH to nixpkgs' onnxruntime instead of bundling a runtime.

    enableKeyring = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Enable GNOME Keyring (Secret Service) so Smriti can store the
        optional assistant API key. Disable if you use KWallet or another
        Secret Service provider.
      '';
    };

    extraWrapperArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "--set" "WEBKIT_DISABLE_DMABUF_RENDERER" "1" ];
      description = "Extra makeWrapper arguments, e.g. WebKit rendering workarounds.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ finalPackage ];
    services.gnome.gnome-keyring.enable = lib.mkIf cfg.enableKeyring (lib.mkDefault true);
  };
}
