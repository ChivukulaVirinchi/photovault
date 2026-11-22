{ config, lib, pkgs, ... }:

let
  cfg = config.programs.smriti;
  finalPackage = cfg.package.override {
    smriti-assets = if cfg.smartFeatures.enable then cfg.smartFeatures.assets else null;
    inherit (cfg) extraWrapperArgs;
  };
in
{
  options.programs.smriti = {
    enable = lib.mkEnableOption "Smriti, an offline-first desktop photo library (no server component)";

    package = lib.mkPackageOption pkgs "smriti" { };

    smartFeatures = {
      enable = lib.mkEnableOption ''
        the optional asset pack (on-device face recognition, offline place
        names). ~300 MB, contains InsightFace models under non-commercial
        terms, so it requires allowUnfree for "smriti-assets". Visual-search
        CLIP models (~1.5 GB) are still downloaded in-app on first use.
      '';
      assets = lib.mkOption {
        type = lib.types.package;
        default = pkgs.smriti-assets;
        description = "Patched asset pack exposed to Smriti via SMRITI_ASSET_DIR.";
      };
    };

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
