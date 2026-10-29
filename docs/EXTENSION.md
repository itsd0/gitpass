# gitpass browser extension — user guide

Chromium on macOS only. This guide covers day-to-day use of the extension
once it is installed (install steps are in the README's
"Browser extension" section). Firefox is out of scope here.

## Pairing with the desktop app / native host

The extension does not hold your vault. It talks over native messaging to
`gitpass-host`, which talks to the desktop app's vault. If the app or the
host is not running, the popup and menus still open, but fills are
unavailable and the right-click menu offers only "Open gitpass".

## Unlocking, and when the Secret Key is asked for again

Open the popup and enter your master password. If your vault has no
`<vault>/secret.key`, you are also asked for your Secret Key.

Your password and Secret Key go only to the background worker, which passes
them over native messaging to `gitpass-host`. Content scripts in web pages
never see them. The host uses the Secret Key once to derive the session
keys and then zeroizes it. Nothing is written to disk, and the extension
does not store it.

You are asked again after any of these: you lock the vault, the idle
auto-lock fires, or the browser restarts. Unlocking from the extension
does not write to the vault repo.

## The popup

Open it from the toolbar icon, or with the keyboard command below.

- **Search.** Type to filter. The list shows suggested matches for the
  current site first, then everything else. Arrow keys move, Enter fills
  the selected login, ⌘C copies its password, Esc clears the search.
  Press `?` in the popup for the full shortcut list.
- **Copy.** Copy buttons copy a password (or another field) and clear the
  clipboard after 30 seconds.
- **TOTP codes.** Items with a one-time code show a live code chip in the
  list; the code counts down to its expiry and refreshes automatically.
- **New item.** Create a login (and other item types) from the popup
  without opening the desktop app.
- **Generator.** Generate a strong password from the popup and use it in
  the page or when creating an item.

When the vault is locked, the popup shows the unlock screen instead of
your items.

## Filling from a page: the inline menu

When you focus a username or password field, gitpass shows its own
in-page menu (in closed shadow DOM, so the page cannot read it) with the
logins matching that site, plus a way to generate a password.

- **Open it:** ↓ (Down arrow) on an empty field, or ⌥↓ (Option + Down)
  on a field that already has text.
- **Move:** ↑ / ↓.
- **Fill:** Enter on the highlighted login.
- **Close:** Esc (or Tab).

Fills are always addressed to the frame you are in. The content script
runs in the top frame only, so a right-click fill inside a subframe
deliberately does nothing rather than risk putting credentials into a
different frame's form.

## The right-click menu

Right-click in a page or an editable field for the gitpass menu:

- **Vault locked:** a single "Unlock gitpass…" entry.
- **Vault unlocked:** a "gitpass" parent with:
  - "Fill login ›" — up to 5 logins matching the tab's site, then
    "Search gitpass…" (opens the popup);
  - "Suggest strong password" — fills a generated password into the
    field; submitting the form then triggers the usual save banner;
  - "Fill card ›" / "Fill identity ›" — only shown when the vault has
    card or identity items;
  - "Open gitpass" — opens the popup.
- **App or host not reachable:** the parent plus "Open gitpass" only, so
  no fill entry is offered that could not work.

Menu titles show item titles and usernames only — never a password, card
number, or other field value.

## Keyboard commands

| Action | macOS | Other platforms |
| --- | --- | --- |
| Open the gitpass popup | ⌘⇧X | Ctrl+Shift+X |
| Fill the best matching login | ⌘⇧L | Ctrl+Shift+L |

Change them at `chrome://extensions/shortcuts`. (The popup command is
Chrome's built-in `_execute_action`; the fill command is `fill-login`.)

## Save / update banner and "Never for this site"

When you submit a login form with credentials gitpass does not know, a
banner appears in the page offering to save them. If the site already has
a login and the password changed, the banner offers to update it instead
(or save the new credentials as a separate item). You can edit the title
and pick the vault before saving.

- **Save / Update:** stores the credentials in the vault.
- **Save as new:** (update case only) keeps the old item and adds a new one.
- **Not now:** dismisses this once; gitpass may ask again later.
- **Never for this site:** gitpass will not offer to save or update
  passwords on that site again.

Manage the exceptions on the extension options page, under
"Never save on these sites": remove a site there to be asked again. The
options page also has the "Offer to save logins" switch, which turns the
banner off entirely.

## Passkeys

When a site asks for a passkey and gitpass has one for that site, an
in-page prompt lets you pick it. If you instead want the browser's own
passkey flow, choose the native/device option in that prompt. If gitpass
has no passkey for the site, no prompt appears at all and the request
passes straight through to the browser's native flow.

## What the extension never stores

- Your master password and Secret Key (asked again after lock, idle
  auto-lock, or browser restart, as above).
- Full card numbers or other field values in menus or banners — only
  titles, usernames, and masked numbers are ever shown.
- Page data in page storage: the extension keeps its working state in
  its own memory, never in a page's `sessionStorage`/`localStorage`,
  which the page could read.

## Source map (keep this table in sync with the code)

| Behaviour in this guide | Source |
| --- | --- |
| Keyboard commands | `extension/manifest.json` (`commands`: `_execute_action`, `fill-login`) |
| Right-click menu contents | `extension/bg-context.js` (`buildMenu`, issue #213) |
| Inline menu keys | `extension/content.js` (field `keydown` handler, dropdown keys) |
| Save / update banner, "Never for this site" | `extension/content.js` (save/update banner), `extension/options/options.html` |
| Passkey prompt and native fallback | `extension/bg-passkey.js` |
| Popup search, copy, TOTP, New item, generator | `extension/popup/popup.js`, `extension/popup/generator.js`, `extension/popup/newitem.js` |
| Unlocking and Secret Key handling | README "Browser extension" section, `crates/gitpass-host/README.md` |
