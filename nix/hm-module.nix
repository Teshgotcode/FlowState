# Home-manager module for FlowState speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ flowstate.homeManagerModules.default ];
#        services.flowstate.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.flowstate;
in
{
  options.services.flowstate = {
    enable = lib.mkEnableOption "FlowState speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "flowstate.packages.\${system}.flowstate";
      description = "The FlowState package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.flowstate = {
      Unit = {
        Description = "FlowState speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/flowstate";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
