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

When typing your master password, the input checks modifier state and
displays an inline **Caps Lock is on** hint under the field when Caps Lock is
active, preventing accidental failed attempts from unintentional uppercase input.
The hint appears while the field is focused and hides immediately as soon as
Caps Lock is turned off or focus leaves the field.

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
  current site first, then everything else. In addition to matching title,
  username, and URL text, the search field supports structured query operators:
  - `tag:<name>` (e.g. `tag:work`)
  - `vault:<name>` (e.g. `vault:Personal`)
  - `type:<category>` (`login`, `card`, `note`, `api`, `ssh`, `identity`, `passkey`, `bank`, `wifi`, `server`, `document`)
  - `is:favorite`, `is:archived`
  - `has:otp`, `has:passkey`, `has:attachment`, `has:url`
  - `url:<host>` (matches against URL host)
  - Leading `-` negates any operator (e.g. `-tag:old`, `-is:archived`)

  For example: `tag:dev is:favorite`, `type:login has:otp`, or
  `-is:archived github.com`. For the complete operator list and syntax, see
  [docs/CLI.md](CLI.md).
  Arrow keys move, Enter fills the selected login, ⌘C copies its password,
  Esc clears the search. Press `?` in the popup for the full shortcut list.
- **Browsing by tag.** When the vault has tagged items, a **Tags** chip appears
  in the filter row. Clicking it opens a tag menu listing every tag sorted
  alphabetically with item counts, plus a search input to narrow the tags.
  Picking a tag filters the list to items with that tag and pins an active tag
  chip (`tag:<name> ×`) in the bar (click it to clear). In item detail, tags
  are displayed as clickable tag pills; clicking a pill filters the list by
  that tag.
- **Archive.** When items are archived, an **Archive** filter chip appears in
  the popup filter row. Archived items remain readable; viewing an archived
  item shows an "item is archived" banner and an **Unarchive** button (with an
  Undo toast). You can archive or unarchive items via the More (`⋯`) menu.
- **Recently Deleted.** The detail's More menu moves an item to Recently
  Deleted — the same 30-day trash the desktop app shows. When the trash is
  non-empty, a **Recently Deleted** chip appears in the filter row. Items in
  Recently Deleted are read-only: copying fields is disabled, autofill is
  unavailable, and the item cannot be edited. The row subtitle shows when the
  item was deleted, and the detail view displays a banner with the days
  remaining until automatic purge (highlighted in red when 3 days or fewer
  remain) alongside a **Restore** button (or ⌘⌥R / Ctrl+Alt+R) with an Undo
  toast.
- **Copy.** Copy buttons copy a password (or another field) and clear the
  clipboard after 30 seconds.
- **TOTP codes.** Items with a one-time code show a live code chip in the
  list; the code counts down to its expiry and refreshes automatically.
- **Large type.** A password or one-time code can be shown in large
  characters, one per cell with index numbers, from the field's large-type
  button or with ⌘⌥L / Ctrl+Alt+L. Esc closes the view and puts focus back
  on the button that opened it; locking the popup clears it.
- **New item.** Create a login (and other item types) from the popup
  without opening the desktop app.
- **Generator.** Generate a strong password, passphrase, or PIN from the popup
  and use it in the page or when creating an item. In **Memorable** passphrase
  mode, you can pick the word separator (`-`, `.`, `_`, or space) using the
  separator buttons or arrow keys, and toggle **Capitalize** to title-case each
  word in the passphrase. Under it, **Recent** lists your last 10 generated
  passwords (masked until you reveal one; copy, reveal, and fill when it was
  generated on the current site). It is hidden while locked and shows "Generator
  history is off" when the desktop setting is off. Passwords generated in the
  extension are saved to the vault's encrypted generator history (the same
  history the desktop app and `gitpass gen history` show), so a password lost to
  a form that swallowed it can be recovered from there instead of from the
  extension. To stop recording, turn off desktop Settings "Remember generated
  passwords" (on by default); to clear what is already recorded, use
  `gitpass gen history clear` (or clear it from the desktop app).
  `gitpass gen --no-history` skips recording for a single CLI generation only.
  Note: when the desktop app is closed, the standalone host currently ignores
  that setting — see issue #225.
- **Opening gitpass.** Actions like "Open in gitpass" and "Open Watchtower" in
  the popup use `open_in_app` to bring the desktop app to the front and navigate
  directly to the item or target view (Watchtower, generator history, or
  Settings). If the app is locked, it shows the unlock screen first and then
  completes the navigation.

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

## Submit after fill

Off by default. On the extension options page, under "Autofill &
saving", the "Submit the form after filling a login" switch
(`autoSubmitAfterFill`) turns it on. When it is on, gitpass submits the
form right after a login fill — but only when all of these hold:

- The fill was one you explicitly triggered: Fill from the popup, a pick
  from the inline menu, the ⌘⇧L fill command, or a right-click fill.
  Nothing ever submits on page load.
- The detected form is a real `<form>` login (or a username-only step of
  one), and the step that was filled is complete: a full login needs its
  username and password filled, a password-only step needs its password,
  a username-only step needs its username. A partially filled form fills
  and stops.
- The form has exactly one visible submit control. Zero, or two or more,
  means no submit.
- The fill origin attested by the background matches the page origin.

Submission uses the form's `requestSubmit()`, so the page's own
validation still runs. It never happens for cards, identities, TOTP,
passkeys, sign-up or change-password forms, and a formless group of
fields never submits.

