#!/usr/bin/env bash
# Parse the app's entitlements the way signing will: by signing something with
# them.
#
#   scripts/check-entitlements.sh [path/to/entitlements.plist]
#
# `plutil -lint` is not enough, and this exists because it said "OK" to a file
# that could not be signed. The file's explanatory comment used `--` as a dash;
# XML forbids `--` inside a comment, plutil tolerates it, and the kernel's
# parser (AMFI) does not:
#
#   Failed to parse entitlements: AMFIUnserializeXML: syntax error near line 9
#
# That broke every signed build — ad-hoc and Developer ID alike — and nothing
# ran the signer between releases to notice. Signing a copy of /usr/bin/true
# takes milliseconds and exercises the same parser.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
ent="${1:-$root/crates/gui/src-tauri/entitlements.plist}"

probe="$(mktemp "${TMPDIR:-/tmp}/swept-entitlements-probe.XXXXXX")"
trap 'rm -f "$probe"' EXIT
cp /usr/bin/true "$probe"

codesign --force --sign - --options runtime --entitlements "$ent" "$probe"
echo "OK: $ent parses as entitlements."
