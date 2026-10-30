# Watchtower: how password health is scored

Watchtower is gitpass's security audit. The CLI (`gitpass audit`) prints the findings; the desktop app
shows the same findings plus a 0–100 score. Everything is offline unless you opt in to the Have I Been
Pwned (HIBP) breach check.

- Checks: `crates/gitpass-core/src/audit/` (the shared score is `score()` in `audit/mod.rs`)
- Desktop score: `watchtower()` in `apps/desktop/gui-core/src/api/app/tools.rs`, with weighting from core `audit::score`
- Score bands and gauge: `apps/desktop/src/features/watchtower/score.ts`
- Offline 2FA site list: `crates/gitpass-core/data/mfa-sites.tsv`

```sh
gitpass audit                      # every vault, default thresholds (365 days)
gitpass audit Personal             # one vault
gitpass audit --max-age-days 180   # stricter "old password" threshold
gitpass audit --expiring-days 60   # warn about expiry dates further ahead (default 30)
gitpass audit --unused-months 24   # flag logins unused for 24 months (default 12; 0 = off)
gitpass audit --hibp               # opt in to the breach check
gitpass audit --include-ignored    # also print findings you chose to ignore
gitpass audit --json               # machine-readable report (counts + per-item findings)
```

| Finding (`kind`) | Applies to | Desktop section | Affects the desktop score |
|---|---|---|---|
| `weak` | items with a password | Weak | yes (`bad`) |
| `reused` | items with a password | Reused | yes (`bad`) |
| `pwned` (opt-in HIBP) | items with a password | Breached | yes (`bad`) |
| `old` | items with a password | Old | yes |
| `missing_totp` | items with a URL and no TOTP secret | Missing 2FA | yes |
| `totp_available` | same, on a site the offline list says supports TOTP | 2FA available | yes (same weight as `missing_totp`) |
| `passkey_available` | items on a passkey-capable site with no stored passkey | Passkeys available | no |
| `expired` / `expiring_soon` | any item with an expiry field (cards, API credentials, …) | Expired / Expiring soon | no |
| `insecure_url` | items whose URL is plain `http://` on a public host | Insecure URLs | no |
| `duplicate` | Login items with the same site and username in one vault | Duplicates | no |
| `weak_ssh_key` | SSH key items with an RSA key under 3072 bits | Weak SSH keys | no |
| `unused` | live Login items not used (or, if never used, not edited or created) for longer than the threshold (default 12 months) | Unused items | no |
| `rotation_due` / `rotation_soon` | Login items with a "Change password every…" reminder (30, 90, 180 or 365 days) | Due for rotation / Rotation due soon | no |

## What gets audited

Every item that is not deleted and not archived, **except Passkey items**: logins, secure notes, credit
cards, identities, API credentials, Wi-Fi, SSH keys, documents and so on. Each check then decides on its
own whether it applies (see the table above), so in practice most findings come from logins. The CLI
audits the selected vault (or all vaults; `items_checked` counts every audited item); the desktop app
audits every vault at once, so reuse is detected across vaults. Passkey items are read (never reported)
to know which sites already have a stored passkey.

Password findings (`Weak`, `Reused`, `Pwned`, `Old`) only apply to items with a non-empty password.
`MissingTotp` / `TotpAvailable` / `PasskeyAvailable` and `InsecureUrl` only apply to items with a
non-empty URL; `Duplicate` only to Login items.
Findings carry titles and counts only — **passwords, password hashes and TOTP secrets never appear in
reports, logs or the UI**.

## Weak password checks

`contains_personal_info()` and `weakness()` apply the rules in this order and report the first one that matches:

