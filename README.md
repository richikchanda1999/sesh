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
| `sesh start [-b branch] [--all] [--preset name] [--linear]` | Create a new worktree session (accepts Linear/Sentry inputs) |
| `sesh list [--active]` | List sessions |
| `sesh stop [name] [--keep-branches]` | Tear down session, clean up worktrees, and release locks |
| `sesh resume [name]` | Re-open VS Code for a session |
| `sesh activate [name]` | Transfer exclusive locks to a session (runs teardown/setup) |
| `sesh status [name]` | Show git status per repo in a session |
| `sesh pr [name] [--base main]` | Push branches and create GitHub PRs |
| `sesh init` | Generate `sesh.toml` interactively |
| `sesh doctor` | Detect and fix orphaned worktrees, sessions, and stale locks |
| `sesh auth linear` | Save your Linear API token |
| `sesh auth sentry` | Save your Sentry auth token |

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
branch_prefix = "richik/"           # auto-prefix all branch names (e.g. richik/eng-123-fix-bug)
shared_context = ["ARCHITECTURE.md"]
copy = ["docker-compose.yml"]       # files from parent dir copied into session dir

# Scripts — each is an array of entries, run in order
[[scripts.setup]]
path = "./scripts/install-deps.sh"

[[scripts.setup]]
path = "./scripts/start-services.sh"
background = true                    # runs in background, auto-killed on `sesh stop`

[[scripts.teardown]]
path = "./scripts/teardown-dev.sh"

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

[[repos.server.setup]]
path = "./scripts/server-setup.sh"

[[repos.server.setup]]
path = "./scripts/server-dev.sh"
background = true

[[repos.server.teardown]]
path = "./scripts/server-teardown.sh"

[repos.web-code]
copy = [".env"]
symlink = ["node_modules"]

[[repos.web-code.setup]]
path = "./scripts/web-setup.sh"

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
| `setup` | Array of setup script entries (see below) |
| `teardown` | Array of teardown script entries (see below) |

### Scripts

Scripts use an array-of-objects format. Each entry has a `path` and an optional `background` flag.

**Global scripts** — run once per session, with the session directory as cwd:

```toml
[[scripts.setup]]
path = "./scripts/install-deps.sh"

[[scripts.setup]]
path = "./scripts/start-services.sh"
background = true

[[scripts.teardown]]
path = "./scripts/teardown-dev.sh"
```

**Per-repo scripts** — run once per repo, with the worktree as cwd:

```toml
[repos.server]
copy = [".env"]
exclusive = true

[[repos.server.setup]]
path = "./scripts/server-setup.sh"

[[repos.server.setup]]
path = "./scripts/server-dev.sh"
background = true

[[repos.server.teardown]]
path = "./scripts/server-teardown.sh"
```

Per-repo setup runs after global setup; per-repo teardown runs before global teardown.

Scripts within each level run in the order they appear in the config file.

**Background scripts** (`background = true`) are spawned as detached processes. Their stdout/stderr is redirected to `<session-dir>/logs/<label>.log`. Background PIDs are tracked and automatically killed (SIGTERM, then SIGKILL after 5s) when you run `sesh stop`.

All scripts receive these environment variables:

| Variable | Value |
|----------|-------|
| `SESH_SESSION` | Session name |
| `SESH_BRANCH` | Branch name |
| `SESH_REPOS` | Comma-separated list of all repo names in the session |
| `SESH_REPO` | Current repo name (per-repo scripts only) |
| `SESH_EXCLUSIVE_SKIP` | Comma-separated repos whose exclusive lock is held by another session (global setup only) |

Foreground scripts inherit the terminal for interactive prompts. Background scripts receive `/dev/null` as stdin.

### Exclusive Locks

Repos with `exclusive = true` use a file-based lock so only one session runs their services (dev servers, etc.) at a time. Locks are stored at `.sesh/locks/<repo>.lock`.

- **`sesh start`** — acquires the lock if free or stale; if another active session holds it, the repo is added to `SESH_EXCLUSIVE_SKIP` so your setup script can skip starting its services.
- **`sesh stop`** — releases locks held by the session being stopped.
- **`sesh activate [name]`** — transfers locks to a different session, running teardown for the previous holder and setup for the new one. Useful for switching which session is "live" without recreating worktrees.
- **`sesh doctor`** — detects and cleans up stale locks.

## Linear & Sentry Integration

`sesh start` auto-detects if your branch input is a Linear ticket or Sentry issue, fetches the title via API, and generates a branch name from it.

Use `sesh start --linear` to browse your assigned Linear tickets in a fuzzy-select picker. Tickets are grouped by status (In Progress → Todo → Backlog), with state and label names rendered in their Linear-configured colors.

### Setup

```bash
sesh auth linear   # paste your Linear API key (Settings → API → Personal API keys)
sesh auth sentry   # paste your Sentry auth token (Settings → Auth Tokens)
```

Tokens are stored in `.sesh/secrets/` (inside the parent directory, outside any repo). For Sentry, you can also set the default org in `sesh.toml`:

```toml
[sentry]
org = "your-org-slug"
```

### Supported inputs

| Input | Example | Generated branch |
|-------|---------|-----------------|
| `--linear` flag | _(fuzzy-select picker)_ | `eng-123-fix-login-bug` |
| Linear URL | `https://linear.app/team/issue/ENG-123/fix-login` | `eng-123-fix-login-bug` |
| Linear ID | `ENG-123` | `eng-123-fix-login-bug` |
| Sentry URL | `https://myorg.sentry.io/issues/12345/` | `sentry-12345-null-pointer-in-handler` |
| Plain text | `feature/auth` | `feature/auth` (unchanged) |

Branch names are slugified (lowercased, non-alphanumeric → hyphens, collapsed, max 60 chars). If `branch_prefix` is configured, it's automatically prepended (e.g. `richik/eng-123-fix-login-bug`).

**Note:** If a branch already exists in any selected repo, `sesh start` will reject it and re-prompt (interactive) or error (with `-b` flag).

## Prerequisites

- **git** — for worktree operations
- **code** (optional) — VS Code CLI for auto-opening windows
- **gh** (optional) — GitHub CLI for `sesh pr`

## License

MIT
