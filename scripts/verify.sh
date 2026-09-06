#!/usr/bin/env bash
#
# The full gate, locally — the same checks CI runs, in the same order.
#
# This exists because CI is not always available to be the oracle. macOS runners
# bill at 10x on a private repository, and when the monthly allowance runs out
# jobs stop starting: they fail in about two seconds having executed zero steps,
# which looks exactly like a broken build and is not one. A change is "done"
# when this script passes; CI is the second opinion.
#
# Usage:
#   scripts/verify.sh              # everything
#   scripts/verify.sh --rust       # workspace only (the fast loop)
#   scripts/verify.sh --gui        # frontend + UX oracle + Tauri shell only
#   scripts/verify.sh --bundle     # everything, plus the real .app/.dmg bundle
#
# `--bundle` exists because CI no longer packages on every push to main, only on
# a v* tag. The ordinary Tauri step here is a *debug* `cargo build`, which
# compiles the shell but never exercises release codegen, the bundler, or the
# `.dmg` step — so nothing between merges would catch a packaging regression.
# Run this before cutting a release. It takes minutes, not seconds, which is
# exactly why it is not in the default gate.
#
# Exit code is 0 only if every step that ran passed. Anything skipped is named
# in the summary rather than quietly counted as a pass.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WANT_RUST=1
WANT_GUI=1
WANT_BUNDLE=0
case "${1:-}" in
  --rust)   WANT_GUI=0 ;;
  --gui)    WANT_RUST=0 ;;
  --bundle) WANT_BUNDLE=1 ;;
  "")       ;;
  *) echo "usage: $0 [--rust|--gui|--bundle]" >&2; exit 2 ;;
esac

PASS=()
FAIL=()
SKIP=()

step() {
  local name="$1"; shift
  printf '\n\033[1m▶ %s\033[0m\n' "$name"
  if "$@"; then
    PASS+=("$name")
  else
    FAIL+=("$name")
    printf '\033[31m✗ %s\033[0m\n' "$name"
  fi
}

skip() {
  SKIP+=("$1 — $2")
  printf '\n\033[33m⤼ skipped %s — %s\033[0m\n' "$1" "$2"
}

# Run a command from the GUI crate, which is excluded from the Rust workspace
# and therefore has to be driven from its own directory.
in_gui() { ( cd "$ROOT/crates/gui" && "$@" ); }

# Can the compiler that will actually run build for this target?
#
# Deliberately not `rustup target list --installed`. On a machine with Homebrew's
# rust ahead of rustup's shims on PATH — which is not exotic — rustup happily
# reports both macOS targets installed for a toolchain that `cargo` never uses,
# and the universal build then fails a few minutes in with "can't find crate for
# `core`". Asking rustc where the target's libdir is asks the right compiler, and
# is the same question the build will ask.
MISSING_TARGET=""
have_target() {
  local t="$1" libdir
  libdir=$(rustc --print target-libdir --target "$t" 2>/dev/null) || libdir=""
  if [ -n "$libdir" ] && [ -d "$libdir" ]; then
    return 0
  fi
  MISSING_TARGET="$(command -v rustc) cannot build $t — 'rustup target add $t', and check that rustup's shims come before any other rust on PATH"
  return 1
}

