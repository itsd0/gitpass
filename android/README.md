# gitpass Android Companion (Skeleton + UniFFI Bridge)

This directory contains the first approved Android milestone for the `gitpass` companion app, authorized in `AGENTS.md` (Issue #435 / PR #436).

## Scope & Status

- **Milestone 1:** Buildable Kotlin + Jetpack Compose shell in `android/`, `#121212` dark background, `#F28C28` orange accent, localized strings (`en`, `values-my`, `values-th`), and a Kotlin-to-Rust UniFFI bridge packaging `arm64-v8a` and `x86_64` native libraries into a debug APK (#438).
- **Milestone 2 (current):** Local vault unlock with Master Password + Secret Key, in-account vault selection, item list, substring search, item detail, and lock (#441). A new account can be created on the device so there is a vault to unlock. The vault directory is `noBackupFilesDir/vault`. Locking drops the UniFFI `Vault` (and the core `Session`). The app also locks when it leaves the foreground, and after 5 minutes with no activity while it is open. `FLAG_SECURE` is set on the activity window. The one-time Secret Key from account creation stays on that secure screen until the owner confirms they saved it.
- **Not in this milestone:** Item editing, sync, Credential Provider passkeys, release signing, and crypto/KDF changes. (Since then: passkeys #477; signed release builds #479, see `docs/RELEASE.md`.) Kotlin `String` values that cross the bridge are not wiped from the JVM heap. Search uses list summaries (title, subtitle, URL, category, tags), not notes or secrets. Deleted and archived items are hidden. Temp/fake data only.

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
First pull itself never pushes or merges; it creates a separate local snapshot.
To keep that account in sync afterwards, set up two-way sync (#455, below).

This does NOT mean the owner's sync is configured: actual owner/repo/branch and
a dedicated read-only credential must be supplied by the owner inside the app.
The development GitHub token is never a data-plane credential. No real vault,
physical-device runner, uninstall or app-data wipe is used by this milestone.

## Two-way sync on the shared core engine (#455)

The unlocked list shows a sync status line (**Sync** opens the sync screen):
last successful sync, *Syncing…*, offline, failure text, and the number of
conflicts in the last sync, with the conflict list and **Sync now** on the
sync screen.

**Engine.** `Vault.syncGithub(target, token, transport, cancellation)` in
`gitpass-mobile-ffi` runs `gitpass_core::sync_remote::sync_with_remote` over
`gitpass_core::github_api::GitHubApi`: the same engine desktop and CLI use
(#452/#453/#454). Pull, field-level merge, delete propagation in both
directions, conflict report (fail-safe: an edit beats a delete, equal-clock
edits keep the local value), rollback refusal, foreign-file refusal, refusal of
a remote password/KDF change, bounded retries when the branch moves, and the
initial commit into an empty repository all run in Rust. Kotlin only supplies
the HTTPS round trip (`sync/GitHubSyncTransport.kt`, platform
`HttpsURLConnection`): fixed origin `https://api.github.com`, no redirects, no
cache, bounded reads, cancellation between chunks, no lower-layer error text.
The native library carries no TLS code. No crypto is re-implemented in Kotlin.

**Triggers** (`sync/SyncCoordinator.kt`, one sync at a time on one background
thread): on unlock, 3 s after the last save/trash/restore, every 15 minutes
while the vault stays unlocked, and **Sync now**. Each needs saved settings, a
network with internet capability and an unlocked session. A lock (Lock button,
leaving the foreground, inactivity) cancels pending and running syncs without
waiting for the network. While a sync holds the native session, item actions
fail fast with "Sync in progress; try again in a moment" instead of blocking.
There is **no background sync while locked** (no WorkManager job): the vault
locks when the app leaves the foreground and the Master Password is never
stored, so there is nothing to sync with.

**What is stored, and how** (`sync/SyncSettingsStore.kt`), per account, in
`noBackupFilesDir/<account>.sync.json` (next to, never inside, the vault
directory; `allowBackup=false`):
- owner, repository, branch, last successful sync time: plain JSON (not secret);
- the GitHub token: AES-256-GCM sealed with a non-exportable Android Keystore
  key (`gitpass.sync.token.v1`, hardware-backed where available, no user
  authentication so automatic sync can run while unlocked). If the key is lost,
  sync reports "not set up" and the token must be re-entered.
- never stored: Master Password, Secret Key, account key, decrypted items.

The token is decrypted only for a sync, passed to Rust for validation
(fine-grained `github_pat_…` only) and to the transport for the
`Authorization` header; it is not part of the request record crossing the
bridge, never logged, never shown (the field is never prefilled) and never in
errors. Use a dedicated fine-grained PAT restricted to the vault repository
with **Contents: read and write**. JVM strings cannot be reliably wiped.

**Tests.** Rust: `crates/gitpass-mobile-ffi/tests/sync_github.rs` (bridge →
core engine → in-process fake GitHub). JVM: `GitHubSyncTransportTest`,
`SyncSettingsStoreTest`, `SyncCoordinatorTest` (fake transport, fake cipher,
manual scheduler). Instrumented (API 35 16 KB emulator): `SyncStatusUiTest`
(status flow on the real screens with a test-only Keystore key and a faked
engine boundary) and `SyncPlatformTest` (Keystore sealing, PATCH support of
the platform HTTPS stack). No network, no real repository, fake data only.
Manual Redmi ↔ desktop verification is still to be done by the owner.

## Opt-in biometric unlock (#473)

On a Master Password unlock, tick **Use biometrics to unlock next time**
(shown only on Android 11+ with an enrolled BIOMETRIC_STRONG biometric). After
one strong biometric check the unlock screen offers **Unlock with
biometrics**; **Turn off biometric unlock** on the unlocked list removes it.

**What is wrapped.** Not the Master Password. gitpass-core derives the 32-byte
Account Unlock Key (AUK) with the vault's own Argon2id parameters (never
lowered) combined with the Secret Key, verifies it by opening `keyset.enc`, and
hands it over the bridge (`derive_quick_unlock_key`, bridge API level 3). The
same unlock then opens the session with that key, so enabling costs no extra
KDF run. Why the AUK and not the password:
- the Master Password is never stored, not even encrypted, so a compromise of
  the Keystore key or the sealed blob never reveals a password reused
  elsewhere;
- the AUK opens only this account's keyset; it is useless for any other vault
  and cannot be used to log in elsewhere;
- it goes stale by itself when the password, Secret Key or KDF parameters
  change (the keyset is re-wrapped); the next biometric unlock fails AEAD
  verification in Rust and the app falls back to the Master Password;
- the Secret Key is not stored separately either (it is folded into the AUK).

**Where and how it is stored** (`biometric/QuickUnlockStore.kt`):
`noBackupFilesDir/<account>.quick-unlock` (next to, never inside, the vault
directory; `allowBackup=false`; not in SharedPreferences). Format:
version | IV | AES-256-GCM(AUK) with the account path as associated data. The
key (`biometric/KeystoreQuickUnlockKeys.kt`, alias
`gitpass.quickunlock.v1.<hash of account path>`) is a non-exportable Android
Keystore AES-256-GCM key, StrongBox-backed when available, with
`setUserAuthenticationRequired(true)`,
`setUserAuthenticationParameters(0, AUTH_BIOMETRIC_STRONG)` (one
authentication per use, no device-credential fallback, no validity window),
`setInvalidatedByBiometricEnrollment(true)` and
`setUnlockedDeviceRequired(true)`. The prompt is the platform
`android.hardware.biometrics.BiometricPrompt` bound to the cipher
(`CryptoObject`), so the AUK can only be unsealed right after a strong
biometric match.

**Fallback.** Enrolling or removing a biometric, removing the screen lock
(`KeyPermanentlyInvalidatedException`), a damaged blob, or a stale AUK
(password/Secret Key/KDF changed elsewhere) deletes the blob and the key,
shows a notice, and pre-ticks the setup box for the next Master Password
unlock. Cancelling the prompt or choosing **Use Master Password** keeps the
password fields.

**Unchanged.** A biometric unlock is side-effect free like the password
unlock (no device registration, no trash purge, no KDF upgrade; the bridge
refuses those without the password) and still runs the integrity and rollback
checks. Locking, the inactivity lock and lock-on-background are unchanged.
Biometrics do **not** enable background sync while locked: the AUK is unsealed
only inside a foreground prompt, and sync still runs only while unlocked.

**Tests.** Rust: `crates/gitpass-core/tests/quick_unlock.rs`,
`crates/gitpass-mobile-ffi/tests/quick_unlock.rs`. JVM: `QuickUnlockStoreTest`
(software stand-in for the Keystore key: round trip, wipe, AAD binding,
tamper, invalidation, disable), `QuickUnlockSessionTest`. Instrumented:
`BiometricUnlockUiTest` (real screens; prompt and Keystore key faked: opt in,
biometric unlock, invalidation fallback, stale fallback, turn off, cancelled
setup) and `BiometricKeystoreTest` (real Keystore: fails closed without an
enrolled biometric; real bridge: derived key opens, wrong key is stale). CI
emulators have no enrolled biometric, so the real prompt and real
enrolment-change invalidation must be checked on a device.

## Password autofill (#475)

Passwords only (no passkeys). Turn it on with **Turn on autofill** on the
unlocked list (opens the system autofill picker), or in Android Settings →
Passwords & accounts → Autofill service.

**Detection** (`autofill/FieldParser.kt`): autofill hints and HTML
`autocomplete` first, then input types (`textPassword`, `textWebPassword`,
`textEmailAddress`, numeric password, HTML `type=password|email`), then
id/name/hint words. The text field right before the first password field
becomes the username when nothing better was found. One-time-code, search,
captcha, hidden and invisible fields are ignored.

**Matching** (`autofill/AutofillTarget.kt` + `gitpass_core::origin`): exactly
one identity per request.
- Browsers on the trusted list (Chrome, Firefox, Brave, Edge, Samsung, Mi
  Browser, …) report the page domain; the origin `https://<domain>` is matched
  with the same eTLD+1 rule the browser extension uses (moved from
  gitpass-host into gitpass-core: `paypal.com.evil.io` never matches
  `paypal.com`; `http` pages never match except loopback).
- Every other app is matched by its package (reported by the OS) against item
  URLs `android://<package>`, `androidapp://<package>` or
  `android://<cert-hash>@<package>/`. A web domain reported by a non-browser
  app (e.g. a WebView) is ignored, so an app cannot obtain another site's
  login.
- Matching runs in Rust (`Vault.autofill_logins`, bridge API level 5; level 4
  is reserved for device pairing #457) over live login items (main URL and
  URL fields); archived and trashed items are never offered.

**Unlock before any secret.** The service only ever answers with one
"Unlock gitpass to fill" entry. Its Intent opens `AutofillAuthActivity`, which
unlocks with biometrics (#473) or Master Password + Secret Key, asks the core
for matching logins and only then builds datasets (title + username shown,
values filled on selection). No match returns nothing. When the framework
attaches the view structure, the activity re-derives the target from it and
refuses a request whose target differs from the service's. A vault unlocked
for autofill is locked again before the activity finishes or stops.

**Save prompt.** Forms with a password field carry `SaveInfo`. On save
(Android 9+), the typed values stay in process memory (`SaveStash`, random
single-use token, 5 minute expiry; never in the Intent) and
`AutofillSaveActivity` asks for an unlock and a confirmation. It updates the
password of the matching item with the same username, otherwise creates a
login (title = domain or package, URL = origin or `android://<package>`), then
locks again.

**Tests.** Rust: `crates/gitpass-core/tests/origin_android.rs`,
`crates/gitpass-mobile-ffi/tests/autofill.rs`. JVM: `FieldParserTest`,
`AutofillTargetTest`, `FillPlanTest`, `SaveStashTest`, `AutofillSessionTest`.
Instrumented: `AutofillUiTest` (unlock activities: nothing before unlock,
datasets after, mismatch returns nothing, vault re-locked, save updates).
The system autofill flow itself (choosing gitpass as the service, real apps
and browsers) is not automated and must be checked on a device.

## Passkey provider (#477, Android 14+)

gitpass can be the passkey provider for Android Credential Manager on
Android 14 (API 34) and newer: **Passkey settings** on the unlocked list
opens the system Credential Manager settings, where gitpass is turned on.
Below API 34 the service and its activities are disabled
(`@bool/passkey_provider_enabled`), the settings button is hidden and
nothing else changes (minSdk stays 26). Passkeys only; passwords keep using
the AutofillService (#475).

**One WebAuthn implementation.** Creating and signing run in
`gitpass_core::webauthn`, the same code the extension host and desktop agent
use (moved out of `gitpass-host`, which now only maps its JSON contract):
ES256 keys, `fmt: none` attestation, sign counter, one passkey per account per
site, the same passkey items in the vault. Bridge API level 7
(`Vault.passkey_entries/register/assert`, `passkey_request_info`,
`passkey_app_rp_host`, `passkey_verify_caller`); level 6 is reserved for
device pairing (#457 / PR #471).

**Who may use which passkey.** The caller comes from Android's
`CallingAppInfo`, never from the request:
- A browser on Google's privileged-apps allowlist (bundled as
  `res/raw/privileged_browsers.json`) gives a web origin via
  `CallingAppInfo.getOrigin(allowlist)`; core then applies the WebAuthn rule
  (https only, the rpId is the origin's host or a registrable parent of it;
  `example.com.evil.io` never gets `example.com`). If the browser supplies
  `clientDataHash`, gitpass signs over it.
- Any other app: package + SHA-256 of its single signing certificate. The
  rpId must be a real domain (no public suffix, IP or port), and
  `https://<rpId>/.well-known/assetlinks.json` (HTTPS, no redirects, 5 s
  timeouts, 512 KB cap, cached 5 min) must grant that package and
  certificate `delegate_permission/common.get_login_creds`. clientDataJSON then
  uses `android:apk-key-hash:<base64url(sha256)>` as its origin. No DAL file,
  or no match, means no passkey. `include` statements are not followed.
- An app that claims a web origin without being on the allowlist, apps
  with several signers, and gitpass itself are refused.
The checks run in the begin step (gitpass is not offered otherwise) and
again in the activity that creates or signs.

**User verification before any secret operation.**
- Create: the "gitpass" entry opens `PasskeyCreateActivity`, which asks for
  biometric unlock (#473) or Master Password + Secret Key, creates the
  passkey in the selected vault, and locks again if it unlocked.
- Sign in, vault locked: Credential Manager shows **Unlock gitpass**. After the
  unlock, the matching passkeys (names only) are listed and the vault is
  locked again at once (the app also locks whenever it leaves the
  foreground). Picking one asks for verification again before it signs, so a
  locked sign-in takes two prompts (two biometric touches when biometric
  unlock is on).
- Sign in, vault already open: the passkeys are listed directly, and picking
  one asks for a fresh verification (biometric or Master Password, checked
  without disturbing the open session), because gitpass reports user
  verification (UV) in every assertion.
- The activities block screenshots and lock a vault they unlocked when they
  finish or go to the background.

**Tests.** Rust: `crates/gitpass-core/tests/webauthn.rs` (rpId rules, Digital
Asset Links, request parsing, create/sign with built and supplied client
data, cross-RP refusal) and `crates/gitpass-mobile-ffi/tests/passkey.rs`. JVM:
`CallerIdentityTest`, `HttpsAssetLinksTest`, `PasskeyBrokerTest`,
`PasskeySessionTest`. Instrumented (CI API 35
emulator): `PasskeyBridgeTest` (real native create/list/sign on a v5 fixture
copy) and `PasskeyProviderUiTest` (Android 14+: framework request extras;
nothing before verification, refusals before any unlock, the unlock action
lists then relocks, fresh verification on an open vault).
The full system flow (choosing gitpass in Credential Manager, a real site or
app) needs an Android 14+ device.
