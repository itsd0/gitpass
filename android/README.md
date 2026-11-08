# gitpass Android Companion (Skeleton + UniFFI Bridge)

This directory contains the first approved Android milestone for the `gitpass` companion app, authorized in `AGENTS.md` (Issue #435 / PR #436).

## Scope & Status

- **Milestone 1:** Buildable Kotlin + Jetpack Compose shell in `android/`, `#121212` dark background, `#F28C28` orange accent, localized strings (`en`, `values-my`, `values-th`), and a Kotlin-to-Rust UniFFI bridge packaging `arm64-v8a` and `x86_64` native libraries into a debug APK (#438).
- **Milestone 2 (current):** Local vault unlock with Master Password + Secret Key, in-account vault selection, item list, substring search, item detail, and lock (#441). A new account can be created on the device so there is a vault to unlock. The vault directory is `noBackupFilesDir/vault`. Locking drops the UniFFI `Vault` (and the core `Session`). The app also locks when it leaves the foreground, and after 5 minutes with no activity while it is open. `FLAG_SECURE` is set on the activity window. The one-time Secret Key from account creation stays on that secure screen until the owner confirms they saved it.
- **Not in this milestone:** Item editing, sync, Autofill, biometrics, Credential Provider passkeys, release signing, and crypto/KDF changes. Kotlin `String` values that cross the bridge are not wiped from the JVM heap. Search uses list summaries (title, subtitle, URL, category, tags), not notes or secrets. Deleted and archived items are hidden. Temp/fake data only.

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

## Stacked milestone: local login CRUD (#443)

Depends on the local-vault foundation in #442; do not merge before that foundation
has passed Android runtime/security regression tests and merged.

- Create login items and edit title, username, password, URL and notes. Other
  categories stay view-only for editing; existing custom sections, tags,
  favorite flag and TOTP are retained, not silently discarded.
- Move any item to Trash (core soft-delete) and restore it; no permanent delete.
  Archived items remain hidden in the active list as before.
- Writes capture vault ID and lock epoch, enforce inactivity and reject stale
  submissions after lock/re-unlock. Editor state is not saved across Activity
  recreation or lock; unsaved edits are deliberately discarded.
- Password generation, live TOTP, sync, Autofill and passkeys are separate work.
  No crypto, vault format, Swift API or workflow changes in this milestone.

Tests use fake/temp vaults only. Physical connected tests are not run (avoids
uninstall/data-loss risk). The foundation CI emulator currently needs a separately
authorized memory-provisioning workflow fix after a confirmed lowmemorykiller kill;
no green runtime claim is made until the whole instrumented suite executes.

## Native password generator and live TOTP (#445)

The login editor offers the existing core generator (letters/digits/symbols), with
length 12–64 and explicit regenerate, masked/reveal, apply and cancel. Applying
only changes the unsaved editor; only Save writes the item. No automatic copy or
plaintext persistence is added. Lock/background discards editor/dialog state.

TOTP fields display masked live codes, never seed values. Explicit Reveal starts
native `totpCode` refreshes at wall-second boundaries; Hide/lock discards codes.
Native core owns otpauth parsing, digits, algorithm, period and Unix-time rollover.
The monotonic inactivity clock is independent; code polling is not user activity.
Invalid configurations get a generic error with no native parser details. There
are no fake production algorithms or new crypto/format/Swift API changes.

Tests separate native RFC6238 vectors and generator lengths, fake-clock/session
validation, and actual Compose apply/cancel/lock and live-code/invalid-config UI.
All fixtures are fake/cache-only; no physical-device runner or data wipe is used.

## First-pull staging groundwork (#447) — NOT SYNC SETUP

`firstpull/EncryptedFirstPull` is an unconnected staging primitive, not a live
GitHub client or import flow. Explicit repository selection has no defaults.
Typed transport results are pinned to one commit; incomplete/oversized trees,
unsafe paths, symlinks/submodules/executable files, duplicates and unsupported
files fail closed. Blob size and Git SHA-1 identity are checked before writing.
Failures and close remove only the new encrypted staging copy; an existing
account is never touched. A cleanup helper removes UUID-named orphan stages only.

Do not expose an import/activation button yet. Missing integration gates:
- bounded HTTPS GitHub transport and redirect/credential-forwarding tests;
- session-memory PAT configuration UI cleared on lock (no maintainer token use);
- native authentication of the entire staged manifest and referenced files,
  with wrong-password and tamper regressions through the actual import path;
- atomic separate-account promotion/selection and restart/crash/cancel/lock tests,
  preserving the existing local account and cleaning orphan stages on startup.

There is deliberately no activation method or trusted/authenticated flag on the
staged-copy type. Downloaded encrypted bytes are not authenticated vault contents.
No real repository/token is configured and no network permission is added by this
partial groundwork. No two-way sync, remote writes, or destructive fallback.
