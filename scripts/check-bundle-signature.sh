#!/usr/bin/env bash
# Assert that a built Swept.app carries a valid signature over the whole
# bundle — ad-hoc or Developer ID — rather than only the linker's signature on
# the executable inside it.
#
#   scripts/check-bundle-signature.sh path/to/Swept.app
#
# Why this exists: v0.5.0 shipped a bundle that was "not signed at all". The
# linker ad-hoc signs every arm64 Mach-O on its own, so the executable looked
# signed, but nothing sealed Info.plist or the resources. `syspolicy_check`
# rates that Fatal, and a quarantined download in that state is the "Swept is
# damaged and can't be opened" dialog — which has no Open Anyway, so the
# README's walkthrough cannot work. An ad-hoc signature over the bundle
# (identity `-`, no Apple account) turns it into the ordinary unsigned-app
# warning the walkthrough is written for.
#
# `cargo tauri build` reports success either way, which is why this is a
# separate check rather than something to read off the build log.
set -euo pipefail

app="${1:?usage: $0 path/to/Swept.app}"
[ -d "$app" ] || { echo "not a bundle: $app" >&2; exit 1; }

# Deep + strict: every nested binary, and the resource seal.
codesign --verify --deep --strict --verbose=2 "$app"

info="$(codesign --display --verbose=2 "$app" 2>&1)"
echo "$info"

# The two lines that distinguish a sealed bundle from a linker-signed binary.
if grep -q '^Info.plist=not bound' <<<"$info"; then
  echo "FAIL: Info.plist is not bound — the bundle itself is unsigned." >&2
  exit 1
fi
if grep -q '^Sealed Resources=none' <<<"$info"; then
  echo "FAIL: no sealed resources — the bundle itself is unsigned." >&2
  exit 1
fi
echo "OK: bundle signature is valid and covers Info.plist and resources."
