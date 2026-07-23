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
      default = self.packages.${pkgs.system}.default;
      defaultText = lib.literalExpression "anime-notif.packages.<system>.default";
      description = "The anime-notif package to run.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "anime-notif";
      description = "User the service runs as. Created automatically if it's the default.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "anime-notif";
      description = "Group the service runs as. Created automatically if it's the default.";
    };

    stateDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/anime-notif";
      description = ''
        Directory for the local database and caches. Also becomes the
        service user's home, so `general.db.path`'s XDG-relative default
        resolves under here unless overridden in `settings`.
      '';
    };

    logLevel = lib.mkOption {
      type = lib.types.str;
      default = "info";
      description = ''
        Value of `RUST_LOG` for the service, e.g. `"debug"` or
        `"anime_notif_daemon=debug,info"`. anime-notif logs nothing at all
        without this set, so the module defaults it to `"info"` rather
        than leaving a running service silent in the journal.
      '';
    };

    settings = lib.mkOption {
      type = format.type;
      default = { };
      description = ''
        anime-notif's `config.toml`, expressed as Nix — see `docs/config.md`
        for the full schema. Written to the Nix store and passed via
        `$ANIME_NOTIF_CONFIG`, so it is read-only at runtime: per-show state
        (category, alias, history) lives in the database, mutated by the
        CLI, not here.
      '';
      example = lib.literalExpression ''
        {
          downloads = {
            base_dir = "/mnt/media/anime";
            default_resolution = "1080";
          };
          categories = [
            { name = "liked"; notify = true; auto_download = true; }
            { name = "normal"; notify = true; auto_download = false; }
            { name = "uninterested"; notify = false; auto_download = false; }
          ];
          sources = [ "/etc/anime-notif/sources/subsplease.toml" ];
        }
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    users.users = lib.mkIf (cfg.user == "anime-notif") {
      anime-notif = {
        isSystemUser = true;
        group = cfg.group;
        home = cfg.stateDir;
        createHome = true;
      };
    };
    users.groups = lib.mkIf (cfg.group == "anime-notif") {
      anime-notif = { };
    };

    systemd.services.anime-notif = {
      description = "anime-notif background service";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      environment = {
        ANIME_NOTIF_CONFIG = configFile;
        RUST_LOG = cfg.logLevel;
      };

      serviceConfig = {
        ExecStart = "${lib.getExe cfg.package} serve";
        User = cfg.user;
        Group = cfg.group;
        StateDirectory = "anime-notif";
        WorkingDirectory = cfg.stateDir;
        Restart = "on-failure";
        RestartSec = 10;

        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ cfg.stateDir ];
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictNamespaces = true;
      };
    };
  };
}
