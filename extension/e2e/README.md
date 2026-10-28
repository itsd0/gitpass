# gitpass extension e2e

```
cd extension
npm install
npx playwright install chromium   # Playwright's Chromium; branded Chrome no longer loads unpacked extensions via flags
npm run e2e                       # headless, mock host
HEADED=1 npm run e2e              # watch it
KEEP_TMP=1 E2E_TIMEOUT_MS=10000 npm run e2e   # keep temp dir, longer waits
```

What it does (nothing outside a fresh `$TMPDIR/gitpass-e2e-*` is touched):
1. Copies `extension/` (minus node_modules/e2e/dist) to a temp dir and computes its unpacked extension id.
2. Registers `e2e/mock-host.js` as `com.gitpass.host` in `<tmp>/profile/NativeMessagingHosts/` (per user-data-dir lookup), so the real host manifest is never used.
3. Serves a test login page on `http://localhost:<random>/login` and a WebAuthn relying party on `/webauthn`.
4. Launches Chromium with `--load-extension`, then checks: host reachable, welcome tab on install, real action popup (opened via `chrome.action.openPopup()` and driven over CDP, so `sender.tab` is unset like a real click) shows the locked view, the `{"event":"unlock"}` push updates the open popup with no password, items are listed, in-page dropdown fill (closed shadow DOM is pierced via CDP `DOM.getDocument {pierce:true}`), popup Fill, save banner after submit → `save_login`, options page.
   With no saved login for the active site (mock host `no_match_hosts`), the Suggested filter shows the empty state
   “Save a login for localhost”; clicking it opens the Login form with the title and URL prefilled, and nothing is
   saved until the user submits.
   The popup list shows the live TOTP code for a login with a secret (mock host returns 123456 / 20 s), and
   clicking the code copies it (the clipboard write is observed in the popup page).
   A two-page login (username page → password page, like x.com) checks exactly one gitpass icon per step and
   that `save_login` carries the step-1 username plus the step-2 password exactly once.
   The checkout card form checks exactly one icon per logical card group, attached to the card number input
   (name/expiry/CVC are not decorated), and that clicking that icon's dropdown fills every card field.
5. Drives the in-page passkey prompt (also a closed shadow DOM, found with CDP): approve create (RP verifies the credential) and get (RP verifies the ES256 assertion, signCount and user handle). Cancelling a create, or a get with no matching gitpass passkey, must fall back to the browser's own WebAuthn (CDP virtual authenticator) and must not open a gitpass window. A duplicate create reports `InvalidStateError`.

Mock host: starts locked; unlocks on `request_unlock`/`unlock`, or when the test writes `control.json` (simulating unlock in the desktop app); `control.json` can also list `no_match_hosts` (e.g. `["localhost"]`) to simulate a site with no saved login; logs every message to `<tmp>/mock/log.jsonl`. It implements `passkey_query` (saved account names for the prompt) and `passkey_create`/`passkey_get` with a software ES256 authenticator (`passkey-authn.js`). Fake items: "Localhost Test Login" (alice@example.com / Corr3ct-H0rse-Battery, matches localhost), GitHub, a Visa card, a secure note.

Exit code 0 only if every non-INFO check passes. Results are also written to `<tmp>/results.json`.
Selectors are text-based (item titles, /unlock/i, /Fill|Open & Fill/, /^Save/) so they survive popup rewrites.

## Strict passkey test (manual, real host)

`node e2e/passkey-inpage.js` is separate from `npm run e2e`. It drives the in-page prompt against the real
`gitpass-host` in standalone mode on `https://app.hyperbeat.test` (resolved to 127.0.0.1) with strict
WebAuthn shapes (parent-domain rp.id, resident key, user verification). Build the binaries first:

```
(cd .. && cargo build --release -p gitpass-host -p gitpass-cli)
node e2e/passkey-inpage.js
```

It needs `openssl`, Playwright's Chromium, and uses a throwaway vault under `$TMPDIR` (never `~/.gitpass`).
Override `GITPASS_HOST_BIN` / `GITPASS_CLI_BIN` for other binaries, `HEADED=1` to watch, and `KEEP_TMP=1`
to keep the temp directory.
