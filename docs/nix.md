# Nix

anime-notif is developed as a Nix flake and is meant to be installed the
same way, whether or not you use NixOS.

> **Want desktop notifications? Use the home-manager module, not the NixOS
> module, to run `serve`.** Desktop notifications go over your login
> session's D-Bus bus. The NixOS module runs `anime-notif serve` as a
> `systemd --system` service under a dedicated system user — that user has
> no session bus, so every notification attempt fails (silently, into the
> journal — nothing crashes, nothing on your screen either). The
> home-manager module runs it as *your* `systemd --user` service instead,
> which does have your session bus. See "home-manager" below. The NixOS
> module is still the right choice if you only want polling/auto-download
> without notifications, or you're intentionally running this on a
> headless box.

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
`/var/lib/anime-notif`). No desktop notifications from this path — see the
callout at the top of this page.

The module also puts `cfg.package` on `environment.systemPackages`, so the
`anime-notif` CLI (`list`/`<show> set ...`/`source test`/...) is available
interactively regardless of whether you use this module or home-manager's
to run the daemon itself.

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
it as a `systemd --user` service on Linux, or a `launchd` agent on macOS —
**this is the path that gets you desktop notifications**, since it runs
inside your own login session.

If you also want to keep the daemon's state/downloads under a shared
system location rather than your home directory, set
`settings.downloads.base_dir` and `settings.general.db.path` accordingly;
running as your user just decides who the *process* runs as, not where its
files live.

The service is ordered `After`/`PartOf`/`WantedBy` **`graphical-session.target`**
(not `default.target`, which is reached very early in the user session,
often before the desktop's notification daemon exists) — this matters
because starting before the notification service is registered on the
session bus isn't just a delay: any notification attempted in that window
fails outright (`ServiceUnknown: The name is not activatable`), and since
the episode is already durably recorded by the time the notification is
attempted, that specific episode's notification is gone for good rather
than retried on the next poll. This narrows the boot/login race but can't
close it entirely (nothing guarantees the notification daemon reaches
`graphical-session.target` before anime-notif does); a session manager
that never imports `graphical-session.target` at all (uncommon outside
minimal/manual WM setups) means the service needs a manual
`systemctl --user start anime-notif` or a different target — override
`systemd.user.services.anime-notif.Install.WantedBy` in that case.

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
