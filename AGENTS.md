# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Project Overview

**Scorekeeper** is a Rust/Actix-web API for sports score tracking.

| Stack | Details |
|-------|---------|
| Language | Rust (2021 edition) |
| Framework | Actix-web 4 |
| Database | DynamoDB (planned) |
| Cache | Redis (planned) |
| Auth | JWT bearer tokens |

## Agent Workflow

### 1. Find Available Work

```bash
bd ready                    # Show unblocked issues
bd show <id>                # View issue details and dependencies
```

### 2. Claim Work and Create Worktree

**CRITICAL: Always work in a dedicated worktree and branch.**

```bash
# Claim the issue
bd update <id> --status in_progress

# Create worktree with feature branch (from repo root)
git worktree add ../scorekeeper-<id> -b <id>/<short-description> main

# Navigate to worktree
cd ../scorekeeper-<id>

# Verify you're on the correct branch
git branch --show-current
```

**Naming conventions:**
- Worktree directory: `../scorekeeper-<bead-id>` (e.g., `../scorekeeper-sk-bmb`)
- Branch name: `<bead-id>/<short-description>` (e.g., `sk-bmb/init-project-structure`)

### 3. Do the Work

- Write code, tests, documentation
- Run quality gates frequently: `cargo build && cargo test && cargo clippy`
- Commit incrementally with meaningful messages

### 4. Quality Gates (Before Completing)

**ALL must pass before session end:**

```bash
cargo fmt --check           # Formatting
cargo clippy -- -D warnings # Lints
cargo build                 # Compilation
cargo test                  # All tests
```

---

## Session Completion Protocol

**Work is NOT complete until your branch is pushed and a PR is opened.**

```bash
# 1. Ensure all changes committed
git status

# 2. Push branch to remote
git push -u origin $(git branch --show-current)

# 3. Open PR
gh pr create --title "<bead-id>: <description>" --body "Closes <bead-id>

## Summary
- <what was done>

## Test Plan
- [ ] <verification steps>
"

# 4. Sync beads
bd sync

# 5. Verify PR exists
gh pr view
```

### After PR is Merged (or if continuing later)

```bash
# Return to main repo
cd /path/to/scorekeeper

# Clean up worktree
git worktree remove ../scorekeeper-<id>

# Delete local branch (if merged)
git branch -d <id>/<short-description>
```

### Handoff Notes

Before ending session, provide:
1. PR link
2. What was completed
3. What remains (create new beads if needed)
4. Any blockers or decisions needed

---

## Beads Quick Reference

```bash
bd ready                              # Find available work
bd show <id>                          # View issue details
bd update <id> --status in_progress   # Claim work
bd close <id>                         # Complete work
bd create --title="..." --type=task --priority=2  # New issue
bd dep add <issue> <depends-on>       # Add dependency
bd sync                               # Sync with git
```

---

## Critical Rules

### Deployments
**NEVER deploy from the command line.** All deployments MUST go through GitHub Actions:
- Push code to main (or create a PR and merge it)
- Let GitHub Actions handle the build and deployment
- Do NOT run `cdk deploy`, `aws ecs update-service`, or any deployment commands locally
- If you need to trigger a deployment, push to the repo and let CI/CD handle it

### Mandatory Workflow
**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

- pick up only tasks, not epics
- DO NOT ASK FOR PERMISSION TO GET WORK DONE. I believe in you, you're smart! The PR is when you ask for permission. If you ever are fully blocked by a systems-level permission, ALWAYS add the command you asked to use to `.claude/settings.local.json`
- **File issues for remaining work** - Create issues for anything that needs follow-up
- **Always use worktrees** - Never work directly on `main`
- **Always push to remote before ending** - Local branches are useless to others
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
- **Always open a PR** - Even for WIP, use draft PRs
- **Run quality gates** (if code changed) - Tests, linters, builds
- **Update issue status** - Close finished work, update in-progress items
- **Clean up** - Clear stashes, prune remote branches
- **Verify** - All changes committed AND pushed
- **Hand off** - Provide context for next session
- **Never say "ready when you are"** - YOU must push and open the PR
- **Run tests before pushing** - Broken builds block everyone
- **Create beads for follow-up work** - Don't leave undocumented TODOs

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:970c3bf2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
