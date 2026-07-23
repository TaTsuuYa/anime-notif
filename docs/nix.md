# Nix

anime-notif is developed as a Nix flake and is meant to be installed the
same way, whether or not you use NixOS.

## Flake outputs

| Output | What it is |
|---|---|
| `packages.<system>.default` (`anime-notif`) | The binary, built with [crane](https://github.com/ipetkov/crane). |
| `apps.<system>.default` | `nix run github:TaTsuuYa/anime-notif -- list` etc. |
| `overlays.default` | Adds `anime-notif` to `pkgs`. |
| `nixosModules.default` | `services.anime-notif` for NixOS. |
| `homeManagerModules.default` | `services.anime-notif` for home-manager (Linux systemd user service, or a launchd agent on macOS). |
| `devShells.default` | The development environment — see the top-level `README.md`. |
| `checks` | `anime-notif` (build, including the offline test suite — the live-network test is `#[ignore]`d so it doesn't run here), `clippy` (`-D warnings`), `fmt`. Run with `nix flake check`. |

## NixOS

```nix
{
  inputs.anime-notif.url = "github:TaTsuuYa/anime-notif";

  outputs = { self, nixpkgs, anime-notif, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        anime-notif.nixosModules.default
        {
          services.anime-notif = {
            enable = true;
            settings = {
              downloads = {
                base_dir = "/mnt/media/anime";
                default_resolution = "1080";
                default_method = "torrent";
              };
              categories = [
                { name = "liked"; notify = true; auto_download = true; }
                { name = "normal"; notify = true; auto_download = false; }
                { name = "uninterested"; notify = false; auto_download = false; }
              ];
              sources = [ "/etc/anime-notif/sources/subsplease.toml" ];
            };
          };
        }
      ];
    };
  };
}
```

This runs `anime-notif serve` as a systemd service under a dedicated
`anime-notif` system user (created automatically), with the config written
to the Nix store and passed via `$ANIME_NOTIF_CONFIG` — see
`services.anime-notif.settings`'s full shape in `docs/config.md`; it maps
1:1 onto `config.toml`. The unit is reasonably sandboxed
(`ProtectSystem = "strict"`, `ProtectHome`, `NoNewPrivileges`, ...), with
write access only to `services.anime-notif.stateDir` (default
`/var/lib/anime-notif`).

Since `settings` is read-only Nix-store content, day-to-day show
management (`anime-notif <show> set category liked`, `rm`, ...) still goes
through the CLI as `anime-notif` (or `sudo -u anime-notif anime-notif`, per
your user's permissions) against the database — see `docs/cli.md`. The
`categories add/rm`/`source add` commands that rewrite `config.toml`
**will fail** against a NixOS-managed config (it's a read-only store path)
— add categories/sources via `settings` in your Nix configuration instead.

## home-manager

```nix
{
  imports = [ anime-notif.homeManagerModules.default ];
  services.anime-notif = {
    enable = true;
    settings = {
      downloads.base_dir = "${config.home.homeDirectory}/anime";
      sources = [ "${config.home.homeDirectory}/.config/anime-notif/sources/subsplease.toml" ];
    };
  };
}
```

Writes `config.toml` to `$XDG_CONFIG_HOME/anime-notif/config.toml` and runs
it as a `systemd --user` service on Linux, or a `launchd` agent on macOS.

## Source plugins from another flake

A source plugin is just a file, so another repo can publish one as a flake
package output and you reference it directly — no runtime fetch, fully
reproducible:

```nix
# consuming flake
inputs.nyaa-source.url = "github:someone/anime-notif-nyaa-source";

# ...
services.anime-notif.settings.sources = [
  "/etc/anime-notif/sources/subsplease.toml"  # local/declarative path
  inputs.nyaa-source.packages.${system}.default  # a store path a plugin author ships
];
```

The NixOS/home-manager modules accept `sources` entries as plain strings
(paths or URLs — see `docs/sources.md`) or Nix store paths/derivations
interchangeably, since `pkgs.formats.toml`'s generator serializes a
derivation the same way it would any other path value.

## Without Nix

The binary has no hard runtime dependency on Nix — see the top-level
`README.md`/`docs/cli.md` for running it directly, or building release
binaries per OS (`docs/architecture.md`'s milestone list covers this).
