# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/idnwit/lattice/releases/tag/lattice-deck-core-v0.1.0) - 2026-11-27

### Added

- *(deck)* reconnect the worker link and show its status
- *(deck)* pair workers from the rail invite dialog
- *(pair)* pair decks and workers by invite command
- run apps as wasm packages and add the m1 poc deck shell
- add platform layer 1 skeleton crates

### Fixed

- *(pair)* deliver the worker finish before exit
- *(pair)* confirm pairing with PairDone and bound each exchange
- *(transport)* close connections refused by peer policy
