# Home-manager module for Abrax speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ abrax.homeManagerModules.default ];
#        services.abrax.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.abrax;
in
{
  options.services.abrax = {
    enable = lib.mkEnableOption "Abrax speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "abrax.packages.\${system}.abrax";
      description = "The Abrax package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.abrax = {
      Unit = {
        Description = "Abrax speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/abrax";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