# The version lives in `[workspace.package]` for the four workspace crates, but
# the Tauri shell is outside the workspace and cannot inherit it — so three files
# carry it by hand. Nothing made them agree until this, and a `.dmg` named for a
# version the binary inside does not report is the kind of mismatch nobody
# notices until a user does.
versions_agree() {
  local ws tauri conf pkg lock
  ws=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
  tauri=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/crates/gui/src-tauri/Cargo.toml" | head -1)
  conf=$(sed -n 's/.*"version": "\(.*\)".*/\1/p' "$ROOT/crates/gui/src-tauri/tauri.conf.json" | head -1)
  pkg=$(sed -n 's/.*"version": "\(.*\)".*/\1/p' "$ROOT/crates/gui/package.json" | head -1)
  # `npm ci` refuses a lockfile that disagrees with its package.json, so this
  # one is not cosmetic: forgetting it turns a version bump into a red CI job
  # on a file nobody thinks of as carrying a version.
  lock=$(sed -n 's/.*"version": "\(.*\)".*/\1/p' "$ROOT/crates/gui/package-lock.json" | head -1)

  if [ -z "$ws" ]; then
    echo "no version in [workspace.package] — the single source is gone" >&2
    return 1
  fi
  local bad=0
  for pair in "src-tauri/Cargo.toml:$tauri" "tauri.conf.json:$conf" "package.json:$pkg" \
              "package-lock.json:$lock"; do
    if [ "${pair#*:}" != "$ws" ]; then
      echo "version mismatch: ${pair%%:*} is ${pair#*:}, workspace is $ws" >&2
      bad=1
    fi
  done
  [ "$bad" = 0 ] && echo "OK: every version reads $ws."
  return "$bad"
}

# The release notes are the CHANGELOG section for the tag, extracted by
# `scripts/release-notes.sh` and handed to the GitHub Release — so a version
# with no section produces a release with no notes. That failure used to happen
# at tag time, after the tag was already pushed; this moves it to the local gate
# where it costs nothing to fix.
changelog_has_version() {
  local ws
  ws=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
  if ! "$ROOT/scripts/release-notes.sh" "$ws" >/dev/null; then
    return 1
  fi
  echo "OK: CHANGELOG.md has notes for $ws ($("$ROOT/scripts/release-notes.sh" "$ws" | wc -l | tr -d ' ') lines)."
}

# The safety guard CI enforces, kept here so it fails before a push rather than
# after one. SAFETY CONTRACT item 7: integration tests build their own tempfile
# fixture and never resolve the real $HOME. A match is the failure, so the exit
# code is inverted.
no_real_home() {
  if rg -n 'dirs::home_dir|env::var\("HOME"\)|env::home_dir' \
        -g '**/tests/**/*.rs' crates; then
    echo "A test resolves the real home directory. Use a tempfile fixture instead."
    return 1
  fi
  echo "OK: no test resolves the real home directory."
}

# Swept states who publishes it in five places: the copyright line in LICENSE,
# and four documents and manifests that must agree with it. The identity a user
# sees in the About box, in the Terms they accept, and in the licence must be
# one identity — a build whose Info.plist credits one publisher and whose Terms
# name another is exactly the "boilerplate nobody meant" impression the whole
# assent layer exists to avoid.
#
# This replaces an earlier gate that hunted for unfilled entity placeholders.
# Those are gone: Swept names a real publisher everywhere, and ships under an
# individual's name on purpose (docs/LEGAL.md, "Shipping as an individual"). The failure mode that
# survives is *drift* — changing LICENSE when the publisher changes and missing
# one of the other four — so that is what is checked now, and it is checked on
# every run rather than only before a release.
publisher_is_consistent() {
  who=$(sed -n 's/^Copyright (c) [0-9]* \(.*\)$/\1/p' "$ROOT/LICENSE" | head -1)
  if [ -z "$who" ]; then
    echo "no 'Copyright (c) <year> <name>' line in LICENSE to check against" >&2
    return 1
  fi
  bad=0
  for f in NOTICE.md PRIVACY.md \
           crates/gui/src-tauri/Info.plist \
           crates/gui/src-tauri/tauri.conf.json; do
    if ! grep -qF "$who" "$ROOT/$f"; then
      echo "publisher mismatch: LICENSE says '$who', $f does not name it" >&2
      bad=1
    fi
  done
  [ "$bad" = 0 ] && echo "OK: every document names the publisher as '$who'."
  return "$bad"
}

