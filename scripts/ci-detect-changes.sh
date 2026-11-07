#!/usr/bin/env bash
# Classifies repository changes into CI target areas without regex lookahead.
#
# Supported inputs:
#  - Piped via stdin: echo "file" | scripts/ci-detect-changes.sh
#  - Via env CI_DETECT_FILES: CI_DETECT_FILES="file" scripts/ci-detect-changes.sh
#  - Via git diff: uses env EVENT, BASE, HEAD
#
# Outputs key=value pairs:
#  code, rust, desktop, frontend, supply_chain, android
set -euo pipefail

EVENT="${EVENT:-}"
BASE="${BASE:-}"
HEAD="${HEAD:-HEAD}"

files=""

if [ -n "${CI_DETECT_FILES:-}" ]; then
  files="$CI_DETECT_FILES"
elif [ ! -t 0 ]; then
  files="$(cat)"
elif [ "$EVENT" = "workflow_dispatch" ] || [ "$EVENT" = "merge_group" ]; then
  # Full suite runs on manual dispatch and merge queue runs
  files="__ALL__"
elif [ -n "$BASE" ] && [ "$BASE" != "0000000000000000000000000000000000000000" ] && git cat-file -e "$BASE^{commit}" 2>/dev/null; then
  files="$(git diff --name-only "$BASE" "$HEAD")"
else
  # Fallback when base commit cannot be resolved
  files="__ALL__"
fi

if [ "$files" = "__ALL__" ]; then
  echo "code=true"
  echo "rust=true"
  echo "desktop=true"
  echo "frontend=true"
  echo "supply_chain=true"
  echo "android=true"
  exit 0
fi

code=false
rust=false
desktop=false
frontend=false
supply_chain=false
android=false

while IFS= read -r file; do
  [ -z "$file" ] && continue

  case "$file" in
    # Documentation, licenses, repository metadata (docs-only)
    *.md|docs/*|LICENSE|.gitignore|.gitattributes|.kombai/*)
      ;;

    # CI workflow definition changes trigger full verification
    .github/workflows/ci.yml)
      code=true; rust=true; desktop=true; frontend=true; supply_chain=true; android=true
      ;;

    # Other workflow files (e.g., ios.yml, release.yml)
    .github/workflows/*)
      code=true
      ;;

    # Android companion app sources, Gradle build scripts, and wrappers
    android/*)
      code=true; android=true
      ;;

    # Shared mobile UniFFI Rust bridge (used by Android & iOS companions)
    ios/rust/gitpass-ios-ffi/*)
      code=true; rust=true; android=true; supply_chain=true
      ;;

    # iOS app sources, tests, Xcode project (Swift-only)
    ios/*)
      code=true
      ;;

    # Universal core vault & crypto engine
    crates/gitpass-core/*)
      code=true; rust=true; desktop=true; android=true; supply_chain=true
      ;;

    # Root workspace Cargo manifests and rust configuration
    Cargo.toml|Cargo.lock|.cargo/*)
      code=true; rust=true; desktop=true; android=true; supply_chain=true
      ;;

    # Supply chain security configuration
    deny.toml)
      code=true; supply_chain=true
      ;;

    # Desktop Rust backend
    apps/desktop/gui-core/*)
      code=true; rust=true; desktop=true; supply_chain=true
      ;;

    # Desktop Tauri macOS shell
    apps/desktop/src-tauri/*)
      code=true; desktop=true; supply_chain=true
      ;;

    # Desktop web UI sources & configs
    apps/desktop/src/*|apps/desktop/index.html|apps/desktop/vite.config.ts|apps/desktop/tsconfig*.json|apps/desktop/package*.json)
      code=true; desktop=true; frontend=true
      ;;

    # Other desktop app files
    apps/desktop/*)
      code=true; desktop=true; frontend=true
      ;;

    # Chromium browser extension
    extension/*)
      code=true; frontend=true
      ;;

    # CLI crate
    crates/gitpass-cli/*)
      code=true; rust=true; supply_chain=true
      ;;

    # Native messaging host crate (used by desktop and extension)
    crates/gitpass-host/*)
      code=true; rust=true; desktop=true; frontend=true; supply_chain=true
      ;;

    # Concealed pasteboard crate
    crates/gitpass-pasteboard/*)
      code=true; rust=true; desktop=true; supply_chain=true
      ;;

    # Version alignment script
    scripts/check-tauri-versions.mjs)
      code=true; desktop=true; frontend=true
      ;;

    # CI helper scripts
    scripts/ci-detect-changes.sh|scripts/ci-ok-gate.sh|scripts/test-ci.sh)
      code=true; rust=true; desktop=true; frontend=true; supply_chain=true; android=true
      ;;

    # Other repository scripts
    scripts/*)
      code=true; desktop=true
      ;;

    # Any unrecognized file conservatively triggers all suites
    *)
      code=true; rust=true; desktop=true; frontend=true; supply_chain=true; android=true
      ;;
  esac
done <<< "$files"

echo "code=$code"
echo "rust=$rust"
echo "desktop=$desktop"
echo "frontend=$frontend"
echo "supply_chain=$supply_chain"
echo "android=$android"
