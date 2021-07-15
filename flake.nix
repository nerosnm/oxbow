{
  description = "Stuck Overflow Algorithms";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = 
  { self
  , nixpkgs
  , rust-overlay
  , flake-utils
  , ... 
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rust-toolchain =
          (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain).override {
            extensions = [ "rust-src" ];
          };
      in with pkgs; {
        devShell = mkShell {
          nativeBuildInputs = [
            clang
            openssl
            pkg-config
            rust-toolchain
            sqlite
          ];
        };
      });
}
