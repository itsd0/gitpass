#!/usr/bin/env bash
# Test suite for CI change detection and ci-ok gate logic.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DETECT_SCRIPT="$SCRIPT_DIR/ci-detect-changes.sh"
GATE_SCRIPT="$SCRIPT_DIR/ci-ok-gate.sh"

pass_count=0
fail_count=0

assert_eq() {
  local test_name="$1"
  local actual="$2"
  local expected="$3"
  if [ "$actual" = "$expected" ]; then
    echo "  [PASS] $test_name"
    pass_count=$((pass_count + 1))
  else
    echo "  [FAIL] $test_name: expected '$expected', got '$actual'"
    fail_count=$((fail_count + 1))
  fi
}

echo "=== Running Path Classification Tests ==="

# Test 1: Android-only changes
out=$(echo "android/app/src/main/MainActivity.kt" | "$DETECT_SCRIPT")
assert_eq "Android-only: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Android-only: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "Android-only: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "false"
assert_eq "Android-only: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"
assert_eq "Android-only: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "false"
assert_eq "Android-only: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "false"

# Test 2: iOS-only changes (outside shared bridge)
out=$(echo "ios/Sources/ContentView.swift" | "$DETECT_SCRIPT")
assert_eq "iOS-only: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "iOS-only: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "false"
assert_eq "iOS-only: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "false"
assert_eq "iOS-only: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"
assert_eq "iOS-only: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "false"
assert_eq "iOS-only: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "false"

# Test 3: Shared mobile UniFFI bridge
out=$(echo "ios/rust/gitpass-ios-ffi/src/lib.rs" | "$DETECT_SCRIPT")
assert_eq "Shared FFI: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Shared FFI: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "Shared FFI: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "true"
assert_eq "Shared FFI: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "true"
assert_eq "Shared FFI: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"
assert_eq "Shared FFI: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "false"

# Test 4: Universal core crate
out=$(echo "crates/gitpass-core/src/crypto.rs" | "$DETECT_SCRIPT")
assert_eq "Core: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Core: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "true"
assert_eq "Core: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "true"
assert_eq "Core: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "Core: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "true"

# Test 5: Root Cargo manifests
out=$(printf "Cargo.lock\nCargo.toml" | "$DETECT_SCRIPT")
assert_eq "Cargo manifests: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "true"
assert_eq "Cargo manifests: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "true"
assert_eq "Cargo manifests: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "Cargo manifests: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "true"

# Test 6: Desktop UI changes
out=$(echo "apps/desktop/src/App.tsx" | "$DETECT_SCRIPT")
assert_eq "Desktop UI: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Desktop UI: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "true"
assert_eq "Desktop UI: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "true"
assert_eq "Desktop UI: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "false"
assert_eq "Desktop UI: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "false"

# Test 7: Extension-only changes
out=$(echo "extension/popup.js" | "$DETECT_SCRIPT")
assert_eq "Extension: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Extension: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "true"
assert_eq "Extension: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"
assert_eq "Extension: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "false"
assert_eq "Extension: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "false"

# Test 8: Docs-only changes
out=$(printf "README.md\ndocs/CLI.md\nLICENSE\nAGENTS.md" | "$DETECT_SCRIPT")
assert_eq "Docs-only: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "false"
assert_eq "Docs-only: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "false"
assert_eq "Docs-only: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"
assert_eq "Docs-only: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "false"
assert_eq "Docs-only: supply_chain" "$(echo "$out" | grep '^supply_chain=' | cut -d= -f2)" "false"
assert_eq "Docs-only: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "false"

# Test 9: Mixed changes (Android + Docs)
out=$(printf "android/app/build.gradle.kts\nREADME.md" | "$DETECT_SCRIPT")
assert_eq "Mixed Android+Docs: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Mixed Android+Docs: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "Mixed Android+Docs: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "false"

