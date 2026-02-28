use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sesh", about = "Multi-repo worktree session manager for AI-assisted development")]
pub struct Cli {
    /// Path to the parent directory containing repos (defaults to current dir)
    #[arg(short, long, global = true)]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new worktree session
    Start {
        /// Branch name for the session
        #[arg(short, long)]
        branch: Option<String>,

        /// Base branch to create worktrees from (overrides sesh.toml for this session)
        #[arg(long)]
        from: Option<String>,

        /// Include all discovered repos (skip interactive selection)
        #[arg(long)]
        all: bool,

        /// Use a preset from sesh.toml
        #[arg(long)]
        preset: Option<String>,

        /// Skip running setup scripts
        #[arg(long)]
        no_setup: bool,

        /// Don't open VS Code
        #[arg(long)]
        no_vscode: bool,

        /// Pick a branch from your Linear tickets
        #[arg(long)]
        linear: bool,
    },

    /// List sessions
    List {
        /// Show only sessions with existing worktrees
        #[arg(long)]
        active: bool,
    },

    /// Stop and clean up a session
    Stop {
        /// Session name (interactive if omitted)
        name: Option<String>,

        /// Keep branches after removing worktrees
        #[arg(long)]
        keep_branches: bool,
    },

    /// Re-open VS Code windows for a session
    Resume {
        /// Session name (interactive if omitted)
        name: Option<String>,
    },

    /// Show git status per repo in a session
    Status {
        /// Session name (interactive if omitted)
        name: Option<String>,
    },

    /// Push branches and create PRs
    Pr {
        /// Session name (interactive if omitted)
        name: Option<String>,

        /// Base branch for PRs
        #[arg(long, default_value = "main")]
        base: String,
    },

    /// Generate sesh.toml interactively
    Init,

    /// Detect and fix orphaned worktrees/sessions
    Doctor,

    /// Transfer exclusive locks to a session (runs teardown/setup scripts)
    Activate {
        /// Session name (interactive if omitted)
        name: Option<String>,
    },

    /// Configure API tokens for integrations (Linear, Sentry)
    Auth {
        #[command(subcommand)]
        provider: AuthProvider,
    },

    /// View background script logs
    Log {
        /// Session name (interactive if omitted)
        #[arg(short, long)]
        session: Option<String>,

        /// Script label to view (lists available if omitted)
        script: Option<String>,

        /// Follow the log output (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },

    /// Run a command in each repo's worktree
    Exec {
        /// Session name (interactive if omitted)
        #[arg(short, long)]
        session: Option<String>,

        /// Command to execute in each repo's worktree
        command: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum AuthProvider {
    /// Set your Linear API token
    Linear,
    /// Set your Sentry auth token
    Sentry,
}
