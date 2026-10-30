# gitpass vX.Y.Z — release notes template

<!--
How to use this template (these comments do not render on the release page):
1. Copy this file into the draft release body and replace every `vX.Y.Z` /
   `vPREV` placeholder with the real versions.
2. Write the intro paragraph by hand — one short paragraph saying what this
   release is about. Do not paste the changelog here; the themed sections
   below are the human-written summary.
3. Delete every themed section that has no entries. Empty sections must not
   ship.
4. Every bullet gets a bold lead-in and a link to the PR that shipped it:
   `- **Lead-in:** what changed ([#N](https://github.com/itsd0/gitpass/pull/N))`.
5. Keep the What's Changed list as generated: on the draft release page,
   click "Generate release notes" (it uses `.github/release.yml`) and paste
   the result under "What's Changed" below. See `docs/RELEASE.md`
   ("Writing release notes") for the labelling convention that drives it.
6. For UI-facing releases, an optional Highlights screenshot or GIF goes
   under Highlights.
-->

<!-- Intro: one short, human-written paragraph about what this release is about. -->

## Highlights

<!-- The one or two changes a user will notice first. Optional screenshot or GIF here. -->

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## Desktop app

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## Browser extension

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## CLI

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## Security

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## Fixes

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## Docs & maintenance

- **Lead-in:** description ([#N](https://github.com/itsd0/gitpass/pull/N))

## What's Changed

<!-- Paste the auto-generated list here ("Generate release notes" on the draft).
     Format: `- title by @author in #N`. -->

## Install / Upgrade

- **Apple Silicon Macs:** download `gitpass_X.Y.Z_aarch64.dmg`.
- **Intel Macs:** download `gitpass_X.Y.Z_x64.dmg`.
- **Existing installs** auto-update via Settings → About → Check for updates;
  no manual download is needed unless auto-update is off.
- **Required user action (if any):** <!-- e.g. re-sharing an item, re-enabling
  Touch ID — delete this line when there is none. -->
- **macOS Gatekeeper note:** macOS builds are signed with a self-signed
  certificate (not Developer ID, not notarized).
- **Updater files** (`gitpass_X.Y.Z_{aarch64,x64}.app.tar.gz`, `.sig`,
  `latest.json`) are used by auto-update; you do not need to download them.
<!-- Add a SHA256SUMS.txt line here once the release workflow attaches one
     to every release (owner-only workflow change; see #296 follow-up). -->

**Full Changelog:** https://github.com/itsd0/gitpass/compare/vPREV...vX.Y.Z
