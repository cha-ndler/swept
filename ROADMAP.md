# mac-cleaner roadmap

This is the autonomous development loop's compass. Each iteration: pick the first
unchecked item, complete it to its **verifiable goal** via the `/loop` workflow
(test-first → clippy/fmt → deletion-safety-reviewer if deletion logic changes →
verifier → PR → CI green → merge), check it off here, then start the next.

**Production-done = every item below checked.** (A native GUI is explicitly out
of scope for the first production release; this milestone is a complete, safe CLI.)

## Done
- [x] Safety substrate + Claude Code harness (PR #1)
- [x] Age-based cleanup filter `--older-than-days` (PR #2)
- [x] `--json` structured scan output (PR #3)
- [x] Cleaner-category registry + Homebrew downloads (PR #4)
- [x] Large-old-files finder (`--min-size`) (PR #5)

## Backlog (in order)
- [ ] **Empty-Trash cleaner** — treat `~/.Trash` as a first-class cleaner with
  its own count/size, plus `macclean empty-trash` convenience. *Goal: dry-run
  default, consent-gated, audited; tested via DirSink.*
- [ ] **Startup/login-items inspector** — read-only scan of `~/Library/
  LaunchAgents` reporting enabled login items as performance recommendations.
  *Goal: parses a fixture LaunchAgents dir; never modifies anything.*
- [ ] **Property tests for the safety kernel** — `proptest` fuzzing the invariant
  "no protected path ever yields a `SafePath`" and "allowlist ⊄ denylist". *Goal:
  proptest cases pass in CI.*
- [ ] **CI/release hardening** — bump `actions/checkout@v5`, add a release build
  job producing a `macclean` binary artifact, add a status badge. *Goal: CI green
  on the new workflow; artifact uploaded.*
- [ ] **Docs + v0.1.0** — finalize README usage, add CHANGELOG, tag `v0.1.0`.
  *Goal: docs match the shipped CLI; tag pushed.*
