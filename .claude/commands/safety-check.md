---
description: Run the full safety gate over the current diff (tests, clippy, fmt, deletion-safety review).
---

Run mac-cleaner's complete safety gate on the current working changes and report
a single go/no-go.

1. `git diff --stat` and `git diff` to see what changed.
2. Invoke the `verifier` subagent (fmt + clippy -D warnings + tests + the
   real-path test grep).
3. If the diff touches any file under `crates/safety/` or `crates/core/src/executor.rs`,
   or any delete/move/trash/truncate/overwrite logic anywhere, invoke the
   `deletion-safety-reviewer` subagent on the diff.
4. Summarize: list each gate and its result. Conclude with **GO** only if the
   verifier passed and (where applicable) the reviewer returned `VERDICT: PASS`;
   otherwise **NO-GO** with the blocking findings.

Do not modify code in this command — it only reports. If something fails, hand
back the specific findings.
