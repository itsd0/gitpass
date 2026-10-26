# gitpass extension v0.6: English / Burmese (my) / Thai (th) i18n review checklist

## Mechanics
- [ ] `_locales/en|my|th/messages.json` have identical key sets (`npm run check` enforces this).
- [ ] Every user-visible string in popup, content.js, welcome, options uses `data-i18n*` or `GitpassI18n.t(key, englishFallback)`; no hard-coded English left.
- [ ] Placeholders use `$1`-style `placeholders` in messages.json (exception: `stepsLeft` uses a literal `{n}` token replaced in JS; keep `{n}` in translations), not string concatenation that fixes word order.
- [ ] Plurals: "1 step left" vs "N steps left" are separate keys (`oneStepLeft`, `stepsLeft`). Burmese and Thai don't inflect for plural, but keep both keys.
- [ ] Digits: decide whether to show Burmese digits (၀-၉) or Thai digits (๐-๙). Recommendation: Western digits for counts, codes, TOTP and card last-4.
- [ ] Brand: "gitpass" stays lowercase Latin in every locale. Never translate "Touch ID", "Secret Key" (or translate consistently with the desktop app), "TOTP", "⌘⇧X".
- [ ] Matches the desktop app's glossary for: vault, item, login, unlock, lock, master password, Secret Key, Emergency Kit, autofill.

## Layout / fonts
- [ ] Font stack includes "Noto Sans Myanmar" / "Noto Sans Thai" (welcome.css does; check popup.css and the in-page shadow CSS).
- [ ] Burmese text is ~30-60% longer and taller (stacked diacritics). Check: popup 720x480 header, buttons, the in-page dropdown rows (title and subtitle truncate with ellipsis, no clipped vowels), the save banner, and the welcome checklist pill.
- [ ] Line-height ≥ 1.5 for my/th (tone marks and medials must not be clipped by `overflow:hidden`).
- [ ] Thai has no spaces between words, so allow wrapping (`word-break: normal; overflow-wrap: anywhere` only where needed). Check long buttons.
- [ ] Burmese is Unicode, not Zawgyi. Reject any Zawgyi-encoded strings; test on a machine with a Unicode Myanmar font.

## Per-surface strings to review (en → my, th)
- Welcome: welcomeTitle, welcomeSub, stepConnect(+Body), stepPin(+Body), pinDone, stepTry(+Body), demoSignIn, retry, openApp, checking, hostMissing, hostMissingHelp, thenRetry, appNotRunning, appNotRunningHelp, connectedUnlocked, connectedLocked, tryDone, stepsLeft, oneStepLeft, allSet, openSettings.
- Options: settingsTitle, optAutolock(+Body), optAutofill(+Body), optSave(+Body), optShortcut(+Body), change.
- Locked view: "Unlock with Touch ID", "Unlock in gitpass", "Use password", "Open gitpass app to unlock", "Unlock with password".
- Popup: search placeholder, filter names (Suggestions for this site / All Items / Favorites / categories: login, secure_note, credit_card, identity, api_credential, document, ssh_key, wifi), Copy / Copied, Reveal, "Open & Fill", ⋯ menu (Open in app, Copy link, Favorite), generator labels.
- In-page: badge title, dropdown empty and locked states, "Suggest strong password", save banner ("Save login to gitpass?", Save / Update / Not now / Never for this site).
- Errors: locked, denied, not_found, origin_mismatch, app_not_running. These are shown to users and must be friendly sentences, not codes.

## Native-speaker sign-off
- [ ] my reviewer: ____  date: ____
- [ ] th reviewer: ____  date: ____
- [ ] Screenshots of each surface in each locale attached (Chrome `--lang=my` / `--lang=th`, Firefox `intl.locale.requested`).
