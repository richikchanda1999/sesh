# sesh

Multi-repo worktree session manager for AI-assisted development.

`sesh` creates isolated git worktree sessions across multiple repos with a single command — copying `.env` files, symlinking `node_modules`, configuring MCP servers for Claude Code, and opening VS Code — so you can start working on a feature in seconds.

## The Problem

Working on multi-repo projects with Claude Code requires manually:
- Creating git worktrees in each repo
- Copying `.env` and other gitignored files
- Symlinking heavy directories like `node_modules`
- Configuring `.mcp.json` for Sentry/Linear in each worktree
- Opening VS Code for each repo

For every feature, every time. `sesh` does all of this in one command.

## Install

### Homebrew (macOS/Linux)

```bash
brew install richikchanda1999/tap/sesh
```

### Shell script (macOS/Linux)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/richikchanda1999/sesh/releases/latest/download/sesh-installer.sh | sh
```

### Cargo (from source)

```bash
cargo install --git https://github.com/richikchanda1999/sesh.git
```

### Prebuilt binaries

Download from [GitHub Releases](https://github.com/richikchanda1999/sesh/releases).

## Quick Start

```bash
cd ~/Development/MyProject   # parent dir containing your repos

sesh init                     # generate sesh.toml (one-time)
sesh start                    # create a session interactively
sesh list                     # see active sessions
sesh stop                     # tear down a session
```

## Commands

| Command | Description |
|---------|-------------|
| `sesh start [-b branch] [--all] [--preset name]` | Create a new worktree session |
| `sesh list [--active]` | List sessions |
| `sesh stop [name] [--keep-branches]` | Tear down session, clean up worktrees, and release locks |
| `sesh resume [name]` | Re-open VS Code for a session |
| `sesh activate [name]` | Transfer exclusive locks to a session (runs teardown/setup) |
| `sesh status [name]` | Show git status per repo in a session |
| `sesh pr [name] [--base main]` | Push branches and create GitHub PRs |
| `sesh init` | Generate `sesh.toml` interactively |
| `sesh doctor` | Detect and fix orphaned worktrees, sessions, and stale locks |

All commands accept `-d <DIR>` to specify the parent directory (defaults to cwd).

## How It Works

Given a directory layout like:

```
MyProject/
├── server/          (git repo)
├── web-code/        (git repo)
├── admin/           (git repo)
└── sesh.toml
```

Running `sesh start -b feature/auth` creates:

```
MyProject/
├── .sesh/
│   ├── sessions/
│   │   └── feature-auth/
│   │       ├── session.json
│   │       ├── docker-compose.yml   (copied from parent dir)
│   │       ├── context/
│   │       │   ├── .sesh-context.md
│   │       │   └── ARCHITECTURE.md  (symlinked from parent dir)
│   │       ├── server/          (git worktree on branch feature/auth)
│   │       │   ├── .mcp.json
│   │       │   ├── .env         (copied from original)
│   │       │   └── ...
│   │       └── web-code/        (git worktree on branch feature/auth)
│   │           ├── .mcp.json
│   │           ├── .env
│   │           ├── node_modules (symlinked from original)
│   │           └── ...
│   └── locks/
│       └── server.lock          (exclusive lock, if configured)
├── server/
├── web-code/
├── admin/
└── sesh.toml
```

Branch names with `/` are sanitized into flat folder names (`feature/auth` → `feature-auth`). If a folder name collides with an existing session, `-2`, `-3`, etc. are appended. The real branch name is preserved for all git operations.

Multiple sessions can coexist. Each is isolated in its own worktree set. With 2+ repos, VS Code opens the session folder in a single window; with 1 repo it opens just that worktree.

## Configuration

### `sesh.toml`

```toml
[session]
base_branch = "main"
shared_context = ["ARCHITECTURE.md"]
copy = ["docker-compose.yml"]       # files from parent dir copied into session dir

[scripts]
setup = "./scripts/setup-dev.sh"
teardown = "./scripts/teardown-dev.sh"

# MCP servers configured in every worktree
[[mcp.servers]]
name = "sentry"
type = "http"
url = "https://mcp.sentry.dev/mcp"

[[mcp.servers]]
name = "linear"
type = "http"
url = "https://mcp.linear.app/mcp"

# Per-repo config
[repos.server]
copy = [".env", "supabase/functions/.env"]
symlink = []
exclusive = true                 # only one session runs services for this repo
setup = "./scripts/server-setup.sh"
teardown = "./scripts/server-teardown.sh"

[repos.web-code]
copy = [".env"]
symlink = ["node_modules"]
setup = "./scripts/web-setup.sh"

[repos.admin]
copy = [".env"]
skip = true                  # excluded from interactive selection by default

# Presets for quick selection
[presets]
fullstack = ["server", "web-code"]
all = ["server", "web-code", "admin"]
```

### Per-Repo Options

| Field | Description |
|-------|-------------|
| `base_branch` | Override the default base branch for this repo |
| `copy` | Files to copy from the original repo into the worktree |
| `symlink` | Files/directories to symlink (e.g., `node_modules` to avoid reinstalling) |
| `skip` | Exclude from default selection in the interactive picker |
| `exclusive` | Only one session can hold the lock for this repo at a time (see below) |
| `setup` | Per-repo setup script, run with the worktree as working directory |
| `teardown` | Per-repo teardown script, run before worktree removal |

### Scripts

There are two levels of scripts:

- **Global** (`[scripts].setup` / `[scripts].teardown`) — run once per session, with the session directory as cwd.
- **Per-repo** (`[repos.X].setup` / `[repos.X].teardown`) — run once per repo, with the worktree as cwd. Per-repo setup runs after the global setup; per-repo teardown runs before the global teardown.

All scripts receive these environment variables:

| Variable | Value |
|----------|-------|
| `SESH_SESSION` | Session name |
| `SESH_BRANCH` | Branch name |
| `SESH_REPOS` | Comma-separated list of all repo names in the session |
| `SESH_REPO` | Current repo name (per-repo scripts only) |
| `SESH_EXCLUSIVE_SKIP` | Comma-separated repos whose exclusive lock is held by another session (global setup only) |

Scripts inherit the terminal for interactive prompts.

### Exclusive Locks

Repos with `exclusive = true` use a file-based lock so only one session runs their services (dev servers, etc.) at a time. Locks are stored at `.sesh/locks/<repo>.lock`.

- **`sesh start`** — acquires the lock if free or stale; if another active session holds it, the repo is added to `SESH_EXCLUSIVE_SKIP` so your setup script can skip starting its services.
- **`sesh stop`** — releases locks held by the session being stopped.
- **`sesh activate [name]`** — transfers locks to a different session, running teardown for the previous holder and setup for the new one. Useful for switching which session is "live" without recreating worktrees.
- **`sesh doctor`** — detects and cleans up stale locks.

## Prerequisites

- **git** — for worktree operations
- **code** (optional) — VS Code CLI for auto-opening windows
- **gh** (optional) — GitHub CLI for `sesh pr`

## License

MIT
