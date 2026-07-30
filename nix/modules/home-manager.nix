self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.anime-notif;
  format = pkgs.formats.toml { };
  configFile = format.generate "anime-notif-config.toml" cfg.settings;
in
{
  options.services.anime-notif = {
    enable = lib.mkEnableOption "the anime-notif background service";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "anime-notif.packages.<system>.default";
      description = "The anime-notif package to run.";
    };

    settings = lib.mkOption {
      type = format.type;
      default = { };
      description = ''
        anime-notif's `config.toml`, expressed as Nix — see `docs/config.md`
        for the full schema. Written into `$XDG_CONFIG_HOME/anime-notif` and
        read-only at runtime: per-show state lives in the database, mutated
        by the CLI, not here.
      '';
      example = lib.literalExpression ''
        {
          downloads.base_dir = "$HOME/anime";
          categories = [
            { name = "liked"; notify = true; auto_download = true; }
          ];
          sources = [ "$HOME/.config/anime-notif/sources/subsplease.toml" ];
        }
      '';
    };

    logLevel = lib.mkOption {
      type = lib.types.str;
      default = "info";
      description = ''
        Value of `RUST_LOG` for the service, e.g. `"debug"`. anime-notif
        logs nothing at all without this set.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    xdg.configFile."anime-notif/config.toml".source = configFile;

    home.packages = [ cfg.package ];

    systemd.user.services.anime-notif = lib.mkIf pkgs.stdenv.isLinux {
      Unit = {
        Description = "anime-notif background service";
        # graphical-session.target (not default.target, which is reached
        # very early in the user session) so this starts after the desktop
        # session -- and with it, the org.freedesktop.Notifications
        # service -- is actually up. Starting before that exists is a real,
        # observed failure mode: notifications attempted in that window
        # fail with "ServiceUnknown: The name is not activatable", and
        # (since the episode is already durably recorded by then) that
        # notification is gone for good, not just delayed. This narrows
        # the race but can't eliminate it outright, since nothing
        # guarantees the notification daemon itself is the first thing to
        # reach graphical-session.target -- see the non-fatal notify-error
        # handling in `anime_notif_daemon::engine::send_notification` for
        # the rest of the mitigation.
        After = [
          "network-online.target"
          "graphical-session.target"
        ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${lib.getExe cfg.package} serve";
        Environment = "RUST_LOG=${cfg.logLevel}";
        Restart = "on-failure";
        RestartSec = 10;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };

    launchd.agents.anime-notif = lib.mkIf pkgs.stdenv.isDarwin {
      enable = true;
      config = {
        ProgramArguments = [
          (lib.getExe cfg.package)
          "serve"
        ];
        EnvironmentVariables.RUST_LOG = cfg.logLevel;
        RunAtLoad = true;
        KeepAlive = true;
        StandardOutPath = "${config.xdg.cacheHome}/anime-notif/serve.log";
        StandardErrorPath = "${config.xdg.cacheHome}/anime-notif/serve.err.log";
      };
    };
  };
}
