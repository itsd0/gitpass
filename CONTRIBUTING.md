# Contributing

gitpass is a personal project of @itsd0, developed with several AI agents. **[AGENTS.md](AGENTS.md) is the source of truth** for layout, commands and rules.

Workflow:

1. Pick (or open) an issue using the Feature or Bug template; it is labelled `agent:<name>` and `area:<area>`.
2. Create a branch `agent/<agent>/<feature>` from `main` (on the Mac: `scripts/agent-worktree.sh add <agent> <feature>`).
3. Make the change with tests. Run the commands in AGENTS.md with a temp `HOME`.
4. Open a PR using the template. CI runs on the owner's self-hosted Mac.
5. Grok Bot / the owner reviews. **Only the owner merges or releases.**

Never push to `main`, edit `.github/workflows`, commit secrets, or touch real vault data (`~/.gitpass`, `itsd0/gitpass-vault`, Keychain, `~/.gitpass-release`).
