# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/idnwit/lattice/releases/tag/lattice-worker-v0.1.0) - 2026-11-27

### Added

- *(pair)* pair decks and workers by invite command
- run apps as wasm packages and add the m1 poc deck shell
- add platform layer 1 skeleton crates

### Fixed

- use one per-user runtime dir
- *(worker)* restore the failed phase of an instance whose spawn failed
- *(pair)* deliver the worker finish before exit
- *(proc)* give shells a TERM and a clean env
- *(pair)* confirm pairing with PairDone and bound each exchange
- *(worker)* base versions on a per-boot epoch to survive restarts
- *(transport)* close connections refused by peer policy

### Other

- *(worker)* pin that a failed spawn stays listed until a close
