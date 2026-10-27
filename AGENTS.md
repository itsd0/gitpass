# AGENTS.md — working on gitpass (for AI agents and humans)

> **Recommended prompt for any agent:**
> "You are working on itsd0/gitpass. First read AGENTS.md and CONTRIBUTING.md in the repo root and follow every rule in them, and contribute to the repo."
>
> That prompt is enough: this file holds the whole workflow. Follow it top to bottom.

gitpass is a **personal** password manager whose encrypted vault lives in a git repo.
Targets: **macOS** (desktop app, CLI, iOS) and **Chromium** browsers (extension). Single owner
(@itsd0); no team, SSO or enterprise features. Merges, tags and releases happen **only with the owner's OK**.

Known agents: **edison**, **opencode**, **agy** (Antigravity), **muse**, **grokbot** (review, Mac build/install, releases).
Any other agent picks its own lowercase name. Agents may run on the owner's Mac, a cloud VM, a Linux
container, or GitHub only (no shell, e.g. a GitHub App).

## Start here: autonomous contribution loop

1. **Identify yourself.** If your prompt says `Your agent name is <name>`, use exactly that name. Never guess it
   from your model or app (a Gemini model inside Codex is `codex`, not `agy`). Otherwise use the name of the app
   you run in: `edison`, `opencode`, `agy`, `muse`, `qoder`, `codex`, `grokbot`, or another lowercase name
   (`[a-z0-9._-]`). Use it in branch names, labels (`agent:<name>` if it exists) and the PR footer.
2. **Pick work.** List open issues (`gh issue list --state open`, the REST API, or the web UI).
   - Prefer issues labelled `agent:<you>`; otherwise pick an unassigned issue with no open PR and no
     `agent/*/<issue#>-*` branch (`gh pr list --search "<issue#>"`, `git ls-remote --heads origin "agent/*"`).
   - **Check claims first:** read the issue comments and labels. If any `Claiming as` comment or `agent:` label
     from another agent exists within the last **48 h**, skip it.
   - **Claim it:** comment `Claiming as <agent>` and add your `agent:` label if permitted.
   - **Re-check after claiming:** reload the comments. If another agent's `Claiming as` comment is earlier than
     yours, the earlier claim wins: comment `Releasing, <other> claimed first`, remove your label, and pick
     another issue. Never work on an issue you lost.
   - **No suitable issue?** Do not invent big features. Stop and report, or open a *small* issue proposal
     (Feature template) and wait for the owner.
3. **Branch** `agent/<name>/<issue#>-<slug>` from the latest `main` (e.g. `agent/opencode/5-desktop-export`).
   - Shared machine (the owner's Mac): `scripts/agent-worktree.sh add <name> <issue#>-<slug>`.
   - Your own VM/container: a normal clone + `git switch -c agent/<name>/<issue#>-<slug> origin/main`.
   - GitHub-only: create the branch from `main` through the API/UI.
4. **Detect your environment** and use the matrix below to know what you can run (`uname -s`;
   no shell ⇒ GitHub-only).
5. **Implement** the smallest change that closes the issue, with tests. Tests use a temp `HOME` and temp
   vaults only (`HOME=$(mktemp -d)`, `tempfile::tempdir()`); all test data fake.
6. **Verify** by running every command your environment supports (matrix). Record what you ran, the result,
   and what you skipped and why. Never claim a result you did not see.
7. **Open a PR** to `main` using the template: `Closes #N`, your `agent:<name>` + `area:*` labels, and a line
   `Verified in env: <macOS 15 arm64 | Linux (distro, arch) | GitHub-only (relies on CI)> — <commands run>`.
8. **Review loop.** Address review comments by pushing more commits to the same branch. CI must be green.
   **Never merge, tag, release, or push to `main`.** The owner merges.

### Environment matrix

| Command | macOS (owner's Mac) | Linux VM / container | GitHub-only |
|---|---|---|---|
| `cargo test --workspace --locked` (core, cli, host, gui-core, ios-ffi) | ✅ | ✅ verified | ❌ → CI |
| `cargo clippy --workspace --locked --all-targets -- -D warnings` | ✅ | ✅ verified (Rust **1.90.0**) | ❌ → CI |
| `apps/desktop`: `npm ci && npm test`, `npx tsc --noEmit`, `npm run build` | ✅ | ✅ verified | ❌ → CI |
| `extension`: `npm ci && npm test`, `npm run check` | ✅ | ✅ verified | ❌ → CI |
| `cargo deny --locked check` (needs `cargo install --locked cargo-deny`) | ✅ | ⚠️ unverified (should work; tool not installed in the test VM) | ❌ → CI |
| `apps/desktop/src-tauri`: `cargo check`/`clippy`, `npx tauri build` (macOS bundle, Touch ID, Keychain) | ✅ | ❌ macOS app; not supported (would also need webkit2gtk) | ❌ → CI |
| `extension` e2e (`npm run e2e`, needs Chromium + Playwright) | ✅ | ⚠️ unverified | ❌ |
| iOS: `./ios/scripts/build-xcframework.sh`, `xcodegen`, simulator tests | ✅ (Xcode) | ❌ | ❌ → CI |

"Verified" = run successfully on Debian 13 x86_64, Rust 1.90.0, Node 20 on 2026-10-27 (all Rust tests,
clippy, 103 desktop tests + tsc + build, 70 extension tests + check). macOS-specific code is behind
`#[cfg(target_os = "macos")]` with Linux fallbacks (`device.rs`, `remote.rs`, `gui-core/src/agent.rs`), so the
workspace builds on Linux without feature flags.

**Toolchain:** use Rust **1.90.0** (what CI's Mac runs; `rustup toolchain install 1.90.0 -c clippy`,
then `RUSTUP_TOOLCHAIN=1.90.0`). Newer clippy adds lints (e.g. 1.99 flags `unnecessary_sort_by`) that CI
does not; don't "fix" those in unrelated code. Node 22 matches CI (20 also works).

**GitHub-only agents** cannot run anything locally and cannot read CI logs/results in some setups: open the PR,
write `Verified in env: GitHub-only (relies on CI)`, and do not claim tests pass. The owner or Grok Bot reports CI.
They also cannot edit `.github/workflows` (and must not anyway).

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
GitHub-only agents may not be able to read CI results; ask the owner or Grok Bot.

## Rules

1. **One issue = one branch = one PR.** Branch name: `agent/<agent>/<issue#>-<slug>` (e.g. `agent/opencode/5-desktop-export`).
   On a shared machine use `scripts/agent-worktree.sh` so agents never share a working tree.
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
scripts/agent-worktree.sh add <agent> <issue#>-<slug> [base]   # ~/gitpass-wt/<agent>/<issue#>-<slug> on agent/<agent>/<issue#>-<slug> (base: origin/main)
scripts/agent-worktree.sh list
scripts/agent-worktree.sh remove <agent> <issue#>-<slug>       # refuses if there are uncommitted changes
```

## PR checklist

- [ ] Linked issue (`Closes #N`) and agent label (`agent:<name>`) + area label
- [ ] Branch `agent/<agent>/<issue#>-<slug>`, based on current `main`; issue claimed
- [ ] `Verified in env: …` line; skipped commands listed
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
Verified in env: <macOS … | Linux … | GitHub-only (relies on CI)>
Commands run and results: <summary, no secrets>; skipped: <commands + why>

## Risk / notes
<crypto, vault format, migration, UI screenshots>

Agent: <edison | opencode | agy | muse | grokbot | human>
```
