# SPDX-FileCopyrightText: 2026 Serokell <https://serokell.io/>
#
# SPDX-License-Identifier: MPL-2.0

# Pure-evaluation test for `../filter-groups.nix`.
#
# Asserts:
#   - nodes match on groups set on the node, on a profile or on the deploy.
#   - `groups` can be a string or a list.
#   - other nodes are dropped without evaluating their profiles.
#   - top-level deploy attrs are preserved.

{ pkgs }:
let
  filterGroups = import ../filter-groups.nix;

  # Evaluating this profile would fail the test
  unevaluated = throw "a node outside the wanted groups was evaluated";

  deploy = {
    sshUser = "deployer";
    nodes = {
      web = {
        hostname = "web.example";
        groups = "web";
        profiles.system.path = "/nix/store/0000000000000000000000000000000000-web";
      };
      db = {
        hostname = "db.example";
        profiles.system = {
          groups = [
            "db"
            "prod"
          ];
          path = "/nix/store/0000000000000000000000000000000000-db";
        };
      };
      other = {
        hostname = "other.example";
        groups = [ "lab" ];
        profiles.system.path = unevaluated;
      };
    };
  };

  names = d: builtins.attrNames d.nodes;
  forced = d: builtins.deepSeq d d;

  web = forced (filterGroups [ "web" ] deploy);
  prod = forced (filterGroups [ "prod" "web" ] deploy);
  none = forced (filterGroups [ "missing" ] deploy);
  everyone = filterGroups [ "all" ] (deploy // { groups = "all"; });
in
assert names web == [ "web" ];
assert
  names prod == [
    "db"
    "web"
  ];
assert names none == [ ];
assert
  names everyone == [
    "db"
    "other"
    "web"
  ];
assert web.sshUser == "deployer";

pkgs.runCommand "filter-groups-test" { } "touch $out"
