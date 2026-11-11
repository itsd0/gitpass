# Shared encrypted cross-platform test vault fixtures

One vault directory per format version (`v2`, `v3`, `v4`, `v5`), plus an
`expected.json` next to each one. The same decrypted content is verified
by four consumers, each through its own stack:

- core: `crates/gitpass-core/tests/shared_fixtures.rs`
  (read-only unlock, full semantic dump incl. generator history)
- bridge: `ios/rust/gitpass-ios-ffi/tests/ffi.rs`
  (`open_vault` + `record_json`; generator history is not exposed by the
  bridge, so that section is covered by the core test only)
- Android instrumented: `android/app/src/androidTest/.../SharedFixturesTest.kt`
  (fixtures ship as `androidTest` assets, copied to the app cache)
- iOS XCTest: `ios/Tests/GitpassTests.swift` (`testSharedFixtures`,
  fixtures ship as test-bundle resources)

## Test-only credentials

- Master password: `fixture-password-451`
- Secret Key: see each fixture's `secret.key.txt` (same fixed key in all
  four fixtures)

These are throwaway test-only values. All item content is synthetic
(`FAKE-…`, `example.invalid`); there are no real credentials anywhere in
these fixtures. **Never** use this password or Secret Key for a real vault.

## Content

Two vaults ("Personal", "Work"). "Personal" holds one item per category
(login, credit card, secure note, identity, API credential, document,
SSH key, wifi, passkey, bank account, server) plus:

- a favorite login with TOTP, tags, a 90-day rotation reminder, an extra
  "recovery" section and a related-item link
- a login whose password was rotated once (password history + item history)
- a login edited twice (two history entries)
- an archived login, a soft-deleted ("trashed") login, a login with an
  ignored Watchtower finding
- generator history (two entries)

Differences per version (the format version is the `format.json`
version; the manifest format is content-derived):

| fixture | `format.json` | manifest | extra |
|---------|---------------|----------|-------|
| v2 | 2 | v3 (devices) | — |
| v3 | 3 | v3 (devices) | — |
| v4 | 4 | v4 (identity) | account identity keypair |
| v5 | 5 | v5 (files) | identity + one attachment |

## Regenerating

```sh
cargo run --locked --example gen_shared_fixtures
```

Only the decrypted content (`expected.json`) is reproducible: salts,
nonces, account ids and device ids are random per generation, so the
encrypted bytes always differ and every fixture file is rewritten.
Review the `expected.json` diff — that is the part that must stay
stable unless the content intentionally changed.

CI fails if the generator's output differs from the committed
`expected.json` (the `generator_output_matches_committed` test in
`crates/gitpass-core/tests/shared_fixtures.rs` regenerates into a temp
dir and compares). To check without writing:

```sh
cargo run --locked --example gen_shared_fixtures -- --check
```

## Two deliberate timestamp sentinels

- The trashed item's `deleted_at` is 2100-01-01 (`4102444800`): a
  normal unlock purges soft-deleted items older than 30 days, which
  would delete the trash item from a static fixture within a month.
- The generator-history entries' `created_at` is also 2100-01-01:
  entries older than 90 days are pruned on read.

Both are synthetic test data; `expected.json` never contains the raw
timestamps (the semantic dump reduces deletion to a boolean and drops
history timestamps).

## Rules for tests

- Copy a fixture to a temp dir before opening it. A normal unlock
  writes device state, may re-wrap the keyset to the production KDF
  (which bumps `format.json` to v5 in the copy), and purges expired
  trash. Never open the committed fixture in place.
- Read `format.json`'s version *before* opening when asserting the
  fixture version, for the re-wrap reason above.
- No committed fixture carries device-local state
  (`.gitpass.lock`, `.gitpass-device.json`, `.gitpass-device-key.json`,
  `.gitpass-state.json`, `.gitpass-throttle.json`).
