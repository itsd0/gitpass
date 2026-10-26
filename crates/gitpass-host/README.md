# gitpass-host + browser extension (0.4.0)

Keys never live in the browser. `gitpass-host` (native messaging host `com.gitpass.host`) unlocks the
vault with gitpass-core and holds the `Session` in memory; it auto-locks after `idle_secs` (default 300) of
no vault activity (`status`/`generate` don't count). The browser only gets metadata, and username+password via `fill`.

## Protocol
Framing: 4-byte **little-endian** u32 length + UTF-8 JSON. Inbound max 1 MiB; outbound max 1 MiB (Chrome limit).
A malformed frame ends the process (the browser sees a disconnect).

Request `{"id": <any>, "cmd": "<name>", ...}` → `{"id": <same>, "ok": true, "result": {...}}` or `{"id": <same>, "ok": false, "error": "..."}`.

| cmd | args | result |
|---|---|---|
| status | – | `{unlocked, version, idle_timeout_secs}` |
| unlock | `password` | `{unlocked:true}` (Secret Key read from `secret_key_path`, default `<vault>/secret.key`) |
| lock | – | `{unlocked:false}` |
| match | `url` (page origin) | `{items:[{id,title,username,has_totp}]}` – no passwords |
| search | `query` | `{items:[{id,title,username,url,has_totp}]}` (≤50) – no passwords |
| fill | `item_id`, `origin` | `{username,password}` only if origin matches the item URL |
| totp | `item_id` | `{code, remaining}` |
| generate | `len` (8..128, default 20) | `{password}` |

`item_id` = `<vault_id>:<item_id>`. Any command but status/generate/unlock returns `"locked"` when locked.

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
