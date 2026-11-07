# gitpass Android Companion (Skeleton + UniFFI Bridge)

This directory contains the first approved Android milestone for the `gitpass` companion app, authorized in `AGENTS.md` (Issue #435 / PR #436).

## Scope & Status

- **Milestone 1 (current):** Buildable Kotlin + Jetpack Compose shell in `android/`, `#121212` dark background, `#F28C28` orange accent, localized string scaffolding (`en`, `my`, `th`), and a working Kotlin-to-Rust UniFFI bridge packaging `arm64-v8a` and `x86_64` native libraries.
- **Non-goals:** No vault CRUD, credential caching, biometrics, sync/OAuth, Autofill, passkeys, release signing, or crypto/KDF modifications. Temp/fake data only.

## Pinned Build Inputs

| Tool / Dependency | Version | Notes |
|---|---|---|
| JDK | OpenJDK 21 (`21.0.11`) | Required by AGP 8.6+ |
| Gradle | 8.10.2 | Wrapper included (`./gradlew`) |
| Android Gradle Plugin (AGP) | 8.6.1 | Defined in `gradle/libs.versions.toml` |
| Kotlin | 2.0.21 | Uses Compose compiler plugin |
| Compile / Target SDK | 35 | minSdk 26 (Android 8.0+) |
| NDK | 27.1.12297006 (r27+) | Set via `ANDROID_NDK_HOME` or Android SDK manager |
| Rust Toolchain | 1.90.0 / stable | Targets: `aarch64-linux-android`, `x86_64-linux-android` |
| cargo-ndk | 4.1.2 | `cargo install --locked cargo-ndk` |
| UniFFI | 0.29 | Direct reuse of `ios/rust/gitpass-ios-ffi` |
| JNA (Android AAR) | 5.14.0 | Runtime FFI library loader for UniFFI |

## Build Instructions

### 1. Prerequisites

Ensure `cargo-ndk` and the Android Rust targets are installed:

```bash
cargo install --locked cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
```

Set environment variables (adjust paths to your local setup):

```bash
export JAVA_HOME="/opt/homebrew/opt/openjdk@21" # or your JDK 21 path
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.1.12297006"
```

### 2. Build Native Libraries & Generate Bindings

```bash
./android/scripts/build-rust.sh
```

This compiles `libgitpass_ios_ffi.so` in release mode for `arm64-v8a` and `x86_64`, places them in `android/app/src/main/jniLibs/`, and generates the UniFFI Kotlin bindings in `android/app/src/main/java/uniffi/`.

### 3. Build Debug APK

```bash
cd android
./gradlew assembleDebug
```

The assembled APK is located at:
`android/app/build/outputs/apk/debug/app-debug.apk`

Contains:
- `lib/arm64-v8a/libgitpass_ios_ffi.so`
- `lib/x86_64/libgitpass_ios_ffi.so`
- JNA dispatch libraries and compiled Compose UI

### 4. Run Connected Smoke Test

On a connected device or running emulator:

```bash
cd android
./gradlew connectedDebugAndroidTest
```

Or install and launch directly:

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n app.gitpass.android/.MainActivity
```