# `acceptance::TERMS_VERSION` is what the app records the user as having
# accepted, and TERMS.md is what it shows them. If those two disagree the record
# names a document that was never presented, which makes the record worthless.
# Same reasoning as `versions_agree`, and the same fix: assert it.
terms_version_agrees() {
  doc=$(sed -n 's/^\*\*Version \([0-9][^.]*\.[0-9]*\)\.\*\*.*/\1/p' "$ROOT/TERMS.md" | head -1)
  code=$(sed -n 's/^pub const TERMS_VERSION: &str = "\(.*\)";$/\1/p' \
         "$ROOT/crates/gui-core/src/acceptance.rs" | head -1)
  if [ -z "$doc" ]; then
    echo "no '**Version X.Y.**' line in TERMS.md — the gate cannot read it" >&2
    return 1
  fi
  if [ "$doc" != "$code" ]; then
    echo "terms version mismatch: TERMS.md is $doc, acceptance.rs is $code" >&2
    return 1
  fi
  echo "OK: terms version reads $doc in both places."
}

if [ "$WANT_RUST" = 1 ]; then
  step "cargo fmt --all --check"     cargo fmt --all --check
  step "cargo clippy -D warnings"    cargo clippy --workspace --all-targets -- -D warnings
  step "cargo test --workspace"      cargo test --workspace
  step "no real \$HOME in tests"     no_real_home
  step "versions agree"            versions_agree
  step "changelog has the version" changelog_has_version
  step "terms version agrees"      terms_version_agrees
  step "publisher is consistent"   publisher_is_consistent
fi

if [ "$WANT_GUI" = 1 ]; then
  if [ ! -d "$ROOT/crates/gui/node_modules" ]; then
    skip "the whole GUI half" "crates/gui/node_modules is absent — run 'cd crates/gui && npm ci'"
  else
    step "tsc + vite build"          in_gui npm run build
    step "UX oracle (axe + visual)"  in_gui npm run ux
    step "tauri fmt"                 in_gui cargo fmt --manifest-path src-tauri/Cargo.toml --check
    step "tauri clippy -D warnings"  in_gui cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    step "tauri build"               in_gui cargo build --manifest-path src-tauri/Cargo.toml

    # The bundle, which the step above does not cover. Release codegen and the
    # `.app`/`.dmg` bundler are the two things CI stopped running on every
    # merge, so this is where that coverage went — deliberately opt-in, because
    # it is minutes rather than seconds.
    #
    # `--target universal-apple-darwin` because that is what a tag builds, and a
    # local gate that builds something else is not the local equivalent of
    # anything. It needs both toolchain targets installed; without them, say so
    # rather than quietly bundling for this machine's architecture only.
    if [ "$WANT_BUNDLE" = 1 ]; then
      if ! command -v cargo-tauri >/dev/null 2>&1; then
        skip "tauri bundle (.app + .dmg, universal)" \
             "cargo-tauri not installed — cargo install tauri-cli --version '^2' --locked"
      elif ! have_target aarch64-apple-darwin || ! have_target x86_64-apple-darwin; then
        skip "tauri bundle (.app + .dmg, universal)" \
             "$MISSING_TARGET"
      else
        step "tauri bundle (.app + .dmg, universal)" \
             in_gui cargo tauri build --target universal-apple-darwin
      fi
    fi
  fi
fi

printf '\n\033[1m── summary ───────────────────────────────\033[0m\n'
for s in ${PASS[@]+"${PASS[@]}"}; do printf '  \033[32m✓\033[0m %s\n' "$s"; done
for s in ${SKIP[@]+"${SKIP[@]}"}; do printf '  \033[33m⤼\033[0m %s\n' "$s"; done
for s in ${FAIL[@]+"${FAIL[@]}"}; do printf '  \033[31m✗\033[0m %s\n' "$s"; done

if [ "${#FAIL[@]}" -gt 0 ]; then
  printf '\n\033[31m%d check(s) failed.\033[0m\n' "${#FAIL[@]}"
  exit 1
fi
if [ "${#SKIP[@]}" -gt 0 ]; then
  printf '\n\033[33mEverything that ran passed, but %d step(s) were skipped — say so when reporting.\033[0m\n' "${#SKIP[@]}"
  exit 0
fi
printf '\n\033[32mAll checks passed.\033[0m\n'
