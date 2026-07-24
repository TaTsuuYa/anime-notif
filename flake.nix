{
  description = "anime-notif — cross-platform anime release watcher, notifier and auto-downloader";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
      home-manager,
    }:
    let
      perSystem = flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rustfmt"
              "clippy"
            ];
          };
          craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain);

          # Source, filtered to what the build actually needs: Cargo files,
          # crate sources, migrations and the default icon (embedded via
          # include_str!/include_bytes!), and the example source plugin +
          # its test fixture (used by crates/fetch's integration test).
          src = pkgs.lib.cleanSourceWith {
            src = craneLib.path ./.;
            filter =
              path: type:
              (craneLib.filterCargoSources path type)
              || (pkgs.lib.hasInfix "/migrations/" path)
              || (pkgs.lib.hasInfix "/sources/" path)
              || (pkgs.lib.hasInfix "/tests/fixtures/" path)
              || (pkgs.lib.hasInfix "/assets/" path);
          };

          commonArgs = {
            inherit src;
            strictDeps = true;
            pname = "anime-notif";
            version = "0.1.0";
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.rustPlatform.bindgenHook # libsql-ffi builds SQLite via bindgen
            ];
            buildInputs = [
              pkgs.sqlite
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.libiconv
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          anime-notif = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              # The live-network subsplease test is #[ignore]'d and not run
              # by plain `cargo test`; everything else is offline
              # (in-memory sqlite, wiremock-mocked HTTP), so it's safe and
              # valuable to run the test suite as part of the build.
              doCheck = true;
            }
          );
        in
        {
          packages.default = anime-notif;
          packages.anime-notif = anime-notif;

          apps.default = flake-utils.lib.mkApp { drv = anime-notif; };

          checks = {
            inherit anime-notif;
            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets --all-features -- -D warnings";
              }
            );
            fmt = craneLib.cargoFmt {
              inherit src;
              pname = "anime-notif";
              version = "0.1.0";
            };
          }
          // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
            # Boots a VM, enables the NixOS module with no sources
            # configured (nothing to poll, but proves the module wires up a
            # working service: user/group creation, the systemd unit, and
            # the control server actually binding and logging).
            nixos-module = pkgs.testers.nixosTest {
              name = "anime-notif-nixos-module";
              nodes.machine =
                { ... }:
                {
                  imports = [ (import ./nix/modules/nixos.nix self) ];
                  services.anime-notif.enable = true;
                };
              testScript = ''
                machine.wait_for_unit("anime-notif.service")
                machine.sleep(2)
                machine.succeed("systemctl is-active anime-notif.service")
                journal = machine.succeed("journalctl -u anime-notif.service --no-pager")
                assert "control server listening" in journal, f"expected control server log line, got:\n{journal}"
                # The CLI must be usable interactively, independent of the
                # service itself (regression check: this was missing until
                # a real deployment caught it).
                machine.succeed("which anime-notif")
              '';
            };

            home-manager-module =
              (home-manager.lib.homeManagerConfiguration {
                inherit pkgs;
                modules = [
                  (import ./nix/modules/home-manager.nix self)
                  {
                    home.username = "test";
                    home.homeDirectory = "/home/test";
                    home.stateVersion = "24.05";
                    services.anime-notif.enable = true;
                  }
                ];
              }).activationPackage;
          };

          devShells.default = pkgs.mkShell {
            packages = [
              rustToolchain
              pkgs.pkg-config
              pkgs.sqlite
              pkgs.cargo-edit
              pkgs.cargo-watch
              pkgs.jq
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.dbus
            ];

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

            shellHook = ''
              export PKG_CONFIG_PATH="${pkgs.sqlite.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            '';
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
    in
    perSystem
    // {
      overlays.default = final: _prev: {
        anime-notif = self.packages.${final.system}.default;
      };

      nixosModules.default = import ./nix/modules/nixos.nix self;
      homeManagerModules.default = import ./nix/modules/home-manager.nix self;
    };
}
