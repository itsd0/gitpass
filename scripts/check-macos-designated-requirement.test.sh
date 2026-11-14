#!/usr/bin/env bash
# Exercises the leaf check with a fake codesign. No certificate or secret is used.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
check="$repo/scripts/check-macos-designated-requirement.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/codesign" << 'EOF'
#!/bin/sh
printf '%s\n' "$FAKE_REQ"
exit "${FAKE_RC:-0}"
EOF
chmod +x "$tmp/codesign"

leaf="4F62E0AD9F26BEDB25E81EF10DC1ABB416BDDC82"
printf '%s\n' "$leaf" > "$tmp/leaf.txt"
good="designated => identifier \"dev.gitpass.desktop\" and certificate leaf = H\"${leaf}\""

run() {
  env -u APPLE_SIGNING_IDENTITY -u GITHUB_REF \
    CODESIGN="$tmp/codesign" EXPECTED_LEAF_FILE="$tmp/leaf.txt" \
    "$@"
}

FAKE_REQ="$good" FAKE_RC=0 run "$check" "$tmp/Gitpass.app" >/dev/null

FAKE_REQ='designated => identifier "dev.gitpass.desktop" and cdhash H"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' \
  FAKE_RC=0 run env APPLE_SIGNING_IDENTITY=- "$check" "$tmp/Gitpass.app" \
  | grep -q 'ad-hoc dry run'

if FAKE_REQ='designated => cdhash H"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' \
  FAKE_RC=0 run env APPLE_SIGNING_IDENTITY=- GITHUB_REF=refs/tags/v9.9.9 "$check" "$tmp/Gitpass.app" >/dev/null; then
  echo "tag ad-hoc run should fail" >&2
  exit 1
fi

if FAKE_REQ='designated => identifier "dev.gitpass.desktop" and certificate leaf = H"0000000000000000000000000000000000000000"' \
  FAKE_RC=0 run "$check" "$tmp/Gitpass.app" >/dev/null; then
  echo "wrong leaf should fail" >&2
  exit 1
fi

printf 'not-a-hash\n' > "$tmp/leaf.txt"
if FAKE_REQ="$good" FAKE_RC=0 run "$check" "$tmp/Gitpass.app" >/dev/null; then
  echo "bad expected hash should fail" >&2
  exit 1
fi

echo "designated-requirement checks passed"
