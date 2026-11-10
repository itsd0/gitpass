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
| UniFFI | 0.29.5 | 0.29+ | Shared crate reuse of `crates/gitpass-mobile-ffi` |
| JNA (Android AAR) | 5.17.0 | JNA 5.17.0+ | Exact compatible release containing Android 16-KB page fixes (#1647, #1624) |

## UniFFI Kotlin Generation & Namespace

The Kotlin FFI bridge reuses the `crates/gitpass-mobile-ffi` crate. Kotlin bindings are configured via `crates/gitpass-mobile-ffi/uniffi.toml`:

```toml
[bindings.swift]
ffi_module_name = "GitpassCore"

[bindings.kotlin]
package_name = "app.gitpass.core"
cdylib_name = "gitpass_mobile_ffi"
```

- **Generated Source Location:** `android/app/src/main/java/app/gitpass/core/gitpass_mobile_ffi.kt`.
- **Package:** `app.gitpass.core`.
- **Regeneration:** `android/scripts/build-rust.sh` runs `uniffi-bindgen generate --library ... --config crates/gitpass-mobile-ffi/uniffi.toml --language kotlin` and cleanly refreshes `app/gitpass/core/`, preventing namespace drift or duplicate stale files.
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
- **16-KB Kernel Status:** **Incomplete / Untested at Runtime.** The local development machine does not have a 16-KB page size kernel system image installed (`PAGE_SIZE=16384`). ELF load segment alignment is verified at 16-KB for all packaged binaries (`libgitpass_mobile_ffi.so` and `libjnidispatch.so`), but runtime execution on a true 16-KB kernel remains to be verified when an API 35+ 16-KB emulator image is provisioned.

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

Validates toolchain versions, compiles `libgitpass_mobile_ffi.so` with 16-KB page alignment for `arm64-v8a` and `x86_64`, and generates `app.gitpass.core` Kotlin bindings into `android/app/src/main/java/app/gitpass/core/gitpass_mobile_ffi.kt`.

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

## Selected-repository first pull (#447) — NOT TWO-WAY SYNC

The locked screen's **First pull** form requires an explicit owner, repository,
branch, dedicated PAT, Master Password and Secret Key. No repository defaults or
maintainer credentials are used. Restrict a fine-grained PAT to the selected
repository and **Contents: read-only**. PAT/password/key are memory-only, masked,
not saveable, and cleared on submit, cancel, disposal and lock/background. They
must be re-entered for another attempt. JVM strings/native call arguments cannot
be reliably wiped; cancellation cannot interrupt an Argon2 call already running.
There is no credential persistence, clipboard operation or hidden write API.

Production transport uses only `https://api.github.com`, disables redirects and
HTTP caching, and resolves the branch once before reading a pinned commit's tree
and blobs. Per-connection timeouts are 15s; the attempt deadline is five minutes
(checked before/after reads, so a blocked read may take up to 15s more). Responses,
requests, tree count (10000), files (4 MiB), and aggregate bytes (64 MiB) are bounded.
Paths/modes are allowlisted; symlinks, traversal, duplicate paths, truncated trees,
submodules and executables are rejected. Blob lengths and Git object hashes must
match. HTTP/parser errors never surface credential-bearing bodies or headers.
Only fake connections/typed fixture transports are used by tests, not real repos.

Before Argon2 allocation, an additive UniFFI import gate bounds `format.json` and
rejects unsupported downloaded KDF settings: Argon2id memory **8192..262144 KiB**,
iterations **1..6**, lanes **1..4**. These are acceptance floor/ceilings, NOT target
settings: no vault/KDF rewrite or clamping occurs. The normal production default
262144 KiB/t3/p4 is accepted on a capable device; costly unsupported vaults require
a different capable client, not weakened security. The existing global format,
crypto defaults and Swift entry points are unchanged.

`Session::reopen_verify(..., false)` authenticates the keyset, manifest and every
referenced hash (including attachments), checks rollback and rejects unexpected
encrypted entries. Every vault/item is additionally decrypted, including Trash.
Unlike ordinary unlock, this does not register devices, upgrade KDF, purge Trash
or repair/reseal integrity failures. Core writes only local throttle/epoch state
inside the staging copy. A keyset-only unlock is NEVER sufficient for activation.

Only the completed authenticated, fsynced copy is atomically renamed into a NEW
UUID account root. A separate non-secret, atomically replaced account-selection
journal records the selection and prior roots; the original legacy `vault` root
is retained and selectable. A session-mutex/locked-epoch gate serializes commit
with all lock paths; stale callbacks cannot activate after lock/account changes.
Cancellation/errors remove staging and unregistered imports, not old accounts.
Process startup cleans owned orphan stages and unregistered UUID imports; a
corrupt journal never authorizes cleanup. Registered account data is never deleted.
No automatic refresh, push, merge or bidirectional sync is implemented. First pull
creates a separate local snapshot; editing it does not update the GitHub repository.

This does NOT mean the owner's sync is configured: actual owner/repo/branch and
a dedicated read-only credential must be supplied by the owner inside the app.
The development GitHub token is never a data-plane credential. No real vault,
physical-device runner, uninstall or app-data wipe is used by this milestone.
