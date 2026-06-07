# mac-cleaner

A free, open-source alternative to CleanMyMac. Finds junk to clean, recommends
performance tweaks, scans for app updates, and assists with clutter removal —
and **never deletes anything automatically**. It previews exactly what it would
remove and acts only on explicit consent.

> ⚠️ This is a data-destroying tool. Every destructive path is built around a
> safety substrate first (see [`CLAUDE.md`](CLAUDE.md) → SAFETY CONTRACT).

## Status

Safety substrate complete and tested; CLI preview/clean working. GUI deferred.

## Build & test

```bash
cargo test --workspace                                   # the oracle
cargo clippy --workspace --all-targets -- -D warnings    # must be clean
cargo run -p macclean -- scan                            # read-only preview
```

## Usage

```bash
macclean scan                 # preview junk in allowlisted locations (read-only)
macclean clean                # preview (still no changes without --execute)
macclean clean --execute      # move junk to the Trash (recoverable)
macclean clean --execute --yes        # confirm a mass delete
macclean clean --execute --permanent  # irreversible (per-action consent)
```

Every planned and executed action is written to an append-only audit log
(default `~/Library/Application Support/macclean/audit.jsonl`).

## Safety model

Three layers, in order of authority:

1. **Denylist** (`crates/safety/denylist.rs`) — refuses system roots, keychains,
   mail, the home root, anything inside `.git`. Checked first; always wins.
2. **Path guard** (`crates/safety/path_guard.rs`) — canonicalizes (resolving
   symlinks), rejects `..`, re-checks the denylist. The only constructor of
   `SafePath`; re-run immediately before every mutation (TOCTOU defense).
3. **Allowlist** (`crates/safety/allowlist.rs`) — confines cleanup to caches,
   logs, Xcode derived data, and the user Trash.

The executor (`crates/core/executor.rs`) is the only code that mutates the
filesystem, and only under explicit `Consent`. Default is a dry run.

## Layout

```
crates/safety  trust kernel (denylist, path guard, allowlist) — never deletes
crates/core    scanner → plan → executor → audit
crates/cli     macclean binary
```

## License

MIT
