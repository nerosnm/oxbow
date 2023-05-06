{
  description = "Oxbow Twitch chat bot";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";

    cargo2nix.url = "github:cargo2nix/cargo2nix";
    cargo2nix.inputs.flake-utils.follows = "flake-utils";
    cargo2nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , ...
    } @ inputs:
    let
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [
          inputs.cargo2nix.overlays.default
          inputs.fenix.overlays.default

          (final: prev: {
            rust-toolchain =
              let
                inherit (final.lib.strings) fileContents;

                stableFor = target: target.toolchainOf {
                  channel = fileContents ./rust-toolchain;
                  sha256 = "sha256-eMJethw5ZLrJHmoN2/l0bIyQjoTX1NsvalWSscTixpI=";
                };

                rustfmt = final.fenix.latest.rustfmt;
              in
              final.fenix.combine [
                rustfmt
                (stableFor final.fenix).toolchain
              ];
          })

          (final: prev: {
            cargo2nix = inputs.cargo2nix.packages.${system}.default;
          })
        ];
      };

      supportedSystems = with flake-utils.lib.system; [
        aarch64-darwin
        x86_64-darwin
        x86_64-linux
      ];

      inherit (flake-utils.lib) eachSystem;
    in
    eachSystem supportedSystems (system:
    let
      pkgs = pkgsFor system;

      rustPkgs = pkgs.rustBuilder.makePackageSet {
        packageFun = import ./Cargo.nix;
        rustToolchain = pkgs.rust-toolchain;

        packageOverrides = pkgs: pkgs.rustBuilder.overrides.all ++ [
          (pkgs.rustBuilder.rustLib.makeOverride {
            name = "refinery-macros";
            overrideAttrs = drv: {
              propagatedNativeBuildInputs = drv.propagatedNativeBuildInputs or [ ] ++ [
                pkgs.sqlite
              ];
            };
          })

          (pkgs.rustBuilder.rustLib.makeOverride {
            name = "oxbow";
            overrideAttrs = drv: {
              propagatedNativeBuildInputs = drv.propagatedNativeBuildInputs or [ ] ++ [
                pkgs.sqlite
              ];
            };
          })
        ];
      };

      inherit (pkgs.lib) optionals;
    in
    rec
    {
      packages = rec {
        default = oxbow;
        oxbow = (rustPkgs.workspace.oxbow { }).bin;
        oxbow-cacti-dev = pkgs.callPackage ./site/default.nix { };
      };

      apps = rec {
        oxbow = flake-utils.lib.mkApp {
          drv = packages.oxbow;
        };
        default = oxbow;
      };

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo2nix
          clang
          libiconv
          nixpkgs-fmt
          openssl
          pkg-config
          rust-toolchain
          sqlite
          zola
        ] ++ optionals stdenv.isDarwin (with darwin.apple_sdk.frameworks; [
          # Darwin-only dependencies
          Security
        ]);
      };

      formatter = pkgs.nixpkgs-fmt;

      checks = {
        format = pkgs.runCommand
          "check-nix-format"
          { buildInputs = [ pkgs.nixpkgs-fmt ]; }
          ''
            ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt --check ${./.}
            touch $out
          '';
      };
    });
}
