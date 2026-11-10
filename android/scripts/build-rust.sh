#!/usr/bin/env bash
# Build gitpass native libraries and generate UniFFI Kotlin bindings for Android.
#
# Pinned reproducible inputs:
#  - Rust toolchain: 1.90.0 (matches repo CI)
#  - Android NDK: 27.1.12297006 (exact pinned release)
#  - cargo-ndk: 4.1.2 (installed via cargo install --version 4.1.2 --locked cargo-ndk)
#
# Targets:
#  - arm64-v8a (aarch64-linux-android)
#  - x86_64 (x86_64-linux-android)
#
# Outputs:
#  - android/app/src/main/jniLibs/arm64-v8a/libgitpass_mobile_ffi.so
#  - android/app/src/main/jniLibs/x86_64/libgitpass_mobile_ffi.so
#  - android/app/src/main/java/app/gitpass/core/gitpass_mobile_ffi.kt
set -euo pipefail

REQUIRED_RUST_VERSION="1.90.0"
REQUIRED_NDK_VERSION="27.1.12297006"
REQUIRED_CARGO_NDK_VERSION="4.1.2"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$ANDROID_DIR/.." && pwd)"
FFI_DIR="$REPO_ROOT/crates/gitpass-mobile-ffi"
JNILIBS_DIR="$ANDROID_DIR/app/src/main/jniLibs"
JAVA_SRC_DIR="$ANDROID_DIR/app/src/main/java"

echo "==> Verifying pinned toolchain inputs"

# 1. Validate Rust 1.90.0
TOOLCHAIN_ARG=""
if rustup toolchain list | grep -q "^1.90.0-"; then
    TOOLCHAIN_ARG="+1.90.0"
    export RUSTUP_TOOLCHAIN="1.90.0"
elif rustc --version 2>/dev/null | grep -q "1.90.0"; then
    TOOLCHAIN_ARG=""
else
    echo "error: Rust $REQUIRED_RUST_VERSION is required (matches CI toolchain)." >&2
    echo "       Install it with: rustup toolchain install $REQUIRED_RUST_VERSION -c clippy" >&2
    exit 1
fi
echo "    Rust toolchain: $(rustc $TOOLCHAIN_ARG --version)"

# 2. Validate exact NDK 27.1.12297006
RESOLVED_NDK=""
if [ -n "${ANDROID_NDK_HOME:-}" ] && [ -d "$ANDROID_NDK_HOME" ]; then
    RESOLVED_NDK="$ANDROID_NDK_HOME"
elif [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk/$REQUIRED_NDK_VERSION" ]; then
    RESOLVED_NDK="$ANDROID_HOME/ndk/$REQUIRED_NDK_VERSION"
elif [ -d "$HOME/Library/Android/sdk/ndk/$REQUIRED_NDK_VERSION" ]; then
    RESOLVED_NDK="$HOME/Library/Android/sdk/ndk/$REQUIRED_NDK_VERSION"
fi

if [ -z "$RESOLVED_NDK" ] || [ ! -d "$RESOLVED_NDK" ]; then
    echo "error: Android NDK $REQUIRED_NDK_VERSION is required but was not found." >&2
    echo "       Please set ANDROID_NDK_HOME or install via Android SDK manager:" >&2
    echo "       sdkmanager 'ndk;$REQUIRED_NDK_VERSION'" >&2
    exit 1
fi

NDK_PROP="$RESOLVED_NDK/source.properties"
if [ -f "$NDK_PROP" ]; then
    NDK_REV="$(grep "^Pkg.Revision" "$NDK_PROP" | awk -F'= ' '{print $2}' | tr -d ' \r\n')"
    if [ "$NDK_REV" != "$REQUIRED_NDK_VERSION" ]; then
        echo "error: Found NDK at $RESOLVED_NDK with version '$NDK_REV', expected '$REQUIRED_NDK_VERSION'." >&2
        exit 1
    fi
fi
export ANDROID_NDK_HOME="$RESOLVED_NDK"
echo "    Android NDK: $ANDROID_NDK_HOME ($REQUIRED_NDK_VERSION)"

# 3. Validate cargo-ndk 4.1.2
if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "error: cargo-ndk is not installed." >&2
    echo "       Install with: cargo install --version $REQUIRED_CARGO_NDK_VERSION --locked cargo-ndk" >&2
    exit 1
fi
INSTALLED_CARGO_NDK_VER="$(cargo ndk --version 2>/dev/null | awk '{print $2}' | tr -d ' \r\n')"
if [ "$INSTALLED_CARGO_NDK_VER" != "$REQUIRED_CARGO_NDK_VERSION" ]; then
    echo "error: cargo-ndk version $INSTALLED_CARGO_NDK_VER does not match required $REQUIRED_CARGO_NDK_VERSION." >&2
    echo "       Run: cargo install --version $REQUIRED_CARGO_NDK_VERSION --locked cargo-ndk" >&2
    exit 1
fi
echo "    cargo-ndk: $INSTALLED_CARGO_NDK_VER"

# 4. Ensure required Rust targets are installed for toolchain
echo "==> Ensuring rustup targets are installed for Rust $REQUIRED_RUST_VERSION"
rustup target add aarch64-linux-android x86_64-linux-android ${TOOLCHAIN_ARG:+--toolchain $REQUIRED_RUST_VERSION}

# 5. Build native shared libraries for arm64-v8a and x86_64 with 16KB max-page-size ELF alignment
echo "==> Building native shared libraries (arm64-v8a, x86_64) with 16-KB page alignment"
mkdir -p "$JNILIBS_DIR"
# Drop native libraries left over from an earlier bridge name so they are never packaged.
find "$JNILIBS_DIR" -name 'libgitpass_*ffi.so' -delete
RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384" \
cargo $TOOLCHAIN_ARG ndk \
    --target arm64-v8a \
    --target x86_64 \
    --output-dir "$JNILIBS_DIR" \
    build \
    --release \
    --locked \
    --manifest-path "$FFI_DIR/Cargo.toml"

# 6. Generate UniFFI Kotlin bindings using uniffi-bindgen and explicit uniffi.toml config
echo "==> Generating UniFFI Kotlin bindings using uniffi.toml config"
BINDINGS_TMP="$(mktemp -d)"
trap 'rm -rf "$BINDINGS_TMP"' EXIT

cargo $TOOLCHAIN_ARG run --release --locked \
    --manifest-path "$FFI_DIR/Cargo.toml" \
    --bin uniffi-bindgen -- \
    generate \
    --library "$JNILIBS_DIR/arm64-v8a/libgitpass_mobile_ffi.so" \
    --config "$FFI_DIR/uniffi.toml" \
    --language kotlin \
    --out-dir "$BINDINGS_TMP" \
    --no-format

# 7. Clean stale bindings and copy generated package to app.gitpass.core
echo "==> Updating generated sources in app/src/main/java"
rm -rf "$JAVA_SRC_DIR/uniffi"
rm -rf "$JAVA_SRC_DIR/app/gitpass/core"
mkdir -p "$JAVA_SRC_DIR/app/gitpass/core"
cp -R "$BINDINGS_TMP/app/gitpass/core/"* "$JAVA_SRC_DIR/app/gitpass/core/"

echo "==> Build complete"
echo "Native libraries in: $JNILIBS_DIR"
echo "Generated bindings in: $JAVA_SRC_DIR/app/gitpass/core/gitpass_mobile_ffi.kt"
