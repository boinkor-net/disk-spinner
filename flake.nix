{
  outputs = inputs @ {
    self,
    flake-parts,
    nixpkgs,
    fenix,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      imports = [
        inputs.devshell.flakeModule
        inputs.flake-parts.flakeModules.easyOverlay
      ];

      perSystem = {
        config,
        pkgs,
        final,
        system,
        ...
      }: let
        cIncludes =
          if (! pkgs.stdenv.isDarwin)
          then [pkgs.udev]
          else [];
        cLibs =
          if pkgs.stdenv.isDarwin
          then [pkgs.libiconv]
          else [];
      in {
        formatter = pkgs.alejandra;

        packages.default = config.packages.disk-spinner;
        packages.disk-spinner = let
          rustPlatform = pkgs.makeRustPlatform {
            inherit (fenix.packages.${system}.stable) rustc cargo;
          };
          nativeBuildInputs =
            (builtins.map (l: pkgs.lib.getDev l) cIncludes)
            ++ cIncludes
            ++ cLibs
            ++ [pkgs.pkg-config];
        in
          rustPlatform.buildRustPackage {
            pname = "disk-spinner";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            inherit nativeBuildInputs;
            buildInputs = nativeBuildInputs;
            src = let
              fs = pkgs.lib.fileset;
            in
              fs.toSource {
                root = ./.;
                fileset = fs.unions [
                  ./Cargo.toml
                  ./Cargo.lock
                  ./src
                ];
              };
            doCheck = false; # The sandbox blocks io_uring, which makes testing this program impossible.
            cargoLock.lockFile = ./Cargo.lock;
            meta.mainProgram = "disk-spinner";
          };

        apps = {
          default = config.apps.disk-spinner;
          disk-spinner.program = config.packages.disk-spinner;
        };

        devshells = {
          default = {
            imports = [
              "${inputs.devshell}/extra/language/rust.nix"
              "${inputs.devshell}/extra/language/c.nix"
            ];
            packages =
              [fenix.packages.${system}.stable.rust-analyzer]
              ++ (pkgs.lib.optionals pkgs.stdenv.isLinux [
                pkgs.perf-tools
                pkgs.linuxKernel.packages.linux_latest_libre.perf
              ]);
            language.rust = {
              enableDefaultToolchain = false;
              packageSet = fenix.packages.${system}.stable;
              tools = ["rust-analyzer" "cargo" "clippy" "rustfmt" "rustc"];
            };

            language.c.includes = cIncludes;
            language.c.libraries = cLibs;
          };
        };

        overlayAttrs = {inherit (config.packages) disk-spinner;};
      };
    };

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell.url = "github:numtide/devshell";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
}
