#!/usr/bin/env bash
# Fail when a signed gitpass.app no longer pins the expected certificate leaf.
# The hash is public (it is inside the designated requirement). This script
# does not read or print a private key, a .p12, or a keychain password.
set -euo pipefail

app="${1:?path to gitpass.app}"
repo="$(cd "$(dirname "$0")/.." && pwd)"
leaf_file="${EXPECTED_LEAF_FILE:-$repo/apps/desktop/src-tauri/macos-leaf-sha1.txt}"
codesign_bin="${CODESIGN:-codesign}"

leaf="$(tr -d '[:space:]' < "$leaf_file" | tr '[:lower:]' '[:upper:]')"
if ! [[ "$leaf" =~ ^[0-9A-F]{40}$ ]]; then
  echo "::error::expected leaf hash is not 40 hex characters"
  exit 1
fi

# Dry runs without the certificate are ad-hoc signed. A tag release never
# reaches this branch: preflight already refuses a missing certificate.
if [ "${APPLE_SIGNING_IDENTITY:-}" = "-" ]; then
  echo "::warning::ad-hoc dry run: designated-requirement leaf check skipped"
  if [[ "${GITHUB_REF:-}" == refs/tags/v* ]]; then
    echo "::error::tag release is ad-hoc signed; refusing to publish"
    exit 1
  fi
  exit 0
fi

if ! req="$("$codesign_bin" -d -r- "$app" 2>&1)"; then
  echo "::error::could not read the designated requirement"
  exit 1
fi

want_id='identifier "dev.gitpass.desktop"'
want_leaf="certificate leaf = H\"${leaf}\""
if ! grep -F -q -- "$want_id" <<<"$req" || ! grep -F -q -- "$want_leaf" <<<"$req"; then
  echo "::error::designated requirement does not pin dev.gitpass.desktop and leaf ${leaf}. Touch ID Keychain trust would break. See docs/RELEASE.md."
  printf '%s\n' "$req" | grep -E 'designated|identifier|certificate leaf' || true
  exit 1
fi

echo "designated requirement pins dev.gitpass.desktop and leaf ${leaf}"
