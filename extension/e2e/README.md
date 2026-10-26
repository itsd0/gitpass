# gitpass extension e2e

```
cd extension
npm install
npx playwright install chromium   # Playwright's Chromium; branded Chrome no longer loads unpacked extensions via flags
npm run e2e                       # headless
HEADED=1 npm run e2e              # watch it
KEEP_TMP=1 E2E_TIMEOUT_MS=10000 npm run e2e   # keep temp dir, longer waits
```

What it does (nothing outside a fresh `$TMPDIR/gitpass-e2e-*` is touched):
1. Copies `extension/` (minus node_modules/e2e/dist) to a temp dir and computes its unpacked extension id.
2. Registers `e2e/mock-host.js` as `com.gitpass.host` in `<tmp>/profile/NativeMessagingHosts/` (per user-data-dir lookup), so the real host manifest is never used.
3. Serves a test login page on `http://localhost:<random>/login`.
4. Launches Chromium with `--load-extension`, then checks: host reachable, welcome tab on install, real action popup (opened via `chrome.action.openPopup()` and driven over CDP, so `sender.tab` is unset like a real click) shows the locked view, the `{"event":"unlock"}` push updates the open popup with no password, items are listed, in-page dropdown fill (closed shadow DOM is pierced via CDP `DOM.getDocument {pierce:true}`), popup Fill, save banner after submit → `save_login`, options page.

Mock host: starts locked; unlocks on `request_unlock`/`unlock`, or when the test writes `control.json` (simulating unlock in the desktop app); logs every message to `<tmp>/mock/log.jsonl`. Fake items: "Localhost Test Login" (alice@example.com / Corr3ct-H0rse-Battery, matches localhost), GitHub, a Visa card, a secure note.

Exit code 0 only if every non-INFO check passes. Results are also written to `<tmp>/results.json`.
Selectors are text-based (item titles, /unlock/i, /Fill|Open & Fill/, /^Save/) so they survive popup rewrites.
