{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {inherit system overlays;};
      rustToolchain =
        pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
      };
      rustWithSrc = rustToolchain.override {
        extensions = ["rust-analyzer" "rust-src"];
      };
      deps = [pkgs.openssl];
    in
      with pkgs; {
        defaultPackage = rustPlatform.buildRustPackage {
          name = "ursa-minor";
          src = ./.;
          cargoLock = {lockFileContents = builtins.readFile ./Cargo.lock;};
          buildInputs = deps;
          nativeBuildInputs = [pkgs.pkg-config];
          env = {GIT_HASH = self.rev or self.dirtyRev or "nix-dirty";};

          meta = {
            mainProgram = "ursa-minor";
            license = lib.licenses.agpl3Only;
          };
        };
        devShells.default = mkShell {
          buildInputs =
            [pkg-config rustWithSrc sccache cargo-make]
            ++ deps;

          shellHook = ''
            export RUSTC_WRAPPER="${sccache}/bin/sccache"
          '';
        };
        formatter = alejandra;
      });
}
