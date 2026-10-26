#!/bin/sh
# Sign gitpass-host so the desktop app's SecCode check accepts it:
# certificate "gitpass Local Signing", identifier dev.gitpass.host.
# usage: crates/gitpass-host/sign-macos.sh [path/to/gitpass-host]   (default: target/release/gitpass-host)
set -eu
BIN="${1:-${CARGO_TARGET_DIR:-target}/release/gitpass-host}"
IDENTITY="${GITPASS_SIGNING_IDENTITY:-gitpass Local Signing}"
codesign --force --sign "$IDENTITY" --identifier dev.gitpass.host --options runtime --timestamp=none "$BIN"
codesign -dvv "$BIN" 2>&1 | grep -E '^(Identifier|Authority)='
