# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/idnwit/lattice/releases/tag/lattice-v0.1.0) - 2026-11-27

### Added

- *(deck)* pair workers from the rail invite dialog
- *(cli)* add lattice attach for terminal instances
- *(pair)* pair decks and workers by invite command
- run apps as wasm packages and add the m1 poc deck shell
- add platform layer 1 skeleton crates

### Fixed

- *(cli)* report a failed instance in attach
- use one per-user runtime dir
- *(pair)* deliver the worker finish before exit
- *(cli)* let the worker pick the probe cwd and report a failed spawn
- *(pair)* confirm pairing with PairDone and bound each exchange
- *(cli)* restore the tty when attach is killed

### Other

- *(cli)* move the probe commands into a module
- fix the package cache path doc and lock the daemon build
- *(latticed)* resume a session across a latticed restart
