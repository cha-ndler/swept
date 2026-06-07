#!/usr/bin/env bash
# PreToolUse guard for a filesystem-destroying tool under development.
# Blocks the agent from running real deletions against anything that is not
# clearly a throwaway fixture/temp path. Exit 2 + stderr = block.
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
  echo "GUARD: refusing a real filesystem deletion during development. mac-cleaner is a data-destroying tool — exercise deletion logic only against a throwaway fixture/temp dir (\$TMPDIR, /var/folders, a path containing 'fixture'). Run it yourself if you truly mean it." >&2
  exit 2
fi
exit 0