| # | Rule | Default |
|---|------|---------|
| 1 | Contains the item's username (or email local part) or registrable site name from the URL (at least 4 characters; case-insensitive, ignoring common separators and leetspeak substitutions: `0→o`, `1→l/i`, `3→e`, `4/@→a`, `5/$→s`). | — |
| 2 | Contains a bundled common password as a substring (case-insensitive; e.g. `password`, `qwerty`, `master`). | 18-entry list |
| 3 | At least 4 characters but at most 2 distinct characters (`aaaa`, `abab`). | — |
| 4 | Shorter than `min_length` characters (Unicode scalar values, not bytes). | 12 |
| 5 | Estimated entropy below `min_entropy_bits`. | 60 bits |

The rules are substring/character based on purpose: they are cheap, deterministic and explainable.
Rule 1 matches personal information (username and site name) even inside longer passwords like `github-alice-2024!` or `aliceGithub#99`.
Rule 2 matches *inside* a longer password, so `MyPassword2024!!` is still flagged.

## Entropy estimate

`entropy_bits()` is a deliberately naive estimate:

```
pool  = 26·(has lowercase) + 26·(has uppercase) + 10·(has digit)
      + 33·(has other ASCII: punctuation, space, control) + 100·(has non-ASCII)
bits  = number_of_characters × log2(pool)
```

Each character class contributes its pool size once, no matter how many characters of that class are
used. Consequences:

- `""` has no pool and **0 bits** (the audit skips empty passwords; the estimate exists for direct callers).
- A 20-character mixed password (`aA1!`-style) is estimated at `20 × log2(95) ≈ 131` bits.
- Non-ASCII text uses one 100-character pool, counted per character, so 12 Cyrillic letters are
  estimated at `12 × log2(100) ≈ 80` bits. The estimate makes no assumptions about the actual alphabet,
  which overrates non-Latin passphrases and underrates 12-character emoji-only secrets.
- The estimate is linear in length, so a 10 000-character password is finite and very large
  (`≈ 47 000` bits for lowercase letters). There is no length cap and no integer overflow (floating point).
- Repetition is only caught by rule 2 of the weak checks (≤ 2 distinct characters); `aaaaaaaaaaaa`
  otherwise scores `12 × log2(26) ≈ 56` bits, below the 60-bit default.

This is not zxcvbn: there is no dictionary attack, keyboard-pattern or l33t analysis.

## Reuse detection

- Passwords are grouped by `blake3(password)`. Hashes are computed in memory for the duration of one
  audit and are never written, cached or printed.
- Grouping spans **all audited items**, including items from different vaults. Two logins with the same
  password in `Personal` and `Work` form one group.
- Each item in a group of `n` gets `Reused { group, others: n − 1 }`; the report also counts groups
  (`reused_groups`).
- Empty passwords (`Some("")` or no password) are never grouped, so several password-less logins do not
  look like reuse.
- Group numbers are assigned in password-hash order. They are stable for a given item set but arbitrary;
  only group membership is meaningful. The desktop UI renumbers groups 1…n in display order.
- Matching is exact: `Summer2025!` and `Summer2025?` are different passwords, and similar-looking values
  are not compared. Changing one occurrence of a reused password leaves the others flagged until each
  item is updated.

## Old passwords

An item is `Old { days }` when its password has not been changed for more than `max_age_days`
(default 365; CLI `--max-age-days`). The password-change time is computed from the newest history entry
or edit where the password value differed, or the item's creation time if it has never changed.
Editing notes, tags, URLs, favourites or other metadata does not reset the password age; only changing the
password itself resets it. The comparison is strict, so an item whose password is exactly 365 days old
is not reported. Only items with a password are checked.

## Missing TOTP and offline 2FA advice

- `MissingTotp`: the item has a non-empty URL, no TOTP secret, **and** the offline list has no TOTP
  advice for the site. This is independent of whether a password is stored.
- `TotpAvailable { site }`: the URL's host is in the bundled offline list and the site documents TOTP.
  When this advice is given the generic `MissingTotp` finding is suppressed, so each item is reported
  once: the desktop shows it under "2FA available" instead of "Missing 2FA". The score still treats it
  as a missing second factor (see the missing-TOTP term below).
