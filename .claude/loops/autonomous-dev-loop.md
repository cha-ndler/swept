# Autonomous dev loop — paste into a fresh Claude Code session

Paste the block below as your message to a new agent session working in this
repo. It runs ONE backlog task per turn, then re-arms itself. Lessons from prior
sessions are baked in (CI-confirm, main-sync, branch/push verify, command-position
hook). Keep it verbatim so each wake-up repeats the loop.

---

AUTONOMOUS Swept DEV LOOP — one task per iteration, then re-arm.

Working dir: the repo root (use a git worktree if asked). Read CLAUDE.md first.

1. `git fetch origin -q --prune`; sync local main: `git checkout -B main origin/main -q`. Read ROADMAP.md; pick the FIRST unchecked `- [ ]` in the active milestone. If none, go to step 8.
2. `git checkout -b feat/<slug> origin/main`. Confirm with `git rev-parse --abbrev-ref HEAD` BEFORE committing. NOTE: the deletion-guard hook is fail-closed and blocks the destructive verbs as substrings *anywhere* in a Bash command — so avoid the bare words in branch names, commit messages, PR bodies, and shell one-liners (write file content containing them via the Write/Edit tools, not bash heredocs). Never run a real removal against a non-fixture path.
3. Implement TEST-FIRST: add tests against tempfile tempdir fixtures (never a real path), `cargo test --workspace` to confirm RED, then implement to GREEN. GUI business logic lives in `crates/gui-core` (plain Rust, tested); Tauri `#[tauri::command]` fns just delegate. ALL deletion must route through `swept-core`'s consent-gated `executor` — never reimplement deletion.
4. `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`. GUI crate (excluded from workspace): `cargo fmt/clippy --manifest-path crates/gui/src-tauri/Cargo.toml`; frontend `npm ci && npm run build` in `crates/gui`.
5. Run the `deletion-safety-reviewer` subagent if the diff touched any delete/move/trash/truncate/overwrite logic OR `crates/safety/` OR a new caller of the executor; resolve to VERDICT: PASS. Run the `verifier` subagent before claiming done (skip only for pure docs/CI/frontend-only diffs — say so).
6. Check the item off in ROADMAP.md. Commit (git config user.email = 48898494+cha-ndler@users.noreply.github.com, name cha-ndler). Push `-u origin <branch>` and VERIFY (if "src refspec does not match", recreate branch from origin/main and cherry-pick). Open a PR. Wait for CI, then CONFIRM via `gh run view <id> --json status,conclusion,jobs` that status=completed AND conclusion=success (do NOT trust `gh run watch` exit code alone — it returns early).
   - Backend / harness / packaging tasks: on confirmed green, `gh pr merge <n> --squash --delete-branch`, then step 7.
   - VISUAL tasks (anything users SEE): do NOT auto-merge. Attach screenshots + ux-critic scorecard to the PR, write `needs input:` for the human taste gate, and STOP.
7. After a successful auto-merge: `git fetch origin -q --prune`, then re-arm: call ScheduleWakeup with this exact prompt (delaySeconds ~180). One task per iteration.
8. FINISH (milestone complete): update README/CHANGELOG, tag the version, push the tag, post a final summary, do NOT reschedule.

Constraints: never bypass the no-real-deletes hook, `git reset --hard`, force-push, or change repo-wide GitHub settings. Keep PRs atomic. State results in your own text (a classifier reads only your message). If genuinely blocked needing the user, write `needs input:` and stop.

Begin iteration now.
