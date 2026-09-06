#!/usr/bin/env bash
# Print the CHANGELOG section for one version, for use as GitHub Release notes.
#
#   ./scripts/release-notes.sh v0.4.0
#   ./scripts/release-notes.sh 0.4.0
#
# The point is that the release page and CHANGELOG.md cannot drift: there is one
# text, and the release copies it. It **fails** rather than printing nothing —
# a release whose notes are silently empty is worse than a job that stops and
# says the section is missing, because the first one ships.
set -euo pipefail

VERSION="${1:?usage: release-notes.sh <version>}"
VERSION="${VERSION#v}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="$ROOT/CHANGELOG.md"

# Everything between this version's heading and the next `## ` heading. The
# headings are `## [0.4.0] — 2026-09-06`, so the version is matched inside
# brackets and the date is not part of the query.
notes=$(awk -v want="$VERSION" '
  /^## / {
    # A heading ends the section we were printing, and may start a new one.
    inside = ($0 ~ "^## \\[" want "\\]")
    next
  }
  # The link-reference block at the foot of the file follows no heading, so
  # without this the oldest section swallows it.
  /^\[[^]]+\]: / { inside = 0; next }
  inside { print }
' "$CHANGELOG")

# Strip the blank lines the heading match leaves at either end.
notes=$(printf '%s\n' "$notes" | sed -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;};/\n$/ba')

if [ -z "$notes" ]; then
  echo "no '## [$VERSION]' section in CHANGELOG.md — write the notes before tagging" >&2
  exit 1
fi

printf '%s\n' "$notes"
