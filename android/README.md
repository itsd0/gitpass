# gitpass Android Companion (Skeleton + UniFFI Bridge)

This directory contains the first approved Android milestone for the `gitpass` companion app, authorized in `AGENTS.md` (Issue #435 / PR #436).

## Scope & Status

- **Milestone 1 (current):** Buildable Kotlin + Jetpack Compose shell in `android/`, `#121212` dark background, `#F28C28` orange accent, localized string scaffolding (`en`, `values-my`, `values-th`), and a working Kotlin-to-Rust UniFFI bridge packaging `arm64-v8a` and `x86_64` native libraries into a debug APK.
- **Non-goals:** No vault CRUD, credential caching, biometrics, sync/OAuth, Autofill, passkeys, release signing, or crypto/KDF modifications. Temp/fake data only.

## Pinned Build Inputs

| Tool / Dependency | Exact Pinned Version | Upstream Minimum / Constraints | Notes / Rationale |
|---|---|---|---|
| JDK | OpenJDK 21 (`21.0.11`) | JDK 17+ | Required by AGP 8.6+; standard LTS JDK |
| Gradle | 8.10.2 | Gradle 8.7+ | Wrapper included (`./gradlew`) with SHA-256 validation |
| Gradle Wrapper Checksum | `31c55713e40233a8303827ceb42ca48a47267a0ad4bab9177123121e71524c26` | N/A | Pinned in `gradle-wrapper.properties` (`distributionSha256Sum`) |
| AGP | 8.6.1 | AGP 8.4+ | Defined in `gradle/libs.versions.toml` |
| Kotlin | 2.0.21 | Kotlin 2.0+ | Compatible with AGP 8.6.1 and Compose compiler |
| Compile / Target SDK | 35 | minSdk 26 | minSdk 26 covers Android 8.0+ |
| NDK | 27.1.12297006 (r27b) | NDK r26+ | Pinned exact release; enforced by `android/scripts/build-rust.sh` |
| Rust Toolchain | 1.90.0 | 1.85+ (`Cargo.toml`) | Deliberate project choice: matches repository CI toolchain |
| cargo-ndk | 4.1.2 | cargo-ndk 3.x+ | Installed with `cargo install --version 4.1.2 --locked cargo-ndk` |
| UniFFI | 0.29.5 | 0.29+ | Shared crate reuse of `ios/rust/gitpass-ios-ffi` |
| JNA (Android AAR) | 5.17.0 | JNA 5.17.0+ | Exact compatible release containing Android 16-KB page fixes (#1647, #1624) |

## UniFFI Kotlin Generation & Namespace

The Kotlin FFI bridge reuses the `ios/rust/gitpass-ios-ffi` crate. Kotlin bindings are configured via `ios/rust/gitpass-ios-ffi/uniffi.toml`:

```toml
[bindings.swift]
ffi_module_name = "GitpassCore"

[bindings.kotlin]
package_name = "app.gitpass.core"
cdylib_name = "gitpass_ios_ffi"
```

- **Generated Source Location:** `android/app/src/main/java/app/gitpass/core/gitpass_ios_ffi.kt`.
- **Package:** `app.gitpass.core`.
- **Regeneration:** `android/scripts/build-rust.sh` runs `uniffi-bindgen generate --library ... --config ios/rust/gitpass-ios-ffi/uniffi.toml --language kotlin` and cleanly refreshes `app/gitpass/core/`, preventing namespace drift or duplicate stale files.
- **iOS Compatibility:** Preserves existing Swift module name (`GitpassCore`), Swift binding generation, and xcframework builds without modification.

## Android 16-KB Page Size Compatibility

Android 15+ devices support 16-KB memory page sizes. Native libraries must have their ELF `LOAD` segments aligned to at least 16,384 bytes (`0x4000`).

1. **Rust cdylib:** Built with `RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"` in `android/scripts/build-rust.sh`.
2. **JNA:** Upgraded to `5.17.0@aar`, which includes upstream ELF alignment and memory mapping fixes for Android 16-KB pages (fixes upstream JNA issues #1647 and #1624).
3. **Packaging:** Both `arm64-v8a` and `x86_64` `.so` files packaged in `app-debug.apk` verify with `Align >= 0x4000`.

### Verifying ELF Alignment

Inspect the packaged `.so` files extracted from `app-debug.apk` using `llvm-readelf`:

```bash
python3 -c "
import zipfile, subprocess, os, tempfile

apk_path = 'app/build/outputs/apk/debug/app-debug.apk'
ndk_readelf = '$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-readelf'

with tempfile.TemporaryDirectory() as tmpdir:
    with zipfile.ZipFile(apk_path, 'r') as z:
        for info in z.infolist():
            if info.filename.endswith('.so'):
                z.extract(info, tmpdir)
                so_path = os.path.join(tmpdir, info.filename)
                print(f'=== Checking {info.filename} ===')
                out = subprocess.check_output([ndk_readelf, '-lW', so_path], text=True)
                for l in out.splitlines():
                    if 'LOAD' in l:
                        print('  ', l.strip())
"
```

All `LOAD` segment lines must indicate `Align 0x4000` (16,384).

### 16-KB Runtime Testing Status

- **Tested Runtime Environments:** Android emulator API 34 (`arm64-v8a`) and API 36 (`arm64-v8a`) running standard 4-KB kernel page size (`getconf PAGE_SIZE` = 4096). All instrumentation smoke tests passed (2/2).
- **16-KB Kernel Status:** **Incomplete / Untested at Runtime.** The local development machine does not have a 16-KB page size kernel system image installed (`PAGE_SIZE=16384`). ELF load segment alignment is verified at 16-KB for all packaged binaries (`libgitpass_ios_ffi.so` and `libjnidispatch.so`), but runtime execution on a true 16-KB kernel remains to be verified when an API 35+ 16-KB emulator image is provisioned.

## Build Instructions

### 1. Prerequisites

Install the pinned tools:

```bash
# Rust 1.90.0 (matches repo CI)
rustup toolchain install 1.90.0 -c clippy
rustup target add aarch64-linux-android x86_64-linux-android --toolchain 1.90.0

# cargo-ndk 4.1.2 pinned
cargo install --version 4.1.2 --locked cargo-ndk
```

Set environment variables:

```bash
export JAVA_HOME="/opt/homebrew/opt/openjdk@21"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.1.12297006"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"
```

### 2. Build Native Libraries & Regenerate Bindings

```bash
./android/scripts/build-rust.sh
```

Validates toolchain versions, compiles `libgitpass_ios_ffi.so` with 16-KB page alignment for `arm64-v8a` and `x86_64`, and generates `app.gitpass.core` Kotlin bindings into `android/app/src/main/java/app/gitpass/core/gitpass_ios_ffi.kt`.

### 3. Assemble Debug APK

```bash
cd android
./gradlew assembleDebug
```

Output: `android/app/build/outputs/apk/debug/app-debug.apk`

### 4. Run Connected Instrumentation Smoke Test

With an emulator or device attached:

```bash
cd android
./gradlew connectedDebugAndroidTest
```
