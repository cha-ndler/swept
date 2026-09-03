# mac-cleaner

A free, open-source CleanMyMac alternative: finds junk to clean, recommends
performance tweaks, scans for app updates, assists with clutter removal. It acts
only on items that are safe to remove and **never deletes anything
automatically** — it previews exactly what it would remove and acts only on
explicit consent.

This is a **data-destroying tool**. Safety beats features, always. When in doubt,
refuse and preview.

---

## Commands (run these — don't rediscover them)

| Task | Command |
|------|---------|
| **The full gate (do this before every PR)** | `./scripts/verify.sh` |
| Rust half only (the fast loop) | `./scripts/verify.sh --rust` |
| GUI half only | `./scripts/verify.sh --gui` |
| Test (the oracle) | `cargo test --workspace` |
| Lint (must be clean) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Format | `cargo fmt --all` (check: `cargo fmt --all --check`) |
| Safety-kernel tests only | `cargo test -p macclean-safety` |
| Preview a scan (read-only) | `cargo run -p macclean -- scan` |
| Build release | `cargo build --release` |

**A change is not done until `./scripts/verify.sh` passes.** It runs the same nine
gates CI does, in the same order, and names anything it skipped rather than
counting a skip as a pass. Never report success you haven't observed — run the
`verifier` subagent if unsure.

**Local verification is the primary oracle, not a rehearsal for CI.** GitHub
bills macOS runners at **10x** on a private repository, and this project cannot
move its jobs to Linux without giving up the thing they test (macOS path
semantics for the safety kernel; `*-darwin.png` baselines for the visual gate).
The monthly allowance is therefore a real constraint, and it has been exhausted
once already.

---

## The development loop (TDD — this is the default workflow)

Every change follows this loop. It is what makes autonomous iteration safe: the
tests are the correctness oracle.

1. **Write the test first**, against a throwaway tempdir fixture (`tempfile::tempdir()`). Never a real path.
2. Run `cargo test` — **confirm the new test fails** for the right reason.
3. Implement the smallest change to make it pass. Don't touch unrelated code.
4. Run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all`.
5. **If the diff touches delete / move / trash / truncate / overwrite logic, run the `deletion-safety-reviewer` subagent and resolve every finding before continuing.**
6. Run the full suite via the `verifier` subagent.

**STOP and ask the user** if any of these is true:
- A change would weaken or remove a safety-invariant test.
- Clippy cannot pass without `#[allow(...)]` on a correctness lint.
- A real (non-fixture) filesystem path appears anywhere in a test.
- You'd need to bypass the `no-real-deletes.sh` hook.

Slash commands wrap this: `/loop`, `/safety-check`, `/new-cleaner`, `/scan-fixture`.

---

## SAFETY CONTRACT (non-negotiable — this is a data-destroying tool)

Every code path that deletes, trashes, moves, truncates, or overwrites a file MUST satisfy all of:

1. **Dry-run is the default.** Destructive action requires an explicit flag/confirmation. Default behavior previews only.
2. **Protected-path denylist, checked first.** Refuse anything under: `/`, `/System`, `/usr`, `/bin`, `/sbin`, `/Library`, `/Applications`, `~/Library/Keychains`, `~/Library/Mail`, the home root itself, and anything inside a `.git`. Canonicalize paths and re-check after resolving symlinks (no `..` escape, no TOCTOU).
3. **Scoped allowlist.** Cleanup is confined to known-safe locations (caches, logs, trashes, build artifacts) — never arbitrary user paths without confirmation.
4. **Trash, not unlink.** Prefer moving to the system Trash (recoverable). Irreversible deletion only behind explicit per-action consent.
5. **No unconfirmed mass delete.** Recursive removals show count + total size and require confirmation; reject suspiciously broad globs.
6. **Append-only audit log** of every planned (dry-run) and executed action, with absolute paths and sizes.
7. **Tests run against a throwaway temp dir — never the real filesystem.**

Before presenting any diff that adds or changes deletion logic, run the `deletion-safety-reviewer` subagent on it.

### How the contract maps to code

| Contract item | Enforced in |
|---------------|-------------|
| 1 Dry-run default | `crates/core/src/executor.rs` (`Consent::default()` previews) |
| 2 Denylist first + canonicalize/re-check | `crates/safety/src/denylist.rs`, `path_guard.rs`; re-guarded in `executor.rs` |
| 3 Scoped allowlist | `crates/safety/src/allowlist.rs` |
| 4 Trash not unlink | `executor.rs` (`Sink` trait; Trash default, `--permanent` gated) |
| 5 No unconfirmed mass delete | `crates/core/src/plan.rs` thresholds + `executor.rs`; `safety/src/dir_guard.rs` supplies the real recursive count/size for a directory |
| 6 Append-only audit log | `crates/core/src/audit.rs` (JSONL, append+flush; errors abort the run) |
| 7 Temp-dir-only tests | `crates/core/tests/integration.rs`, in-module tests |

**The one chokepoint:** every destructive op must go through `safety::guard()`,
which is the *only* constructor of `SafePath`. If you find code deleting a raw
`&Path`, that's a bug — route it through the guard.

---

## Architecture

Rust workspace (chosen for memory safety on a destructive tool, and because
`cargo test`/`clippy` give a fast, strict verification oracle — the engine of
the autonomous loop). UI shell is deferred until the substrate is complete.

```
crates/
  safety/   Trust kernel — denylist, path_guard (canonicalize + SafePath),
            dir_guard (bounded fail-closed tree walk + SafeDir), allowlist.
            Pure: never deletes. Everything destructive must pass through it.
  core/     Engine — scanner (read-only) → plan (dry-run data) → executor
            (consent-gated, the ONLY mutator) → audit (append-only JSONL).
  cli/      `macclean` front-end. `scan` previews; `clean` previews unless
            --execute; --permanent and --yes gate the dangerous paths.
```

