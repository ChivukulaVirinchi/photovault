{
  description = "Smriti (formerly PhotoVault) — offline-first desktop photo library";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ self.overlays.default ];
        # The asset pack contains InsightFace pretrained models (non-commercial terms).
        config.allowUnfreePredicate = pkg:
          builtins.elem (nixpkgs.lib.getName pkg) [ "smriti-assets" ];
      };
    in
    {
        overlays.default = final: _prev: {
          smriti = final.callPackage ./nix/package.nix {
            # callPackage would otherwise auto-inject these from pkgs despite the
            # `? null` defaults in package.nix. Keep the base app lean; the NixOS
            # module opts in via .override.
            smriti-assets = null;
            onnxruntime = null;
          };
          smriti-assets = final.callPackage ./nix/assets.nix { };
        };

      packages = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          default = pkgs.smriti;
          smriti = pkgs.smriti;
          smriti-assets = pkgs.smriti-assets;
        });

      # Applying this module also applies the overlay, so `programs.smriti`
      # works with plain nixpkgs.
      nixosModules.default = {
        imports = [ ./nix/module.nix ];
        nixpkgs.overlays = [ self.overlays.default ];
      };

      devShells = forAllSystems (system: {
        default = (pkgsFor system).callPackage ./nix/shell.nix { };
      });
    };
}
