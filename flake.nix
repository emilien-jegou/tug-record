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
      in {
        packages = {
          tug-record = rustPlatform.buildRustPackage {
            pname = "tug-record";
            version = "0.1.0";

            src = pkgs.lib.cleanSource ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
            buildInputs = [ pkgs.openssl pkgs.wmctrl ];

            doCheck = false;

            postInstall = ''
              wrapProgram $out/bin/tug-diff-editor \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.wmctrl ]}
            '';
          };

          default = self.packages.${system}.tug-record;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            devToolchain # Uses the heavy toolchain only in dev
            openssl
            pkg-config
            wmctrl
            bacon
          ];

          shellHook = ''
            export PATH="$PATH:$(pwd)/bin/";
            [ -f .localrc ] && source .localrc
          '';
        };
      });
}
