# cond

A Git worktree agent orchestrator for parallel development workflows.

`cond` automates the creation, management, and integration of isolated development tasks using Git worktrees. Each task gets its own branch and worktree, letting you work on multiple features simultaneously without stashing or switching branches. It integrates with Claude Code for AI-assisted code review and GitHub CLI for PR management.

## Prerequisites

- **Rust** toolchain (edition 2021+)
- **Git**
- **gh** (GitHub CLI) -- for PR creation and merging
- **claude** (Claude Code CLI) -- for AI-assisted review

## Installation

```bash
cargo install --path .
```

## Getting Started

```bash
cd /path/to/your/repo
cond init                    # initialize cond in the repo
eval "$(cond shell-setup)"   # enable shell integration (add to .zshrc/.bashrc)
```

`cond init` creates a `.cond/` directory for state and a `.cond-worktrees/` directory (git-ignored) for worktrees.

## Commands

| Command | Description |
|---------|-------------|
| `cond init` | Initialize cond in the current repo |
| `cond spawn <description>` | Create a new task with its own worktree and branch |
| `cond status` / `cond ls` | Show all tasks and their statuses |
| `cond cd <task>` | Print the worktree path for a task |
| `cond base` | Print the repo base path |
| `cond diff <task>` | Show the diff between a task's branch and main |
| `cond review <task>` | Open Claude Code in the task's worktree for AI-assisted review |
| `cond pr <task>` | Push the branch and create a GitHub PR |
| `cond merge <task>` | Merge the task's PR |
| `cond kill <task>` / `cond rm <task>` | Remove a task's worktree and branch |
| `cond prune` | Remove cleaned tasks from state |
| `cond nuke --confirm` | Tear down all tasks and cond infrastructure |
| `cond shell-setup` | Print shell integration code |

Commands can be abbreviated to any unambiguous prefix (e.g. `cond sp`, `cond re`, `cond st`).

All commands that take `<task>` accept either a **task ID** (e.g. `1`) or a **task name** (a substring of the description, e.g. `auth`). If the name is ambiguous, you'll be prompted to be more specific.

### PR options

```bash
cond pr <task> --title "Custom title" --draft
```

### Merge options

```bash
cond merge <task> --squash=true --delete-branch=true
```

## Workflow

```
cond spawn "Add auth feature"    # creates branch cond/task-1-add-auth-feature
                                 # and worktree .cond-worktrees/task-1

cond cd 1                        # jump into the worktree (by ID)
cond cd auth                     # ...or by name
# ... make changes ...

cond review 1                    # AI review with Claude Code
cond pr auth                     # create GitHub PR (by name works too)
cond merge 1                     # merge and clean up
cond base                        # back to the repo base
cond prune                       # remove merged tasks from state
```

## Task Lifecycle

Each task moves through these statuses:

**Active** -- task created, worktree ready for work
**PrCreated** -- PR opened on GitHub
**Merged** -- PR merged into main
**Cleaned** -- worktree and branch removed

## How It Works

- **Worktrees**: Each task gets a dedicated Git worktree at `.cond-worktrees/task-<id>`, with a branch named `cond/task-<id>-<slug>`. This provides full isolation -- no stashing, no conflicts between tasks.
- **State**: All task metadata (ID, description, branch, status, PR info, timestamps) is persisted in `.cond/state.json`.
- **Review**: `cond review` generates a diff of the task's changes and passes it to Claude Code CLI for interactive review and modification.
- **Shell integration**: The `cond cd` command requires shell integration (`eval "$(cond shell-setup)"`) to actually change your working directory.
