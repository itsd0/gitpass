# Watchtower: how password health is scored

Watchtower is gitpass's security audit. The CLI (`gitpass audit`) prints the findings; the desktop app
shows the same findings plus a 0–100 score. Everything is offline unless you opt in to the Have I Been
Pwned (HIBP) breach check.

- Checks: `crates/gitpass-core/src/audit.rs`
- Desktop score: `watchtower()` in `apps/desktop/gui-core/src/api.rs`
- Score bands and gauge: `apps/desktop/src/features/watchtower/score.ts`
- Offline 2FA site list: `crates/gitpass-core/data/mfa-sites.tsv`

```sh
gitpass audit                      # every vault, default thresholds (365 days)
gitpass audit Personal             # one vault
gitpass audit --max-age-days 180   # stricter "old password" threshold
gitpass audit --hibp               # opt in to the breach check
```

## What gets audited

Only **Login** items that are not deleted and not archived. The CLI audits the selected vault (or all
vaults); the desktop app audits every vault at once, so reuse is detected across vaults. Passkey items
are read (never reported) to know which sites already have a stored passkey.

Password findings (`Weak`, `Reused`, `Pwned`, `Old`) only apply to items with a non-empty password.
Findings carry titles and counts only — **passwords, password hashes and TOTP secrets never appear in
reports, logs or the UI**.

## Weak password checks

`weakness()` applies the rules in this order and reports the first one that matches:

| # | Rule | Default |
|---|------|---------|
| 1 | Contains a bundled common password as a substring (case-insensitive; e.g. `password`, `qwerty`, `master`). | 18-entry list |
| 2 | At least 4 characters but at most 2 distinct characters (`aaaa`, `abab`). | — |
| 3 | Shorter than `min_length` characters (Unicode scalar values, not bytes). | 12 |
| 4 | Estimated entropy below `min_entropy_bits`. | 60 bits |

The rules are substring/character based on purpose: they are cheap, deterministic and explainable.
Rule 1 matches *inside* a longer password, so `MyPassword2024!!` is still flagged.

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

An item is `Old { days }` when `updated_at` parses as Unix seconds and
`(now − updated_at) / 86 400 > max_age_days` (default 365; CLI `--max-age-days`). The comparison is
strict, so an item exactly 365 days old is not reported. Only items with a password are checked.

## Missing TOTP and offline 2FA advice

- `MissingTotp`: the login has a non-empty URL, no TOTP secret, **and** the offline list has no TOTP
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

## Score weighting (desktop only)

The desktop report turns the findings into one number, so it can only be as good as the underlying checks.

```
bad          = number of distinct items flagged Weak, Reused or Pwned   (union: one item counts once)
with_pw      = items with a non-empty password
items        = all audited Login items (including password-less ones)
missing_totp = items flagged MissingTotp or TotpAvailable (2FA not set up; each item counts once)
penalty      = 80 × bad / with_pw  +  10 × old / with_pw  +  10 × missing_totp / items
score        = round(100 − penalty), clamped to 0…100
```

- When no login has a password (`with_pw == 0`) the score stays **100** — there is nothing to weigh.
- `passkey_available` is advice and never changes the score. A login on a TOTP-capable site is reported
  as `totp_available` rather than `missing_totp`, but both mean "no second factor stored" and carry the
  same weight.
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

## Limits

- The score is a heuristic for priorities, not a security guarantee. A password that passes all checks
  can still be guessable, and an old password is not automatically compromised.
- The entropy estimate is a character-pool calculation, not a crack-time model (see above).
- Reuse is exact-match only; one-character variations are not detected.
- The offline site list is bundled with the release; it is not refreshed automatically.
- Findings never include secret values; do not add logging that prints them.