- `PasskeyAvailable { site }`: the site documents passkeys and no stored passkey covers it. Suppression
  compares the URL host, the matched site and every stored `rp_id` in both directions, so a passkey
  registered at `accounts.google.com` also suppresses the advice for `google.com` and `mail.google.com`.
- Host matching lowercases, drops `www.`, scheme, userinfo, port and path. Lookalikes never match
  (`example.test.evil.invalid` does not match `example.test`), and when several listed domains match a
  host the most specific (longest) one wins — `accounts.example.test` over `example.test`.
- The list never touches the network; it is a snapshot of documented support and may lag behind a site.

Neither availability finding is shown twice: an item on a TOTP-capable site gets `TotpAvailable`
instead of `MissingTotp`, and a stored passkey removes `PasskeyAvailable`. `TotpAvailable` feeds the
score's missing-2FA term; `PasskeyAvailable` never affects the score.

## Optional HIBP breach check

`--hibp` (CLI) or the desktop setting *Check passwords against known breaches* enables a
k-anonymity lookup against `api.pwnedpasswords.com`:

- SHA-1 of the password is computed locally; only the **first 5 hex characters** of the hash are sent,
  together with `Add-Padding: true`.
- The response is matched locally; padding rows with count `0` are ignored. The full hash and the
  password never leave the process.
- Results are cached per 5-character prefix for the rest of the audit.
- The desktop performs the check only when the call opts in **and** the persisted setting is on
  (default: off). Without it, breach findings are empty and the report says `hibpChecked: false`.

## Expired and expiring soon

- **Which items:** any audited item with a field whose id is `expiry` / `expires`, or whose id or label
  contains `expir` (case-insensitive) and that has a non-empty value. In practice: a credit card's
  `expiry` and an API credential's `expires`, plus custom fields such as "Passport expires". Removed
  fields are skipped. Each such field is checked on its own.
- **Date formats** (`parse_expiry()`), interpreted in UTC:
  - Month and year — `MM/YY`, `MM/YYYY`, `MM-YY`, `MM-YYYY`, `YYYY/MM`, `YYYY-MM` (two-digit years are
    20YY). The item expires at the **end of that month**: `04/26` is valid until 2026-04-30 23:59:59.
  - Day — `YYYY-MM-DD` or `YYYY/MM/DD`, valid until the end of that day (23:59:59).
  - ISO 8601 date-time (`2026-05-10T12:00:00Z`, 19+ characters) — that exact moment.
  - Unix seconds (9–12 digits).
  - Anything else (free text, `May 2026`) is ignored rather than guessed.
- `Expired { on }` when the expiry moment has passed. Otherwise `ExpiringSoon { on, days }` when the
  whole days left are **≤ `expiring_days`** (default **30**; CLI `--expiring-days`; the desktop uses the
  default). `on` is the field value exactly as stored.
- Expiry findings are listed in the desktop but **do not change the score**.

## Insecure URLs

`InsecureUrl { url_host }`: the item's URL starts with `http://` (any case) and points to a public host.
Not flagged: `https://` and other schemes, `localhost` / `*.localhost`, `local` / `*.local`, loopback
addresses (`127.0.0.0/8`, `::1`) and private IPv4 ranges (`10/8`, `172.16/12`, `192.168/16`). The finding
carries only the lowercased host: userinfo, password, port, path, query and fragment are stripped, so
nothing secret from the URL ends up in a report. It is shown in the desktop but **does not change the
score**.

## Weak SSH keys

`weak_ssh_key`: an SSH key item using an RSA key of under 3072 bits (e.g. 2048-bit RSA). While gitpass
accepts RSA keys of 2048+ bits on import, modern recommendations (such as NIST SP 800-57) favor at least
3072-bit RSA or modern Ed25519 keys for security. It is shown in the desktop under "Weak SSH keys" but
**does not change the score**.

