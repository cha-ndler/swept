---
description: Design and implement a new junk-cleaner category, safety-first.
argument-hint: <category, e.g. "Homebrew download cache">
---

Add a new cleaner category: **$ARGUMENTS**

1. **Plan first.** Invoke the `safety-architect` subagent with this category. Get
   back the scope, safety analysis, file-by-file changes, and the test-first
   plan. Do not write code until you have it.
2. **Confirm the allowlist impact.** A new category should add an allowlist root
   or a within-root recognizer in `crates/safety/src/allowlist.rs` — never a new
   delete path. If the plan introduces a new destructive capability, surface that
   to the user before proceeding.
3. **Implement test-first**, following `/loop` for each change: write the
   tempdir-fixture tests (including the negative "nearby precious file" case),
   watch them fail, implement, make them pass.
4. **Review.** Because this touches the safety substrate, invoke
   `deletion-safety-reviewer` and resolve to `VERDICT: PASS`.
5. **Verify.** Invoke `verifier`. Then show a sample `cargo run -p macclean -- scan`
   preview against a fixture HOME proving the new category is detected and
   nothing outside the allowlist is.

Report the plan, the diff summary, and the verifier verdict.
