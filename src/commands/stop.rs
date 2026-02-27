use std::path::Path;

use anyhow::{Context, Result};
use console::style;

use crate::config::SeshConfig;
use crate::lock;
use crate::scripts;
use crate::session;
use crate::worktree;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>, keep_branches: bool) -> Result<()> {
    let session = pick_session(parent_dir, name)?;
    let session_dir = session::session_dir(parent_dir, &session.name);

    // Run teardown script if configured
    let config_path = parent_dir.join("sesh.toml");
    let config = SeshConfig::load(&config_path)?;
    if let Some(ref teardown) = config.scripts.teardown {
        let script_path = parent_dir.join(teardown);
        let repo_names: Vec<String> = session.repos.iter().map(|r| r.name.clone()).collect();
        if script_path.exists() {
            println!("Running teardown script...");
            scripts::run_teardown_script(&script_path, &session_dir, &session.branch, &repo_names)
                .context("Teardown script failed")?;
        }
    }

    // Remove worktrees
    for repo in &session.repos {
        println!("Removing worktree for {}...", style(&repo.name).cyan());
        if let Err(e) = worktree::remove_worktree(&repo.original_repo_path, &repo.worktree_path) {
            eprintln!("  Warning: failed to remove worktree for {}: {}", repo.name, e);
        }
        if let Err(e) = worktree::prune_worktrees(&repo.original_repo_path) {
            eprintln!("  Warning: failed to prune worktrees for {}: {}", repo.name, e);
        }
    }

    // Delete branches unless --keep-branches
    if !keep_branches {
        for repo in &session.repos {
            if let Err(e) = worktree::delete_branch(&repo.original_repo_path, &session.branch) {
                eprintln!("  Warning: failed to delete branch '{}' in {}: {}", session.branch, repo.name, e);
            }
        }
    }

    // Release exclusive locks held by this session
    for repo in &session.repos {
        let is_exclusive = config
            .repos
            .get(&repo.name)
            .map(|rc| rc.exclusive)
            .unwrap_or(false);
        if is_exclusive {
            if let Ok(Some(lock_info)) = lock::check_lock(parent_dir, &repo.name) {
                if lock_info.session == session.name {
                    if let Err(e) = lock::release_lock(parent_dir, &repo.name) {
                        eprintln!("  Warning: failed to release lock for {}: {}", repo.name, e);
                    }
                }
            }
        }
    }

    // Remove session directory
    session::delete_session_dir(&session_dir)?;

    println!(
        "{} Session '{}' stopped and cleaned up.",
        style("✔").green(),
        session.name,
    );

    Ok(())
}
