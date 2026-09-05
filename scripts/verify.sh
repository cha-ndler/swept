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

if [ "$WANT_RUST" = 1 ]; then
  step "cargo fmt --all --check"     cargo fmt --all --check
  step "cargo clippy -D warnings"    cargo clippy --workspace --all-targets -- -D warnings
  step "cargo test --workspace"      cargo test --workspace
  step "no real \$HOME in tests"     no_real_home
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
    if [ "$WANT_BUNDLE" = 1 ]; then
      if command -v cargo-tauri >/dev/null 2>&1; then
        step "tauri bundle (.app + .dmg)" in_gui cargo tauri build
      else
        skip "tauri bundle (.app + .dmg)" \
             "cargo-tauri not installed — cargo install tauri-cli --version '^2' --locked"
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
