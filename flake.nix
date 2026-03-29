{
  description = "eRDFa Publish — Semantic UI components as DA51 CBOR shards";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages = {
          erdfa-publish = pkgs.rustPlatform.buildRustPackage {
            pname = "erdfa-publish";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };
            buildFeatures = [ "native" "cli" ];
            buildNoDefaultFeatures = true;
          };

          default = self.packages.${system}.erdfa-publish;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo rustc rust-analyzer rustfmt clippy
          ];
        };
      }
    );
}
