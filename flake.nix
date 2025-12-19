{
  description = "A flake for tug-record";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }@inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        buildToolchain = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = [ "rustc" "cargo" ];
        };

        devToolchain = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = buildToolchain;
          rustc = buildToolchain;
        };

        commonBuildArgs = {
          src = pkgs.lib.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
          buildInputs = [ pkgs.openssl pkgs.wmctrl ];
          doCheck = false;
        };

      in {
        packages = {
          tug-record = rustPlatform.buildRustPackage (commonBuildArgs // {
            pname = "tug-record";
            version = "0.1.0";

            postInstall = ''
              wrapProgram $out/bin/tug-diff-editor \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.wmctrl ]}
            '';
          });

          tug-stats = rustPlatform.buildRustPackage (commonBuildArgs // {
            pname = "tug-stats";
            version = "0.1.0";
            buildAndTestSubdir = "tug-stats";
          });

          default = pkgs.symlinkJoin {
            name = "tug-workspace";
            paths = [
              self.packages.${system}.tug-record
              self.packages.${system}.tug-stats
            ];
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            devToolchain
            openssl
            pkg-config
            wmctrl
            bacon
            jujutsu
          ];

          shellHook = ''
            export PATH="$PATH:$(pwd)/bin/";
            [ -f .localrc ] && source .localrc
          '';
        };
      });
}
