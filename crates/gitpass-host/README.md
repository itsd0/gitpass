# gitpass-host + browser extension (0.6)

`gitpass-host` is the native messaging host `com.gitpass.host`. Keys never live in the browser.

```
extension <-native messaging-> gitpass-host <-UNIX socket-> gitpass desktop app (holds the unlocked Session)
```

## Modes
**App mode (normal).** On start (and lazily before each request) the host connects to the desktop app's agent
socket — macOS `~/Library/Application Support/gitpass/agent.sock`, Linux `$XDG_RUNTIME_DIR/gitpass/agent.sock`
(overrides: `GITPASS_AGENT_SOCK`, `GITPASS_AGENT_TOKEN`). The socket directory must be owned by the user and not
group/other accessible. The host sends `hello {client:"gitpass-host", version, token?}`; `token` is the content of
`agent.token` next to the socket if that file exists. Every command except `generate` and `unlock` is proxied
unchanged (the host rewrites `id` to its own numeric id on the socket and restores the extension's id in the
reply), so unlocking the app (password or Touch ID) unlocks the extension. App events `{"event":"lock"|"unlock"}`
are forwarded to the extension as-is. If the app connection drops the host sends
`{"event":"lock","reason":"app_disconnected"}` and falls back to standalone mode. Timeouts: `request_unlock` 125 s,
other commands 30 s. Connecting to the app drops any standalone session.

**Standalone (fallback).** If the socket is missing, refuses `hello`, or the app is not running, the host answers
the same commands itself with gitpass-core: `unlock {password, secret_key?}` (Secret Key from the popup, or the
optional file `secret_key_path`, default `<vault>/secret.key`), session in memory only, idle auto-lock after
`idle_secs` (default 300; `status`/`generate` don't count). `status` reports `app_running:false, mode:"standalone"`;
`request_unlock` returns `app_not_running`; `open_in_app` runs `open -b dev.gitpass.desktop`.

## Protocol
Framing: 4-byte **little-endian** u32 length + UTF-8 JSON. Browser side: in/out max 1 MiB (Chrome limit).
Socket side: max 4 MiB. A malformed browser frame ends the process.

Request `{"id": <any>, "cmd": "<name>", ...}` → `{"id", "ok": true, "result": {...}}` or
`{"id", "ok": false, "error": "<code>", "message": "..."}` with codes `locked`, `denied`, `not_found`,
`origin_mismatch`, `bad_request`, `app_not_running`, `internal`.

Commands (see the v0.6 contract): `hello`, `status`, `request_unlock`, `lock`, `list_items {vault_id?, category?,
query?, limit?}`, `match {origin}`, `get_item {item_id}`, `reveal {item_id, field, source:"popup"}`,
`fill {item_id, origin}` → `{username, password, totp?}`, `fill_card` / `fill_identity {item_id}` (needs
`source:"popup"` or `source:"inpage_click"`) → `{<field id>: value}`, `totp` → `{code, remaining, period}`,
`save_login {origin, title?, username, password, vault_id?}` → `{item_id}`, `update_password`, `item_findings {item_id}`,
`record_generated_password {password, source?, origin?}` → `{ok: true, recorded: bool}` (skips recording and returns `recorded: false` when `settings.rememberGeneratedPasswords` in `.gitpass-settings.json` is false), `site_icon {host}`,
`generate {len, digits, symbols}`, `list_vaults`, `open_in_app {item_id?, view?, section?}` (`view`: `"generator_history" | "watchtower" | "settings"`, `item_id` wins). Standalone-only: `unlock`; 0.5 compat: `search`,
`match {url}`.

ItemSummary: `{id:"<vault>:<item>", vault_id, vault_name, category, title, subtitle, url?, favicon_host?, has_totp,
favorite, updated_at}`; `category` is the real v5 type (`login|secure_note|credit_card|identity|api_credential|
document|ssh_key|wifi`). ItemDetail adds `fields:[{name,label,value?,concealed,kind}]`, `notes?`, `tags`,
`created_at`; secret fields (concealed / TOTP) never carry a value — use `reveal`. Archived and deleted items are hidden.

**Origin checks in both modes:** `match`, `fill` and `save_login` origins must be https (or http on loopback) or
the host replies `origin_mismatch` without contacting the app.

## macOS signing
The desktop app verifies the peer's code signature (certificate "gitpass Local Signing", identifier
`dev.gitpass.host`). After `cargo build --release -p gitpass-host` run `crates/gitpass-host/sign-macos.sh
[binary]` (uses `codesign --identifier dev.gitpass.host`; override the cert with `GITPASS_SIGNING_IDENTITY`).