## Duplicates

`Duplicate { group }`: two or more **Login** items in the **same vault** with the same site and the same
username. The site is the URL's host (lowercased, scheme / userinfo / port / path / leading `www.`
dropped, so `https://www.example.com/login` and `http://example.com:8080` match) and the username is
compared trimmed and case-insensitively. Items without a username or without a URL are never grouped,
and items in different vaults are not duplicates of each other. The group label is `<site>/<username>`.

The desktop lists each group with an **Archive older** action: it keeps the most recently updated item
and archives the rest (with Undo). Archived items are no longer audited. Duplicates **do not change the
score** (identical passwords in a group are already counted by `Reused`).

## Unused items

`Unused { days }`: a live **Login** item whose last use on this device is older than the threshold.
It is housekeeping advice to help you archive logins you no longer need; it **never changes the score**.

- **Last use comes from local state.** The desktop app, the extension host and the CLI record a use
  (reveal, copy, fill, TOTP, passkey sign-in — not merely viewing the item) in `.gitpass-state.json`,
  which is per device and never synced. When the item has never been used on this device, its last edit
  time (`updated_at`) is used instead, and for items that were never edited the creation time. An item
  with no timestamp at all is skipped rather than guessed.
- **Threshold:** the desktop Watchtower has a *Flag logins unused for* setting with **6, 12 or 24
  months** or **Off** (default **12**); the CLI uses `--unused-months` (default 12, `0` turns it off).
  A month counts as one twelfth of 365 days, so 6 months = 182 days, 12 = 365 and 24 = 730.
- The comparison is strict: an item exactly at the threshold is kept. `days` is the whole number of days
  since the last activity.
- Archived and recently deleted items are ignored, and so is every non-Login category. Ignoring the
  `unused` warning on an item works like any other finding kind.
- Because last use is local, two devices can disagree about whether an item is unused. That is
  intentional: the finding answers "have *I* used this lately?" and never leaves the device.
- In the desktop, each row offers **Archive** (with Undo) and **Ignore this warning**.

## Per-login rotation reminders

A Login can carry its own reminder, **Change password every…** (30, 90, 180 or 365 days; off by default):
set it in the desktop editor or with `gitpass edit <item> --rotate-every 90d` (`none` clears it). It is
stored with the item (`rotate_every_days`), syncs and survives history restore, copy, move and gitpass-json
export/import, and vaults that never use it are unchanged.

- `RotationDue { every_days, days }`: the password is at least `every_days` days old.
- `RotationSoon { every_days, in_days }`: the reminder falls due within the next **14 days**.
- Age is measured from when the **password** last changed (the same timestamp as `old`), not from the
  last edit, so editing notes never resets it. Archived and recently deleted items are skipped.
- Both are ignorable like any other finding (kinds `rotation_due`, `rotation_soon`) and **never change the
  score**; the global `old` threshold keeps working independently.
- In the desktop, a due or soon login shows an orange pill ("Due for rotation" / "Rotate in N days") in the
  item list (a dot in compact density) and under the title in the detail, and the Watchtower page has a
  card for each kind with the usual **Change password** action.

## Ignored findings