Per-site exceptions live on the extension options page under "Never
auto-submit on these sites": type a hostname (for example
`example.com`) and choose "Add a site". gitpass still fills logins on
those sites but never submits the form for you; the `www.` prefix is
ignored when matching, so an exception for `example.com` also covers
`www.example.com`. Remove a site with its Remove button to allow
auto-submit there again.

## Per-site settings in the popup

When the popup is open on a web page, a small gear button sits next to
the filter chips. It opens "Site settings" for the current site, with
three toggles:

- **Offer to save logins on this site** — off adds the site to the
  "Never save" list (same list as the banner's "Never for this site").
- **Submit after fill on this site** — shown only when the global
  "Submit the form after filling a login" switch is on; off adds the
  site to the "Never auto-submit" exceptions.
- **Show inline menu on this site** — off hides the inline menu and
  field badges on that site. Hidden sites are listed on the extension
  options page under "Hide the inline menu on these sites", where you
  can add or remove sites directly.

All three match by hostname with the `www.` prefix ignored, and changes
apply the next time the page loads or rescans.

## Website icons and monograms

Login rows in the popup and in the inline autofill menu show the site's
website icon when one is available; otherwise they show a monogram —
the first letter of the item title. Non-login items show their category
icon instead.

The extension itself never fetches an icon over the network. When an
icon is needed, it asks the background worker, which asks `gitpass-host`
over native messaging; the host serves a cached icon only when the
desktop app's "Show website icons" setting is on and the vault has a
login for that host. If icons are off, or no icon is cached for the
site, the monogram is shown instead. Icons the extension receives are
kept in its own memory only and are cleared when the vault locks.

There is no icon toggle in the extension itself: the extension options
page's "Website icons" row only explains that icon downloads are
controlled by the desktop app's "Show website icons" setting. That
desktop setting is off by default, because fetching an icon tells the
site you have an account there; turning it off clears the icon cache.

## Passkeys

When a site asks for a passkey and gitpass has one for that site, an
in-page prompt lets you pick it. When a site has more than one passkey
saved in gitpass (for example multiple accounts on GitHub), the prompt
shows an account list with usernames, vault names (if items belong to
differing vaults), and website icons or monograms. The most recently used
passkey is preselected and listed first.

The account list supports keyboard navigation (`role="listbox"` with
Up/Down, Home/End, Enter to sign in, and Esc to cancel). Focus starts on
the preselected row, and the 600 ms anti-clickjacking arming window
applies to both the Sign in button and Enter key. The chosen credential ID
is strictly validated by the background against authoritative query matches
before invoking `passkey_get`. The popup-window fallback (`popup/passkey.html`)
provides the same account choice.

If you instead want the browser's own passkey flow, choose the native/device
option in that prompt. If gitpass has no passkey for the site, no prompt
appears at all and the request passes straight through to the browser's native flow.

## What the extension never stores

- Your master password and Secret Key (asked again after lock, idle
  auto-lock, or browser restart, as above).
- Full card numbers or other field values in menus or banners — only
  titles, usernames, and masked numbers are ever shown.
- Page data in page storage: the extension keeps its working state in
  its own memory, never in a page's `sessionStorage`/`localStorage`,
  which the page could read.
- Generated-password history: the extension keeps no history of its own.
  History lives in the vault's encrypted generator history (see
  Generator above), not in extension storage.

## Source map (keep this table in sync with the code)

| Behaviour in this guide | Source |
| --- | --- |
| Keyboard commands | `extension/manifest.json` (`commands`: `_execute_action`, `fill-login`) |
| Right-click menu contents | `extension/bg-context.js` (`buildMenu`, issue #213) |
| Inline menu keys | `extension/content/dropdown.js` (field `keydown` handler, dropdown keys) |
| Save / update banner, "Never for this site" | `extension/content/banner.js` (save/update banner), `extension/options/options.html` |
| Submit after fill, "Never auto-submit" exceptions | `extension/content/settings.js` (`maybeAutoSubmit`), `extension/lib/forms.js` (`shouldAutoSubmit`, `visibleSubmitControls`), `extension/options/options.html` |
| Website icons and monograms | `extension/background.js` (`site_icon`), `extension/bg-page.js` (`page:site_icon`), `extension/popup/popup.js`, `extension/content/dropdown.js` (`itemIcon`), `crates/gitpass-host/src/handler.rs` (`site_icon`) |
| Passkey prompt and native fallback | `extension/bg-passkey.js`, `extension/content/prompts.js` |
| Popup search, query operators, and list navigation | `extension/popup/popup.js`, `extension/popup/view.js`, `docs/CLI.md` |
| Tag browsing and tag menu | `extension/popup/view.js` (`collectTags`), `extension/popup/popup.js`, `extension/popup/detail.js` |
| Archive and Recently Deleted views, restore | `extension/popup/detail.js`, `extension/popup/largetype.js`, `extension/popup/view.js` (`formatTrashSubtitle`), `extension/popup/popup.js` |
| Generator: memorable separator, capitalize, and recent history | `extension/popup/generator.js`, `extension/popup/panels.js`, `extension/popup/recent.js` |
| New item creation | `extension/popup/newitem.js` |
| Unlock screen and Caps Lock hint | `extension/popup/capslock.js`, `extension/popup/popup.js`, `crates/gitpass-host/README.md` |
| Per-site popup settings | `extension/popup/sitesettings.js` |
| Generator history recording | `extension/background.js`, `extension/bg-page.js` (`record_generated_password`), `crates/gitpass-host/src/handler.rs`, `apps/desktop/gui-core/src/api.rs` (`record_generated_password`, `rememberGeneratedPasswords`) |
| Unlocking and Secret Key handling | README "Browser extension" section, `crates/gitpass-host/README.md` |
