# Contributing

> **Recommended prompt for any agent:**
> "You are working on itsd0/gitpass. First read AGENTS.md and CONTRIBUTING.md in the repo root and follow every rule in them, and contribute to the repo."

gitpass is a personal project of @itsd0, built with several AI agents (any environment: Mac, cloud VM,
Linux container, or GitHub-only). **[AGENTS.md](AGENTS.md) is the source of truth** for layout, commands,
the environment matrix and rules.

The loop (details in AGENTS.md → "Start here"):

1. **Identify** yourself: an agent name (`edison`, `opencode`, `agy`, `grokbot`, or other lowercase).
2. **Pick** an open issue: prefer `agent:<you>`, else unassigned with no PR/branch; comment `Claiming as <agent>`;
   skip issues claimed by someone else within 48 h; no issue ⇒ stop/report or propose a small one.
3. **Branch** `agent/<name>/<issue#>-<slug>` from latest `main` (`scripts/agent-worktree.sh` on shared machines).
4. **Detect** your environment and what it can run (matrix in AGENTS.md).
5. **Implement** small, with tests, temp `HOME`/vaults only.
6. **Verify** what your env supports; list what you skipped. Never claim results you didn't see.
7. **PR** with `Closes #N`, the template, `agent:<name>` + `area:*` labels and `Verified in env: …`.
8. **Review**: push fixes to the same branch. **Only the owner merges, tags or releases.**

Never push to `main`, edit `.github/workflows`, commit secrets, or touch real vault data
(`~/.gitpass`, `itsd0/gitpass-vault`, Keychain, `~/.gitpass-release`).
