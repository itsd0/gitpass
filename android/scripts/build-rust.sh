#!/usr/bin/env bash
# Build gitpass native libraries and generate UniFFI Kotlin bindings for Android.
#
# Targets:
#  - arm64-v8a (aarch64-linux-android)
#  - x86_64 (x86_64-linux-android)
#
# Outputs:
#  - android/app/src/main/jniLibs/arm64-v8a/libgitpass_ios_ffi.so
#  - android/app/src/main/jniLibs/x86_64/libgitpass_ios_ffi.so
#  - android/app/src/main/java/uniffi/gitpass_ios_ffi/gitpass_ios_ffi.kt
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$ANDROID_DIR/.." && pwd)"
FFI_DIR="$REPO_ROOT/ios/rust/gitpass-ios-ffi"
JNILIBS_DIR="$ANDROID_DIR/app/src/main/jniLibs"
JAVA_SRC_DIR="$ANDROID_DIR/app/src/main/java"

# Resolve Android NDK
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    if [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
        LATEST_NDK="$(ls -d "$ANDROID_HOME/ndk/"* 2>/dev/null | sort -V | tail -n 1)"
        if [ -n "$LATEST_NDK" ] && [ -d "$LATEST_NDK" ]; then
            export ANDROID_NDK_HOME="$LATEST_NDK"
        fi
    elif [ -d "$HOME/Library/Android/sdk/ndk" ]; then
        LATEST_NDK="$(ls -d "$HOME/Library/Android/sdk/ndk/"* 2>/dev/null | sort -V | tail -n 1)"
        if [ -n "$LATEST_NDK" ] && [ -d "$LATEST_NDK" ]; then
            export ANDROID_NDK_HOME="$LATEST_NDK"
        fi
    fi
fi

if [ -z "${ANDROID_NDK_HOME:-}" ] || [ ! -d "$ANDROID_NDK_HOME" ]; then
    echo "error: ANDROID_NDK_HOME not found. Please set ANDROID_NDK_HOME or install NDK via Android SDK manager." >&2
    exit 1
fi

echo "==> Using NDK: $ANDROID_NDK_HOME"

# Check cargo-ndk
if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "error: cargo-ndk not found in PATH. Install with: cargo install cargo-ndk" >&2
    exit 1
fi

# Ensure required rustup targets are installed
echo "==> Ensuring rustup targets are installed"
rustup target add aarch64-linux-android x86_64-linux-android

# Build native shared libraries for arm64-v8a and x86_64
echo "==> Building native libraries (arm64-v8a, x86_64)"
mkdir -p "$JNILIBS_DIR"
cargo ndk \
    --target arm64-v8a \
    --target x86_64 \
    --output-dir "$JNILIBS_DIR" \
    build \
    --release \
    --manifest-path "$FFI_DIR/Cargo.toml"

# Generate UniFFI Kotlin bindings using uniffi-bindgen
echo "==> Generating UniFFI Kotlin bindings"
BINDINGS_TMP="$(mktemp -d)"
trap 'rm -rf "$BINDINGS_TMP"' EXIT

cargo run --release \
    --manifest-path "$FFI_DIR/Cargo.toml" \
    --bin uniffi-bindgen -- \
    generate \
    --library "$JNILIBS_DIR/arm64-v8a/libgitpass_ios_ffi.so" \
    --language kotlin \
    --out-dir "$BINDINGS_TMP" \
    --no-format

# Copy generated Kotlin bindings to app/src/main/java
mkdir -p "$JAVA_SRC_DIR"
cp -R "$BINDINGS_TMP/"* "$JAVA_SRC_DIR/"

echo "==> Build complete"
echo "Native libraries in: $JNILIBS_DIR"
echo "Generated bindings in: $JAVA_SRC_DIR/uniffi"
