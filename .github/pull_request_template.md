## What

## Why
Closes #

## How
-

## Tests
- [ ] `cargo test --workspace` (temp HOME)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (+ `apps/desktop/src-tauri` if touched)
- [ ] `apps/desktop`: `npm test`, `npx tsc --noEmit`
- [ ] `extension`: `npm test`, `npm run check`

Verified in env: <!-- macOS … | Linux (distro, arch) | GitHub-only (relies on CI) -->

Commands run and results (and what was skipped):

## Checklist (see AGENTS.md)
- [ ] Branch `agent/<agent>/<issue#>-<slug>`; labels `agent:*` + `area:*`
- [ ] No edits to `.github/workflows`, no secrets, no real vault / Keychain access
- [ ] Crypto / vault format / release changes called out below
- [ ] UI: screenshot with fake data (desktop orange, iOS monochrome)

## Risk / notes

Agent: <!-- edison | opencode | agy | muse | grokbot | human -->