# Test 10: Workflow file edit (.github/workflows/ci.yml)
out=$(echo ".github/workflows/ci.yml" | "$DETECT_SCRIPT")
assert_eq "Workflow ci.yml: code" "$(echo "$out" | grep '^code=' | cut -d= -f2)" "true"
assert_eq "Workflow ci.yml: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "true"
assert_eq "Workflow ci.yml: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "true"
assert_eq "Workflow ci.yml: frontend" "$(echo "$out" | grep '^frontend=' | cut -d= -f2)" "true"
assert_eq "Workflow ci.yml: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"

# Test 11: merge_group / workflow_dispatch forced run
out=$(EVENT=merge_group "$DETECT_SCRIPT")
assert_eq "merge_group: rust" "$(echo "$out" | grep '^rust=' | cut -d= -f2)" "true"
assert_eq "merge_group: android" "$(echo "$out" | grep '^android=' | cut -d= -f2)" "true"
assert_eq "merge_group: desktop" "$(echo "$out" | grep '^desktop=' | cut -d= -f2)" "true"

echo "=== Running ci-ok Gate Truth-Table Tests ==="

# Gate Test 1: Full success when all applicable jobs succeed
res=0
CHANGES_RESULT=success \
RUST_NEEDED=true RUST_TEST_RESULT=success RUST_CLIPPY_RESULT=success \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=true SUPPLY_CHAIN_RESULT=success \
ANDROID_NEEDED=true ANDROID_RESULT=success \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: applicable jobs succeed, others skipped" "$res" "0"

# Gate Test 2: Docs-only (all jobs skipped, changes succeeded)
res=0
CHANGES_RESULT=success \
RUST_NEEDED=false RUST_TEST_RESULT=skipped RUST_CLIPPY_RESULT=skipped \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=false ANDROID_RESULT=skipped \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: docs-only all skipped succeeds" "$res" "0"

# Gate Test 3: Applicable job failure
res=0
CHANGES_RESULT=success \
RUST_NEEDED=false RUST_TEST_RESULT=skipped RUST_CLIPPY_RESULT=skipped \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=true ANDROID_RESULT=failure \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: applicable job failure fails gate" "$res" "1"

# Gate Test 4: Applicable job cancelled
res=0
CHANGES_RESULT=success \
RUST_NEEDED=true RUST_TEST_RESULT=cancelled RUST_CLIPPY_RESULT=success \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=false ANDROID_RESULT=skipped \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: applicable job cancelled fails gate" "$res" "1"

# Gate Test 5: Unexpected skip of an applicable job
res=0
CHANGES_RESULT=success \
RUST_NEEDED=true RUST_TEST_RESULT=skipped RUST_CLIPPY_RESULT=success \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=false ANDROID_RESULT=skipped \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: unexpected skip of applicable job fails gate" "$res" "1"

# Gate Test 6: Non-applicable job executed unexpectedly
res=0
CHANGES_RESULT=success \
RUST_NEEDED=false RUST_TEST_RESULT=success RUST_CLIPPY_RESULT=skipped \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=false ANDROID_RESULT=skipped \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: unexpected execution of non-applicable job fails gate" "$res" "1"

# Gate Test 7: Changes detection failed
res=0
CHANGES_RESULT=failure \
RUST_NEEDED=false RUST_TEST_RESULT=skipped RUST_CLIPPY_RESULT=skipped \
FRONTEND_NEEDED=false FRONTEND_RESULT=skipped \
DESKTOP_NEEDED=false TAURI_MACOS_RESULT=skipped \
SUPPLY_CHAIN_NEEDED=false SUPPLY_CHAIN_RESULT=skipped \
ANDROID_NEEDED=false ANDROID_RESULT=skipped \
"$GATE_SCRIPT" >/dev/null 2>&1 || res=$?
assert_eq "Gate: failed change detection fails gate" "$res" "1"

echo "=== Summary ==="
echo "Passed: $pass_count, Failed: $fail_count"
if [ "$fail_count" -gt 0 ]; then
  exit 1
fi
