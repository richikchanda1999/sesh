use std::path::Path;

use anyhow::Result;
use console::style;
use dialoguer::Confirm;

use crate::discovery;
use crate::lock;
use crate::session;
use crate::worktree;

pub fn run(parent_dir: &Path) -> Result<()> {
    println!("{} Running diagnostics...\n", style("🔍").bold());

    let mut issues = Vec::new();

    // Check sessions
    let sessions = session::list_sessions(parent_dir)?;
    println!("  Sessions found: {}", sessions.len());

    for sess in &sessions {
        for repo in &sess.repos {
            if !repo.worktree_path.exists() {
                issues.push(format!(
                    "Session '{}': worktree for '{}' missing at {}",
                    sess.name,
                    repo.name,
                    repo.worktree_path.display()
                ));
            }
        }
    }

    // Check for orphaned worktrees in discovered repos
    let repos = discovery::discover_repos(parent_dir).unwrap_or_default();
    let sesh_dir = parent_dir.join(".sesh");

    for repo in &repos {
        if let Ok(worktrees) = worktree::get_worktree_list(&repo.path) {
            for wt_path in &worktrees {
                // If worktree is under .sesh/ but no session owns it
                if wt_path.starts_with(&sesh_dir.to_string_lossy().as_ref()) {
                    let owned = sessions.iter().any(|s| {
                        s.repos.iter().any(|r| r.worktree_path.to_string_lossy() == *wt_path)
                    });
                    if !owned {
                        issues.push(format!(
                            "Orphaned worktree for '{}': {}",
                            repo.name, wt_path
                        ));
                    }
                }
            }
        }
    }

    // Check for stale session dirs (no session.json)
    let sessions_dir = parent_dir.join(".sesh/sessions");
    if sessions_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.join("session.json").exists() {
                    issues.push(format!(
                        "Stale session directory (no session.json): {}",
                        path.display()
                    ));
                }
            }
        }
    }

    // Check for stale locks (pointing to sessions that no longer exist)
    let mut stale_locks = Vec::new();
    if let Ok(locks) = lock::list_locks(parent_dir) {
        for (repo_name, lock_info) in &locks {
            if !session::session_exists(parent_dir, &lock_info.session) {
                issues.push(format!(
                    "Stale lock for repo '{}' (session '{}' no longer exists)",
                    repo_name, lock_info.session
                ));
                stale_locks.push(repo_name.clone());
            }
        }
    }

    if issues.is_empty() {
        println!("\n  {} No issues found. Everything looks good!", style("✔").green());
        return Ok(());
    }

    println!("\n  {} Found {} issue(s):\n", style("!").yellow(), issues.len());
    for (i, issue) in issues.iter().enumerate() {
        println!("  {}. {}", i + 1, issue);
    }

    let fix = Confirm::new()
        .with_prompt("\nAttempt to fix issues?")
        .default(false)
        .interact()?;

    if !fix {
        return Ok(());
    }

    // Fix: prune worktrees for all repos
    for repo in &repos {
        if let Err(e) = worktree::prune_worktrees(&repo.path) {
            eprintln!("  Warning: failed to prune worktrees for {}: {}", repo.name, e);
        }
    }

    // Fix: remove stale session dirs
    if sessions_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.join("session.json").exists() {
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        eprintln!("  Warning: failed to remove {}: {}", path.display(), e);
                    } else {
                        println!("  Removed stale dir: {}", path.display());
                    }
                }
            }
        }
    }

    // Fix: remove stale locks
    for repo_name in &stale_locks {
        if let Err(e) = lock::release_lock(parent_dir, repo_name) {
            eprintln!("  Warning: failed to remove stale lock for {}: {}", repo_name, e);
        } else {
            println!("  Removed stale lock: {}", repo_name);
        }
    }

    println!("\n  {} Cleanup complete.", style("✔").green());

    Ok(())
}
