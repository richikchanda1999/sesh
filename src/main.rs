mod cli;
mod commands;
mod config;
mod context;
mod discovery;
mod integrations;
mod lock;
mod mcp;
mod scripts;
mod session;
mod vscode;
mod worktree;

use std::env;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let parent_dir = cli.dir.unwrap_or_else(|| env::current_dir().expect("cannot determine current directory"));

    match cli.command {
        Command::Start { branch, from, all, preset, no_setup, no_vscode, linear } => {
            commands::start::run(&parent_dir, branch, from, all, preset, no_setup, no_vscode, linear).await
        }
        Command::List { active } => commands::list::run(&parent_dir, active),
        Command::Stop { name, keep_branches } => commands::stop::run(&parent_dir, name, keep_branches),
        Command::Resume { name } => commands::resume::run(&parent_dir, name),
        Command::Status { name } => commands::status::run(&parent_dir, name),
        Command::Pr { name, base } => commands::pr::run(&parent_dir, name, base),
        Command::Checkout { branch, pr, all, preset, no_setup, no_vscode } => {
            commands::checkout::run(&parent_dir, branch, pr, all, preset, no_setup, no_vscode).await
        }
        Command::Init => commands::init::run(&parent_dir),
        Command::Doctor => commands::doctor::run(&parent_dir),
        Command::Activate { name } => commands::activate::run(&parent_dir, name),
        Command::Log { session, script, follow } => {
            commands::log::run(&parent_dir, session, script, follow)
        }
        Command::Exec { session, command } => {
            commands::exec::run(&parent_dir, session, &command)
        }
        Command::Completions { shell } => {
            commands::completions::run(shell);
            Ok(())
        }
        Command::Auth { provider } => {
            let provider_name = match provider {
                cli::AuthProvider::Linear => "linear",
                cli::AuthProvider::Sentry => "sentry",
            };
            commands::auth::run(&parent_dir, provider_name)
        }
    }
}
