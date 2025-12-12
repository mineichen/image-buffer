{
  description = "Deterministic Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, flake-utils, fenix, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rust = with fenix.packages.${system}; combine [
          stable.toolchain
        ];
        rustNightlyWithMiri = with fenix.packages.${system}; combine [
          (latest.withComponents [
            "rustc"
            "cargo"
            "miri"
          ])
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rust
            pkgs.bashInteractive
          ];

          LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib";

          shellHook = ''
            echo "===================================="
            echo " Welcome to the deterministic dev shell! "
            echo "===================================="
            echo "Rust toolchain:"
            rustc --version
            echo "Cargo version:"
            cargo --version
            echo "LD_LIBRARY_PATH: $LD_LIBRARY_PATH"
            echo "===================================="
            echo "Ready to develop! 🦀"
          '';
        };

            apps.miri = {
              type = "app";
              program = toString (pkgs.writeShellScript "miri" ''
                export PATH="${rustNightlyWithMiri}/bin:${pkgs.openssl.out}/bin:$PATH"
                export LD_LIBRARY_PATH="${pkgs.openssl.out}/lib"
                exec ${rustNightlyWithMiri}/bin/cargo miri test
              '');
            };
      });
}
