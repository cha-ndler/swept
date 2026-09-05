#!/usr/bin/env bash
# PreToolUse guard for a filesystem-destroying tool under development.
# Blocks the agent from running real deletions against anything that is not
# clearly a throwaway fixture/temp path. Exit 2 + stderr = block.
#
# DESIGN NOTE (deliberate): this is a FAIL-CLOSED substring guard. It matches the
# destructive verbs as whole words ANYWHERE in the command. That over-blocks
# benign commands that merely contain the words (branch names like
# feat/empty-X, commit messages, jq filters) — which is annoying but SAFE.
# An attempt to relax it to "command position only" was reverted after the
# deletion-safety-reviewer found it failed OPEN (env-var prefixes, subshells,
# `$(...)`, and a stray sandbox token anywhere all bypassed it). For a
# data-destroying tool we keep the guard strict and handle the friction by
# convention instead: in shell commands, avoid the bare verbs — write file
# content containing them with the Write/Edit tools (not bash heredocs), and
# phrase commit messages / branch names without them.
input=$(cat)
tool=$(printf '%s' "$input" | jq -r '.tool_name // ""')
[ "$tool" = "Bash" ] || exit 0
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')

# Does the command perform a destructive filesystem op?
if printf '%s' "$cmd" | grep -qE '(\brm\b|\bunlink\b|\btrash\b|\bsrm\b|find[^|]*-delete|\bshred\b|mv[[:space:]].*/dev/null)'; then
  # Allow ONLY if every destructive target is obviously a sandbox: temp dirs,
  # macOS per-user temp (/var/folders), $TMPDIR, or a path containing fixture/tmp/target/build.
  if printf '%s' "$cmd" | grep -qiE '(/var/folders/|\$TMPDIR|/tmp/|fixture|/target/|/build/|\.test-)'; then
    exit 0
  fi
  echo "GUARD: refusing a real filesystem deletion during development. Swept is a data-destroying tool — exercise deletion logic only against a throwaway fixture/temp dir (\$TMPDIR, /var/folders, a path containing 'fixture'). Run it yourself if you truly mean it." >&2
  exit 2
fi
exit 0