A finding can be ignored per item and per kind (*Ignore this warning* in the desktop Watchtower; *Stop
ignoring* in the Watchtower "Ignored warnings" section or in the item's detail). The choice is stored
in the item (`ignored_findings`), syncs like any other edit, and applies to every finding of that kind
on that item. `missing_totp` and `totp_available` count as one kind, as do `pwned` and the desktop's
`breached`.

- Ignored findings are left out of the per-kind counts, the per-item list and the desktop score.
  The report counts them in `ignored`, and lists them in `ignored_per_item` (`--json`) / the desktop's
  "Ignored warnings" section.
- The CLI text output prints only the `ignored` count unless you pass **`--include-ignored`**, which adds
  an "ignored findings" block with each ignored finding marked `(ignored)`. `--json` always contains
  `ignored_per_item` when there are ignored findings.

## Change-password links

Each reported item (`per_item` and `ignored_per_item`) carries `change_password_url` when its URL allows
it: `https://<host>[:port]/.well-known/change-password` (the W3C "well-known change password URL").
The host's subdomain and an explicit port are kept; `http://` is upgraded to `https://`; path, query,
fragment and userinfo are dropped. It is omitted for items without an http(s) URL, IP-address hosts,
`localhost` / `*.local` and malformed hosts. The desktop uses it for the *Change password* action;
whether a site honours the well-known URL is up to the site.

## Score weighting (desktop only)

The desktop report turns the findings into one number, so it can only be as good as the underlying checks.

```
bad          = number of distinct items flagged Weak, Reused or Pwned   (union: one item counts once)
with_pw      = items with a non-empty password
items        = all audited items (every non-passkey item, including password-less ones)
missing_totp = items flagged MissingTotp or TotpAvailable (2FA not set up; each item counts once)
penalty      = 80 × bad / with_pw  +  10 × old / with_pw  +  10 × missing_totp / items
score        = round(100 − penalty), clamped to 0…100
```

- When no audited item has a password (`with_pw == 0`) the score stays **100** — there is nothing to weigh.
- `passkey_available`, `expired`, `expiring_soon`, `insecure_url`, `duplicate` and `unused` are advice and
  never change the score; ignored findings are left out entirely.
- A login on a TOTP-capable site is reported as `totp_available` rather than `missing_totp`, but both
  mean "no second factor stored" and carry the same weight.
- Reused passwords move the score the most because each affected item counts toward `bad`.
- Rounding is to the nearest integer (`63.33 → 63`).
- Bands: **≥ 85 Excellent**, **≥ 70 Good**, **≥ 50 Fair**, **< 50 Poor** (same thresholds as the gauge colors).

### Worked examples

Ten logins with strong, unique passwords on sites that are not in the offline 2FA list, so every login
without a TOTP secret counts as missing TOTP (on TOTP-capable sites such as GitHub the finding is
`2FA available` instead, with the same 10-point weight):

| Situation | bad | old | missing TOTP | penalty | score |
|---|---|---|---|---|---|
| All have TOTP | 0 | 0 | 0 | 0 | 100 |
| None has TOTP | 0 | 0 | 10 | 10 | 90 |
| One weak password, none has TOTP | 1 | 0 | 10 | 8 + 10 | 82 |
| One password not changed for a year | 0 | 1 | 10 | 1 + 10 | 89 |
| Two items share a password, none has TOTP | 2 | 0 | 10 | 16 + 10 | 74 |
| All ten use the same weak password, none has TOTP | 10 | 0 | 10 | 80 + 10 | 10 |

(`missing_totp / items = 10 / 10 = 1`, contributing 10 points whenever every login lacks TOTP.)

## Score history

The desktop Watchtower page shows the score over time as a sparkline (`apps/desktop/src/features/watchtower/ScoreHistory.tsx`, with the logic in `history.ts`). Each time the page loads a fresh Watchtower report, it records one point for the current local day: the score plus per-finding counts only — no item ids, titles or other vault content. At most one point per day is kept (a later load the same day replaces the earlier one), and points older than 90 days are dropped. The sparkline offers a 30/90-day range and a weekly delta against the latest point at least 7 days old. History is per-device: it lives in the browser's localStorage under `gitpass.watchtowerHistory` and is never written to the vault repo or synced.

## Limits

- The score is a heuristic for priorities, not a security guarantee. A password that passes all checks
  can still be guessable, and an old password is not automatically compromised.
- The entropy estimate is a character-pool calculation, not a crack-time model (see above).
- Reuse is exact-match only; one-character variations are not detected.
- The offline site list is bundled with the release; it is not refreshed automatically.
- Findings never include secret values; do not add logging that prints them.
