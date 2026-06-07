# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.1.0] — 2026-06-07

First release: a safe, dry-run-first macOS junk-cleaning CLI built on a
property-tested safety substrate.

### Added
- **Safety substrate** (`crates/safety`): protected-path denylist (checked
  first), path guard (canonicalize → `SafePath`, `..`/TOCTOU defense), scoped
  allowlist. The only chokepoint every destructive op passes through.
- **Engine** (`crates/core`): read-only scanner → dry-run plan → consent-gated
  executor (Trash by default) → append-only JSONL audit log.
- **CLI** (`macclean`): `scan` (preview) and `clean` (dry-run unless
  `--execute`; `--permanent` and `--yes` gate the dangerous paths).
- `--older-than-days` age filter and `--min-size` large-files filter (composable).
- `--json` structured scan output (stable wire contract for tooling / a future GUI).
- Cleaner-category registry (application caches, logs, Xcode derived data, the
  user Trash, Homebrew downloads) with names + descriptions.
- `login-items` — read-only review of `~/Library/LaunchAgents` startup items.
- Property-based tests (`proptest`) fuzzing the safety-kernel invariants.
- CI on macOS (fmt + clippy `-D warnings` + tests + a "no real `$HOME` in tests"
  guard) and a release-build job publishing the `macclean` binary artifact.

### Safety
- Dry-run is the default; destructive actions require explicit consent.
- Recursive/large removals require confirmation; audit failures abort the run.
- Tests run only against throwaway temp-dir fixtures.

[0.1.0]: https://github.com/cha-ndler/mac-cleaner/releases/tag/v0.1.0
