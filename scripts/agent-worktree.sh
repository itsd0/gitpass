#!/usr/bin/env bash
# Per-agent git worktrees: ~/gitpass-wt/<agent>/<feature> on branch agent/<agent>/<feature> (feature = <issue#>-<slug>).
# Usage:
#   scripts/agent-worktree.sh add <agent> <feature> [base]   (base defaults to origin/main)
#   scripts/agent-worktree.sh remove <agent> <feature>       (refuses with uncommitted changes)
#   scripts/agent-worktree.sh list
set -euo pipefail
ROOT="${GITPASS_WT_ROOT:-$HOME/gitpass-wt}"
AGENTS="edison opencode agy grokbot human"
usage() { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
valid() { [[ "$1" =~ ^[a-z0-9][a-z0-9._-]*$ ]] || { echo "invalid name: $1 (use lowercase a-z 0-9 . _ -)" >&2; exit 2; }; }
repo=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "run inside the gitpass repo" >&2; exit 2; }
cmd="${1:-}"; shift || true
case "$cmd" in
  add)
    [ $# -ge 2 ] || usage
    agent=$1 feature=$2 base=${3:-origin/main}
    valid "$agent"; valid "$feature"
    case " $AGENTS " in *" $agent "*) ;; *) echo "warning: unknown agent '$agent' (known: $AGENTS)" >&2 ;; esac
    dir="$ROOT/$agent/$feature" branch="agent/$agent/$feature"
    [ -e "$dir" ] && { echo "already exists: $dir" >&2; exit 1; }
    git -C "$repo" fetch --quiet origin main || echo "warning: fetch failed, using local $base" >&2
    mkdir -p "$ROOT/$agent"
    if git -C "$repo" show-ref --verify --quiet "refs/heads/$branch"; then
      git -C "$repo" worktree add "$dir" "$branch"
    else
      git -C "$repo" worktree add -b "$branch" "$dir" "$base"
    fi
    echo "worktree: $dir"; echo "branch:   $branch"
    echo "push with: git -C $dir push -u origin $branch   (then open a PR; never push to main)"
    ;;
  remove|rm)
    [ $# -eq 2 ] || usage
    valid "$1"; valid "$2"
    dir="$ROOT/$1/$2"
    [ -d "$dir" ] || { echo "no worktree at $dir" >&2; exit 1; }
    if [ -n "$(git -C "$dir" status --porcelain)" ]; then echo "uncommitted changes in $dir; commit/stash first" >&2; exit 1; fi
    git -C "$repo" worktree remove "$dir"
    echo "removed $dir (branch agent/$1/$2 kept; delete it after the PR is merged)"
    ;;
  list) git -C "$repo" worktree list | grep -E "$ROOT|agent/" || echo "(no agent worktrees)" ;;
  *) usage ;;
esac
