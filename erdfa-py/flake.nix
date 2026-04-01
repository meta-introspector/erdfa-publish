{
  description = "erdfa-py — Python bindings for erdfa-publish";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system}; in {
        devShells.default = pkgs.mkShell {
          buildInputs = [ pkgs.python312 pkgs.cargo pkgs.rustc pkgs.maturin ];
          shellHook = ''
            export VIRTUAL_ENV="$PWD/.dev-venv"
            if [ ! -d "$VIRTUAL_ENV" ]; then
              python3 -m venv "$VIRTUAL_ENV"
            fi
            export PATH="$VIRTUAL_ENV/bin:$PATH"
            export PYTHONPATH="$VIRTUAL_ENV/lib/python3.12/site-packages:$PYTHONPATH"
          '';
        };
      }
    );
}