Data flow: `scanner::scan` → `Plan` (pure) → `executor::execute(plan, Consent, …)`.
Default `Consent` is a dry run that mutates nothing.

---

## Workflow notes / gotchas

- This repo does **not** use `acceptEdits` (`.claude/settings.json` → `defaultMode: default`): deletion code must be reviewed, not auto-applied.
- A project hook (`.claude/hooks/no-real-deletes.sh`) blocks the agent from executing `rm`/`trash`/`find -delete` against real paths during development. Exercise deletion only against fixtures (`$TMPDIR`, `/var/folders`, paths containing `fixture`).
- **macOS canonicalization quirk:** `/Users/...` and `/var/folders/...` are symlinks. Always compare canonical paths, and canonicalize the home dir (`safety::canonical_home`) before passing it to `guard`. Tests must `fs::canonicalize` their tempdir home.
- `WalkDir` runs with `follow_links(false)`; symlinks are dropped at the `is_file()` check, and `guard` + allowlist are the backstop if anything slips through.
- Audit-log writes are **fatal on failure** — we refuse to delete if we can't record it. Don't downgrade that to a silent `let _ =`.

## Subagents (in `.claude/agents/`)

- `deletion-safety-reviewer` — adversarial review of any deletion/move diff against the 7-point contract. **Required** before presenting such a diff.
- `verifier` — runs the real `cargo test` + `clippy` + `fmt` suite and reports the actual result. Use before claiming done.
- `safety-architect` — design/plan a new cleaner or refactor so it fits the safety substrate before code is written.
- `ux-critic` — vision reviewer for GUI screenshots; scores against `design/rubric.md` + `design/references/` and pushes for distinctiveness, not just correctness. Used by the prettification loop.

## Autonomous loop conventions (lessons learned — don't relearn these)

The project is developed by self-continuing agent loops (see `.claude/loops/`).
Reusable prompts: `autonomous-dev-loop.md` and `prettify-loop.md` — paste either
into a fresh session. Hard-won conventions baked into them:

- **Confirm CI by conclusion, not the watcher.** `gh run watch --exit-status` can
  return before the run finishes — always verify with `gh run view <id> --json
  status,conclusion` (status=completed AND conclusion=success) before merging.
- **A job that fails in ~2 seconds having run ZERO steps is a billing stop, not a
  broken build.** When the Actions allowance is exhausted GitHub simply refuses
  to start jobs, and the result is indistinguishable from a real failure at a
  glance: red X, `conclusion: failure`, no log. Check
  `gh api repos/{owner}/{repo}/actions/runs/<id>/jobs` — if `steps` is empty and
  `completed_at - started_at` is a couple of seconds, the code is fine and the
  month is not. Do **not** start debugging the build.
- **CI is the second opinion; `./scripts/verify.sh` is the first.** Merge on a
  confirmed-green local run, and say in the PR that verification was local when
  CI did not run. Never write "CI green" for a run that never started.
- **The workflow is shaped around the 10x macOS multiplier** (see the header
  comment in `.github/workflows/ci.yml`): prose and design assets are
  `paths-ignore`d entirely, superseded PR runs are cancelled, and
  `release-build`/`package` are main-and-tags only because they produce
  artifacts rather than signal. Before adding a job or widening a trigger, work
  out what it costs — a macOS minute is ten.
- **Sync local main after every merge.** `gh pr merge` can drop you on a stale
  local `main`; start each iteration with `git checkout -B main origin/main -q`.
- **Verify the branch before committing and the push after.** A blocked `checkout`
  can leave you on the wrong branch; `git rev-parse --abbrev-ref HEAD` first, and
  if `git push` says "src refspec does not match", recreate the branch from
  origin/main and cherry-pick.
- **The deletion-guard hook is a deliberately FAIL-CLOSED substring guard** — it
  blocks the destructive verbs as whole words *anywhere* in a Bash command. This
  over-blocks benign commands that merely contain the words (branch names like
  `feat/empty-X`, commit messages, PR bodies, jq filters), which is annoying but
  safe. (A "command-position only" relaxation was reverted — the
  `deletion-safety-reviewer` found it failed OPEN: env-var prefixes, subshells,
  `$(...)`, and stray sandbox tokens all bypassed it.) **Convention, not a looser
  guard:** in Bash commands avoid the bare verbs — write file content containing
  them with the Write/Edit tools (not bash heredocs), and phrase commit messages /
  branch names without them (e.g. "startup"/"login-items"/"dispose").
- **Visual tasks pause for the human.** Anything a user SEES is built + critiqued
  by the UX oracle, then opened as a PR with screenshots and a `needs input:` —
  never auto-merged. Backend/harness/packaging tasks auto-merge on confirmed green.
- **A commit that introduces a component AND regenerates its baseline makes the
  visual gate useless for that component.** There is nothing to diff against, so
  a green snapshot only says the render matches itself. Found the hard way: a
  new chart's colour key rendered at 0×0 — every swatch was a sized `<span>`
  inside a wrapper, so it was an inline non-replaced box and ignored width and
  height — and it passed `tsc`, the build, axe and the snapshot gate. Markup
  review missed it too, because the markup was correct in every respect except
  that one. Only measuring pixels caught it. So for a *new* component, treat the
  gate as recording the render rather than checking it, and have `ux-critic`
  measure rather than read.
- **The UX oracle is how "pleasant" is made verifiable:** `cd crates/gui && npm
  run ux` renders each screen headlessly → PNGs (`ux/screenshots/`) + axe a11y +
  visual-regression. Critique the PNGs with `ux-critic` (or the Read tool — you
  can see images) against `design/rubric.md`. It has caught real WCAG bugs.
