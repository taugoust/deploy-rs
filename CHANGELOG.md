<!--
SPDX-FileCopyrightText: 2026 Serokell <https://serokell.io/>

SPDX-License-Identifier: MPL-2.0
-->

# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `deploy --keep-going` keeps deploying the other nodes when a node fails to
  build, push or activate, and lists the failed nodes at the end. (#221)

### Changed

- `deploy --groups` only evaluates nodes that can match the given groups,
  instead of evaluating every node and filtering afterwards.

### Fixed

- Activation creates the parent directory of a custom `profilePath`, so the
  first deploy to a new location no longer fails in `nix-env --set`.

## [1.0.0] - 2026-10-19

### Added

- First versioned release of deploy-rs, and the first release on
  [crates.io](https://crates.io/crates/deploy-rs). (#407)

### Changed

- `deploy --version` and `activate --version` print the full version from
  `Cargo.toml`, `1.0.0`, instead of `1.0`. (#408)

[Unreleased]: https://github.com/serokell/deploy-rs/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/serokell/deploy-rs/releases/tag/v1.0.0
