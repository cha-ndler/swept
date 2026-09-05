# Contributing

Thanks for looking. Before anything else: **this is a tool that removes files**,
so the bar for a change that touches disposal is higher than the bar for most
projects, and deliberately so.

If you are reporting something that could destroy data or act without consent,
read [`SECURITY.md`](SECURITY.md) first — that goes through private reporting,
not a public issue.

## The one rule everything else follows from

> **Widen what we can *see*. Never widen what we can *dispose of* — escalate
> per-path with explicit consent instead.**

There are two scopes and they are not interchangeable.
`allowlist::default_roots` is the **disposal** boundary; `discovery_roots` is
much wider and is **read-only**. Discovery yields plain `PathBuf`s and never a
`SafePath`, so nothing a read-only walk finds can reach the executor without
passing `guard` first. A change that blurs those two will not be merged.

The full rules are the SAFETY CONTRACT in [`CLAUDE.md`](CLAUDE.md). They are
non-negotiable, and several of them exist because something went wrong once.

## Getting set up

```bash
cargo test --workspace
cd crates/gui && npm ci && npm run build
```

Then the whole gate, which is what "done" means here:

```bash
./scripts/verify.sh
```

Nine checks: fmt, clippy with `-D warnings`, the workspace tests, a guard that no
test resolves your real `$HOME`, the frontend build, the UX oracle (screenshots +
axe + visual regression), and the Tauri shell's own fmt/clippy/build. Add
`--bundle` to also build the real `.app` and `.dmg`; that is slow, and it is what
you want before cutting a release.

**CI is the second opinion, not the first.** Run `verify.sh` locally and say so
in the pull request.

## Writing a change that touches disposal

- **Test first, and confirm the test fails for the right reason.** "It went red
  because the field does not exist yet" is the evidence that the test is aimed at
  the thing you think it is.
- **Fixtures only.** Every test builds a `tempfile::tempdir()` and canonicalizes
  it — `/var/folders` is a symlink on macOS, and forgetting that has produced
  vacuous passes before. No test may name a real path, and `verify.sh` enforces
  it.
- **Pair every positive with a negative.** A test proving the guard refuses the
  bad thing is half a test; the other half proves the feature still works. Most
  of the real bugs found here were fixes that quietly broke the feature, or
  guards that refused nothing.
- **Prove the check bites.** Disable your new check and watch the test go red.
  A safety test that passes without the code it guards is worse than no test,
  because it reads like coverage.
- **Say what a figure means.** An overstated total is treated here as the same
  family of defect as a wrong deletion. If a number can be incomplete, the type
  should make that impossible to render without saying so.

## Pull requests

Explain **why**, not what — the diff already says what. This repository's commit
messages and PR descriptions are long on purpose: they are where the reasoning
lives, including the arguments that were lost and the things that turned out to
be wrong. If a review changed your mind, say so; that is the most useful part.

Anything you can *see* goes through screenshots and a human's eye before it
lands. Backend work merges on a green gate.

## What is likely to be turned down

- Widening `default_roots`, or carving an exception into the denylist.
- A "just delete it" fast path, a quick-clean from the menu bar, or anything that
  acts without a preview and a confirmation.
- Recursive removal without a count and a total in front of the user first.
- Marketing language. This app does not tell people their Mac is at risk, does
  not round figures up, and does not claim a scan was complete when it was not.
