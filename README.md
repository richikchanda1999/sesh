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
| `sesh stop [name] [--keep-branches]` | Tear down session and clean up worktrees |
| `sesh resume [name]` | Re-open VS Code windows for a session |
| `sesh status [name]` | Show git status per repo in a session |
| `sesh pr [name] [--base main]` | Push branches and create GitHub PRs |
| `sesh init` | Generate `sesh.toml` interactively |
| `sesh doctor` | Detect and fix orphaned worktrees/sessions |

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

Running `sesh start -b my-feature` creates:

```
MyProject/
├── .sesh/
│   └── sessions/
│       └── my-feature/
│           ├── session.json
│           ├── context/
│           │   └── .sesh-context.md
│           ├── server/          (git worktree on branch my-feature)
│           │   ├── .mcp.json
│           │   ├── .env         (copied from original)
│           │   └── ...
│           └── web-code/        (git worktree on branch my-feature)
│               ├── .mcp.json
│               ├── .env
│               ├── node_modules (symlinked from original)
│               └── ...
├── server/
├── web-code/
├── admin/
└── sesh.toml
```

Multiple sessions can coexist. Each is isolated in its own worktree set.

## Configuration

### `sesh.toml`

```toml
[session]
base_branch = "main"
shared_context = ["ARCHITECTURE.md"]

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

[repos.web-code]
copy = [".env"]
symlink = ["node_modules"]

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

### Scripts

Setup and teardown scripts run with these environment variables:

| Variable | Value |
|----------|-------|
| `SESH_SESSION` | Session name |
| `SESH_BRANCH` | Branch name |
| `SESH_REPOS` | Comma-separated list of repo names |

Scripts run with the session directory as their working directory and inherit the terminal for interactive prompts.

## Prerequisites

- **git** — for worktree operations
- **code** (optional) — VS Code CLI for auto-opening windows
- **gh** (optional) — GitHub CLI for `sesh pr`

## License

MIT
