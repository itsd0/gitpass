#!/usr/bin/env bash
# Evaluates applicability-aware status check aggregation for ci-ok.
#
# Rules:
#  1. Change detection MUST succeed.
#  2. Applicable jobs (needed=true) MUST succeed. Unexpected skips, failures,
#     or cancellations cause immediate failure.
#  3. Non-applicable jobs (needed=false) MUST be skipped. Unexpected execution
#     causes immediate failure.
set -euo pipefail

CHANGES_RESULT="${CHANGES_RESULT:-<unset>}"
RUST_NEEDED="${RUST_NEEDED:-false}"
RUST_TEST_RESULT="${RUST_TEST_RESULT:-skipped}"
RUST_CLIPPY_RESULT="${RUST_CLIPPY_RESULT:-skipped}"
FRONTEND_NEEDED="${FRONTEND_NEEDED:-false}"
FRONTEND_RESULT="${FRONTEND_RESULT:-skipped}"
DESKTOP_NEEDED="${DESKTOP_NEEDED:-false}"
TAURI_MACOS_RESULT="${TAURI_MACOS_RESULT:-skipped}"
SUPPLY_CHAIN_NEEDED="${SUPPLY_CHAIN_NEEDED:-false}"
SUPPLY_CHAIN_RESULT="${SUPPLY_CHAIN_RESULT:-skipped}"
ANDROID_NEEDED="${ANDROID_NEEDED:-false}"
ANDROID_RESULT="${ANDROID_RESULT:-skipped}"

echo "=== ci-ok: Gate Evaluation ==="
echo "changes:      $CHANGES_RESULT"
echo "rust-test:    $RUST_TEST_RESULT (needed: $RUST_NEEDED)"
echo "rust-clippy:  $RUST_CLIPPY_RESULT (needed: $RUST_NEEDED)"
echo "frontend:     $FRONTEND_RESULT (needed: $FRONTEND_NEEDED)"
echo "tauri-macos:  $TAURI_MACOS_RESULT (needed: $DESKTOP_NEEDED)"
echo "supply-chain: $SUPPLY_CHAIN_RESULT (needed: $SUPPLY_CHAIN_NEEDED)"
echo "android:      $ANDROID_RESULT (needed: $ANDROID_NEEDED)"

if [ "$CHANGES_RESULT" != "success" ]; then
  echo "::error::Change detection failed or was cancelled ($CHANGES_RESULT)." >&2
  exit 1
fi

check_job() {
  local name="$1"
  local actual="$2"
  local needed="$3"

  if [ "$needed" = "true" ]; then
    if [ "$actual" != "success" ]; then
      echo "::error::Job '$name' was required to run (applicable), but finished with '$actual' (expected 'success')!" >&2
      return 1
    fi
  else
    if [ "$actual" != "skipped" ]; then
      echo "::error::Job '$name' was not applicable, but finished with unexpected status '$actual' (expected 'skipped')!" >&2
      return 1
    fi
  fi
  return 0
}

failures=0
check_job "rust-test"    "$RUST_TEST_RESULT"    "$RUST_NEEDED"         || failures=$((failures + 1))
check_job "rust-clippy"  "$RUST_CLIPPY_RESULT"  "$RUST_NEEDED"         || failures=$((failures + 1))
check_job "frontend"     "$FRONTEND_RESULT"     "$FRONTEND_NEEDED"     || failures=$((failures + 1))
check_job "tauri-macos"  "$TAURI_MACOS_RESULT"  "$DESKTOP_NEEDED"      || failures=$((failures + 1))
check_job "supply-chain" "$SUPPLY_CHAIN_RESULT" "$SUPPLY_CHAIN_NEEDED" || failures=$((failures + 1))
check_job "android"      "$ANDROID_RESULT"      "$ANDROID_NEEDED"      || failures=$((failures + 1))

if [ "$failures" -gt 0 ]; then
  echo "::error::ci-ok failed: $failures job status mismatch(es) detected." >&2
  exit 1
fi

echo "ci-ok: PASS. All applicable jobs succeeded and non-applicable jobs were cleanly skipped."
