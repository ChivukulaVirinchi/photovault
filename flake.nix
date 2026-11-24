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
      };
    in
    {
      overlays.default = final: _prev: {
        # callPackage auto-injects nixpkgs' onnxruntime; the package wrapper
        # points ORT_DYLIB_PATH at it. Models are installed in-app, not packaged.
        smriti = final.callPackage ./nix/package.nix { };
      };

      packages = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          default = pkgs.smriti;
          smriti = pkgs.smriti;
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
