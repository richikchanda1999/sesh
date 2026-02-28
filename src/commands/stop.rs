use std::path::Path;

use anyhow::Result;
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

    // Kill background processes
    let bg_pids = session::load_background_pids(&session_dir);
    if !bg_pids.is_empty() {
        println!(
            "Killing {} background process(es)...",
            bg_pids.len()
        );
        scripts::kill_background_pids(&bg_pids);
    }

    // Run teardown scripts
    let config_path = parent_dir.join("sesh.toml");
    let config = SeshConfig::load(&config_path)?;
    let repo_names: Vec<String> = session.repos.iter().map(|r| r.name.clone()).collect();

    // Per-repo teardown scripts (run before global teardown)
    for repo in &session.repos {
        if let Some(repo_config) = config.repos.get(&repo.name) {
            for entry in &repo_config.teardown {
                let script_path = parent_dir.join(&entry.path);
                if script_path.exists() {
                    println!(
                        "Running teardown for {}: {}...",
                        style(&repo.name).cyan(),
                        entry.path
                    );
                    if let Err(e) = scripts::run_script_entry(
                        "teardown",
                        entry,
                        &script_path,
                        &repo.worktree_path,
                        &session.name,
                        &session.branch,
                        &repo_names,
                        &[("SESH_REPO", repo.name.as_str())],
                    ) {
                        eprintln!(
                            "  Warning: teardown script '{}' for {} failed: {}",
                            entry.path, repo.name, e
                        );
                    }
                }
            }
        }
    }

    // Global teardown scripts
    for entry in &config.scripts.teardown {
        let script_path = parent_dir.join(&entry.path);
        if script_path.exists() {
            println!("Running teardown: {}...", entry.path);
            if let Err(e) = scripts::run_script_entry(
                "teardown",
                entry,
                &script_path,
                &session_dir,
                &session.name,
                &session.branch,
                &repo_names,
                &[],
            ) {
                eprintln!("  Warning: teardown script '{}' failed: {}", entry.path, e);
            }
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
