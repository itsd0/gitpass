#!/bin/bash
# Build the gitpass-core XCFramework for iOS.
#
# What this does, in order:
#  1. Builds the UniFFI crate for aarch64-apple-ios, and for
#     aarch64-apple-ios-sim + x86_64-apple-ios (lipo'd into one
#     universal simulator library) as static libraries.
#  2. Builds the same crate for the host (macOS) as a cdylib, then
#     generates the Swift bindings, C header and XCFramework
#     modulemap from that library with the crate's own
#     uniffi-bindgen-swift binary (see uniffi.toml: the FFI module
#     is named GitpassCore, so the Swift file imports GitpassCore).
#  3. Assembles Frameworks/GitpassCore.xcframework from the two
#     static libraries and the generated headers.
#
# The generated Swift file is copied to ios/Generated/ and compiled
# into the app and extension targets (see project.yml); the
# XCFramework itself only carries the static libraries + headers.
# No signing, no team, no secrets are involved in any step.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IOS_DIR="$ROOT/ios"
FRAMEWORK_DIR="$IOS_DIR/Frameworks"
GENERATED_DIR="$IOS_DIR/Generated"
BINDINGS_DIR="$(mktemp -d)"

trap 'rm -rf "$BINDINGS_DIR"' EXIT

echo "==> Rust targets"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

echo "==> Static libraries (device + simulator)"
cargo build --release --lib -p gitpass-ios-ffi --target aarch64-apple-ios
cargo build --release --lib -p gitpass-ios-ffi --target aarch64-apple-ios-sim
# Intel simulator slice: `generic/platform=iOS Simulator` builds
# arm64 + x86_64, so the simulator library must carry both.
cargo build --release --lib -p gitpass-ios-ffi --target x86_64-apple-ios
SIM_LIB_DIR="$ROOT/target/ios-sim-universal/release"
mkdir -p "$SIM_LIB_DIR"
lipo -create \
    "$ROOT/target/aarch64-apple-ios-sim/release/libgitpass_ios_ffi.a" \
    "$ROOT/target/x86_64-apple-ios/release/libgitpass_ios_ffi.a" \
    -output "$SIM_LIB_DIR/libgitpass_ios_ffi.a"

echo "==> Host library for binding generation"
cargo build --release --lib -p gitpass-ios-ffi
HOST_LIB="$(find "$ROOT/target/release" -maxdepth 1 -name 'libgitpass_ios_ffi.dylib' | head -1)"
if [ -z "$HOST_LIB" ]; then
    echo "error: host cdylib not found (expected target/release/libgitpass_ios_ffi.dylib)" >&2
    exit 1
fi

echo "==> Swift bindings (GitpassCore module)"
cargo run --release -p gitpass-ios-ffi --bin uniffi-bindgen-swift -- \
    "$HOST_LIB" "$BINDINGS_DIR" \
    --swift-sources --headers --modulemap \
    --module-name GitpassCore --modulemap-filename module.modulemap

mkdir -p "$GENERATED_DIR" "$FRAMEWORK_DIR"
cp "$BINDINGS_DIR/gitpass_ios_ffi.swift" "$GENERATED_DIR/gitpass_ios_ffi.swift"

# Headers for the XCFramework: the generated header + modulemap.
# The XCFramework wraps plain static libraries (-library), so the
# modulemap must be a regular `module GitpassCore` (no --xcframework,
# which would emit a `framework module` that Xcode cannot import from
# a library slice). Headers go into a GitpassCore/ subdirectory so the
# modulemap does not collide with other XCFrameworks' Headers/ roots
# when Xcode copies them into the shared include path.
HEADERS_ROOT="$(mktemp -d)"
HEADERS_DIR="$HEADERS_ROOT/GitpassCore"
mkdir -p "$HEADERS_DIR"
cp "$BINDINGS_DIR"/*.h "$HEADERS_DIR/"
cp "$BINDINGS_DIR/module.modulemap" "$HEADERS_DIR/"

echo "==> XCFramework"
rm -rf "$FRAMEWORK_DIR/GitpassCore.xcframework"
xcodebuild -create-xcframework \
    -library "$ROOT/target/aarch64-apple-ios/release/libgitpass_ios_ffi.a" \
    -headers "$HEADERS_ROOT" \
    -library "$SIM_LIB_DIR/libgitpass_ios_ffi.a" \
    -headers "$HEADERS_ROOT" \
    -output "$FRAMEWORK_DIR/GitpassCore.xcframework"
rm -rf "$HEADERS_ROOT"

echo "Done: $FRAMEWORK_DIR/GitpassCore.xcframework"
echo "Bindings: $GENERATED_DIR/gitpass_ios_ffi.swift"
