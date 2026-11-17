# SPDX-FileCopyrightText: 2026 Serokell <https://serokell.io/>
#
# SPDX-License-Identifier: MPL-2.0

# Returns a copy of `deploy` with only the nodes that can match one of
# `wanted` groups, so `deploy --groups` doesn't evaluate the other nodes.
#
# A node is kept if a group set on the deploy, the node or any of its profiles
# is wanted, the same sets deploy-rs merges for each profile. The binary still
# filters profile by profile afterwards. Only `groups` is read here, so the
# profiles of dropped nodes are never evaluated.
#
# The deploy-rs binary loads this file via `include_str!` from `src/cli.rs`.
# It is exercised by `nix/tests/filter-groups.nix`.
wanted: deploy:
let
  # `groups` may be a string or a list of strings
  toList = g: if builtins.isList g then g else [ g ];
  groupsOf = x: toList (x.groups or [ ]);
  anyWanted = builtins.any (g: builtins.elem g wanted);

  deployGroups = groupsOf deploy;
  keepNode =
    _: node:
    anyWanted (
      deployGroups
      ++ groupsOf node
      ++ builtins.concatMap groupsOf (builtins.attrValues (node.profiles or { }))
    );
in
if deploy ? nodes then
  deploy
  // {
    nodes = builtins.removeAttrs deploy.nodes (
      builtins.filter (name: !keepNode name deploy.nodes.${name}) (builtins.attrNames deploy.nodes)
    );
  }
else
  deploy
