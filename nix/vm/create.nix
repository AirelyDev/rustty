{
  system,
  nixpkgs,
  overlay,
  module,
  common ? ./common.nix,
  uid ? 1000,
  gid ? 1000,
}: let
  pkgs = import nixpkgs {
    inherit system;
    overlays = [
      overlay
    ];
  };
in
  nixpkgs.lib.nixosSystem {
    system = builtins.replaceStrings ["darwin"] ["linux"] system;
    modules = [
      {
        virtualisation.vmVariant = {
          virtualisation.host.pkgs = pkgs;
        };

        nixpkgs.overlays = [
          overlay
        ];

        users.groups.rustty = {
          gid = gid;
        };

        users.users.rustty = {
          uid = uid;
        };

        system.stateVersion = nixpkgs.lib.trivial.release;
      }
      common
      module
    ];
  }
