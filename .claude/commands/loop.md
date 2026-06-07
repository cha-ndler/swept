---
description: Run one TDD loop iteration on the given task (test-first, clippy-clean, safety-reviewed).
argument-hint: <what to build or fix>
---

Work on: **$ARGUMENTS**

Follow the mac-cleaner development loop exactly. Do not skip steps.

1. **Test first.** Add or extend a test in the appropriate crate, against a
   `tempfile::tempdir()` fixture (canonicalize the fake home). Include at least
   one negative case (something nearby that must NOT be affected).
2. Run `cargo test --workspace` and confirm the new test **fails for the right
   reason**. Show the failure.
3. Implement the smallest change to make it pass. Match surrounding style. Don't
   refactor unrelated code.
4. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`.
   Fix every warning (no `#[allow]` on correctness lints).
5. **If the diff touched delete/move/trash/truncate/overwrite logic**, invoke the
   `deletion-safety-reviewer` subagent on it and resolve every finding. Re-run it
   until `VERDICT: PASS`.
6. Invoke the `verifier` subagent for the final check.

STOP and ask the user if a safety-invariant test would change, clippy can't pass
cleanly, a real path appears in a test, or you'd need to bypass the
`no-real-deletes.sh` hook.

Report: what you tested, the red→green transition, and the final verifier result.
