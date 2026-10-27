# AGENTS.md — working on gitpass (for AI agents and humans)

gitpass is a **personal** password manager whose encrypted vault lives in a git repo.
Targets: **macOS** (desktop app, CLI, iOS) and **Chromium** browsers (extension). Single owner
(@itsd0); no team, SSO or enterprise features.

Agents on this repo: **Edison** (GitHub App, branches/PRs only), **opencode** (CLI on the owner's Mac),
**agy** (Google Antigravity IDE on the Mac), **Grok Bot** (review, Mac build/install, releases).
Merges and releases happen **only with the owner's OK**.

## Layout

| Path | What |
|---|---|
| `crates/gitpass-core` | Vault format, crypto (`crypto.rs`, `keys.rs`), store/commit (`store.rs`), merge/sync, import, audit (Watchtower/HIBP), passkeys, SSH keys, TOTP |
| `crates/gitpass-cli` | `gitpass` CLI (`init`, `add`, `get`, `run`/`inject` secret injection, `ssh`, `audit`, `import`/`export`, …) |
| `crates/gitpass-host` | Native-messaging host for the browser extension |
| `apps/desktop/gui-core` | Rust API behind the desktop app (`api.rs`, browser agent, ssh-agent) |
| `apps/desktop/src-tauri` | Tauri shell (**excluded from the workspace**, own `Cargo.lock`) |
| `apps/desktop/src` | React/TypeScript UI (Vite, zustand, vitest). Browser mock in `src/api/mock.ts` |
| `extension/` | Chromium MV3 extension (plain JS, `node --test`) |
| `ios/` | SwiftUI app + `ios/rust/gitpass-ios-ffi` (uniffi), XcodeGen `project.yml` |
| `.github/workflows` | CI on the owner's self-hosted Mac — **owner only, do not edit** |
| `docs/RELEASE.md`, `scripts/` | Release notes/process, helper scripts |

## Setup

Rust ≥ 1.85 (`rust-version` in `Cargo.toml`), Node 22, Xcode + XcodeGen for iOS.

```sh
cd apps/desktop && npm ci      # desktop UI deps
cd extension && npm ci         # extension deps
```

## Build / test / lint (same as CI, `.github/workflows/ci.yml`)

Always run tests with a throwaway `HOME` so nothing touches a real vault:

```sh
export HOME=$(mktemp -d)   # in a subshell, or prefix each command with HOME=$(mktemp -d)

# Rust workspace (core, cli, host, gui-core, ios ffi)
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings

# Desktop app
cd apps/desktop
npm test                   # vitest
npx tsc --noEmit           # type check (also part of `npm run build`)
npm run build              # tsc + vite build
cd src-tauri && cargo check --locked && cargo clippy --locked --all-targets -- -D warnings

# Extension
cd extension
npm test                   # node --test test/*.test.js
npm run check              # syntax check + locale keys must match across en/my/th

# Supply chain (CI job "supply chain")
cargo deny --locked check

# iOS (Mac only)
cargo test -p gitpass-ios-ffi --locked
./ios/scripts/build-xcframework.sh && (cd ios && xcodegen generate)
```

Dev UI without Tauri: `cd apps/desktop && npm run dev` (uses the fake in-browser mock).

CI check names on PRs: **`rust + frontend (self-hosted Mac)`** and **`supply chain (cargo deny + cargo audit)`**
(workflow `CI`), plus `Rust FFI tests` / `iOS simulator build + test` (workflow `iOS`).
Edison cannot read CI results; ask the owner or Grok Bot to report them.

## Rules

1. **One issue = one branch = one PR.** Branch name: `agent/<agent>/<feature>` (e.g. `agent/opencode/desktop-export`).
   Use `scripts/agent-worktree.sh` on the Mac so agents never share a working tree.
2. **PRs only. Never push to `main`**, never force-push someone else's branch, never merge, tag or release.
3. **Never touch real user data or secrets:** `~/.gitpass`, the `itsd0/gitpass-vault` repo, macOS Keychain items,
   `~/.gitpass-release` (signing keys), `~/Library/Application Support/gitpass`.
4. **Tests use temp dirs/temp vaults only** (`tempfile::tempdir()`, `mktemp -d`); all test data is fake.
5. **No secrets in commits, logs, PR text or screenshots.** Use obviously fake values (`FAKE-…`, `example.invalid`).
6. **Do not edit `.github/workflows/*`** — owner only. If a workflow change is needed, describe it in the PR.
   New actions must be pinned to a full commit SHA with a version comment.
7. **Style:** desktop app accent is **orange**; the iOS UI stays **monochrome**. Follow existing patterns and UI kit (`apps/desktop/src/ui`).
8. **Scope:** macOS + Chromium only; personal use — no team/SSO/admin-console features.
9. Crypto, vault format and release config (`crates/gitpass-core/src/crypto.rs`, `keys.rs`, `store.rs` format,
   `apps/desktop/src-tauri/tauri.conf.json`, `docs/RELEASE.md`) need an explicit note in the PR and owner review.
10. Keep `Cargo.lock` / `package-lock.json` changes intentional (`--locked` in CI). New deps must pass `cargo deny`.
11. Locale changes: add every new extension key to `en`, `my` and `th`.
12. Never fabricate results: only claim a test/CI passed if you ran or saw it.

## Worktrees (`scripts/agent-worktree.sh`)

```sh
scripts/agent-worktree.sh add <agent> <feature> [base]   # ~/gitpass-wt/<agent>/<feature> on agent/<agent>/<feature> (base: origin/main)
scripts/agent-worktree.sh list
scripts/agent-worktree.sh remove <agent> <feature>       # refuses if there are uncommitted changes
```

## PR checklist

- [ ] Linked issue (`Closes #N`) and agent label (`agent:<name>`) + area label
- [ ] Branch `agent/<agent>/<feature>`, based on current `main`
- [ ] Tests added/updated; the commands above pass locally (list which ones you ran)
- [ ] `cargo clippy … -D warnings` clean (workspace and `src-tauri` if touched)
- [ ] No workflow edits, no secrets, no real vault/Keychain access
- [ ] UI changes: screenshot with fake data; orange desktop / monochrome iOS
- [ ] Crypto/format/release changes called out

## PR description template

```markdown
## What
<one paragraph>

## Why
Closes #<issue>

## How
- <key changes>

## Tests
- [ ] cargo test --workspace   - [ ] cargo clippy -D warnings
- [ ] apps/desktop: npm test, tsc --noEmit   - [ ] extension: npm test, npm run check
Commands run and results: <paste summary, no secrets>

## Risk / notes
<crypto, vault format, migration, UI screenshots>

Agent: <edison | opencode | agy | grokbot | human>
```