## Origin matching (`src/origin.rs`)
1. Requesting origin must be `https://`, or `http://` on loopback (`localhost`, `*.localhost`, `127.0.0.1`, `[::1]`).
2. Item URL parsed with the WHATWG `url` crate (IDNA → punycode); a bare `github.com` means `https://github.com`.
3. Schemes must be equal.
4. IP / loopback hosts: exact host **and** port match.
5. Otherwise eTLD+1 via the Public Suffix List (`psl` crate, builds on Rust 1.85.1) must be equal. If a host is
   itself a public suffix (e.g. `github.io`), exact host match is required.

So `paypal.com.evil.io` (eTLD+1 `evil.io`), `paypal.com-login.io`, `paypa1.com`, homoglyph `pаypal.com`
never match `paypal.com`; `alice.github.io` ≠ `bob.github.io`. Trade-off: any subdomain of the same site matches.

## Caller validation
The browser enforces `allowed_origins` (Chromium) / `allowed_extensions` (Firefox) from the manifest. As defence
in depth the host also checks argv (Chromium: argv[1] `chrome-extension://<id>/`; Firefox: argv[2] extension id)
against `allowed_callers` in `~/.config/gitpass/host.json` (override path: `GITPASS_HOST_CONFIG`) and refuses otherwise.

## Install
```
cargo build --release -p gitpass-host
# Chromium-family: load extension/ unpacked (chrome://extensions → Developer mode), copy its ID, then:
target/release/gitpass-host install --browser chrome --extension-id <32-char id> [--vault ~/.gitpass] [--idle-secs 300]
# Firefox (id fixed in manifest): about:debugging → Load Temporary Add-on → extension/manifest.json
target/release/gitpass-host install --browser firefox --extension-id gitpass@gitpass.dev
```
`print-manifest` prints the JSON without writing. Locations (per user):

| Browser | Linux | macOS | Windows (HKCU registry key → JSON in %LOCALAPPDATA%\gitpass\<browser>\) |
|---|---|---|---|
| chrome | ~/.config/google-chrome/NativeMessagingHosts/ | ~/Library/Application Support/Google/Chrome/NativeMessagingHosts/ | Software\Google\Chrome\NativeMessagingHosts\com.gitpass.host |
| chromium | ~/.config/chromium/NativeMessagingHosts/ | ~/Library/Application Support/Chromium/NativeMessagingHosts/ | Software\Chromium\NativeMessagingHosts\com.gitpass.host |
| brave | ~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/ | ~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/ | Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.gitpass.host |
| firefox | ~/.mozilla/native-messaging-hosts/ | ~/Library/Application Support/Mozilla/NativeMessagingHosts/ | Software\Mozilla\NativeMessagingHosts\com.gitpass.host |

## Extension (`extension/`, MV3, plain JS, no build step)
- `background.js`: sole user of `connectNative('com.gitpass.host')`. Uses the browser-provided `sender` origin, never one in the message; content-script requests accepted only from `frameId 0`.
- `content.js`: top frame only, https/localhost only; adds a 🔑 icon to visible password fields; fills only after a trusted click; re-checks `location.origin` before writing values.
- `popup/`: unlock, site matches + Fill, search, Copy TOTP, generator.
- Firefox: `browser_specific_settings.gecko` + `background.scripts` (Chrome uses `service_worker`).
- `npm test` (node:test policy tests), `npm run check` (syntax + manifest refs).

## Passkeys (contract v0.7, feat/keys)

Both need explicit user approval: `source:"popup", approved:true` (extension approval window) or, via the
desktop broker, the app's own confirm dialog. `origin` is the browser-verified top-frame origin; `rp_id` must
equal its host or be a registrable suffix (public suffixes refused). ES256 only.

- `passkey_create {origin, rp_id?, rp_name?, challenge, user_id, user_name, user_display_name?, exclude_credentials?, algorithms?, vault_id?}`
  → `{item_id, rp_id, credential_id, client_data_json, attestation_object, authenticator_data, public_key (SPKI), public_key_algorithm:-7, transports}`
- `passkey_query {origin, rp_id?, allow_credentials?}`
  → `{rp_id, matching: [{credential_id, user_name, user_handle, vault_name, last_used}]}`
- `passkey_get {origin, rp_id?, challenge, allow_credentials?, credential_id?}`
  → `{item_id, credential_id, client_data_json, authenticator_data, signature (DER), user_handle, sign_count}`

All binary values are base64url (no padding). Errors: `denied`, `locked`, `origin_mismatch`, `exists`,
`not_found`, `not_supported`, `bad_request`.
