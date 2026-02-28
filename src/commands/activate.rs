use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::config::SeshConfig;
use crate::lock;
use crate::scripts;
use crate::session;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>) -> Result<()> {
    let config_path = parent_dir.join("sesh.toml");
    let config = SeshConfig::load(&config_path)?;

    let target_session = pick_session(parent_dir, name)?;
    let target_dir = session::session_dir(parent_dir, &target_session.name);

    // Find exclusive repos in the target session
    let exclusive_repos: Vec<&str> = target_session
        .repos
        .iter()
        .filter(|r| {
            config
                .repos
                .get(&r.name)
                .map(|rc| rc.exclusive)
                .unwrap_or(false)
        })
        .map(|r| r.name.as_str())
        .collect();

    if exclusive_repos.is_empty() {
        bail!("Session '{}' has no exclusive repos to activate.", target_session.name);
    }

    // For each exclusive repo, check who currently holds the lock
    let mut transfers: Vec<(String, String)> = Vec::new(); // (repo_name, old_session_name)

    for &repo_name in &exclusive_repos {
        if let Some(lock_info) = lock::check_lock(parent_dir, repo_name)? {
            if lock_info.session == target_session.name {
                println!(
                    "  {} '{}' already locked by session '{}'",
                    style("·").dim(),
                    repo_name,
                    target_session.name
                );
                continue;
            }

            // Check if the holding session still exists
            if session::session_exists(parent_dir, &lock_info.session) {
                transfers.push((repo_name.to_string(), lock_info.session.clone()));
            } else {
                // Stale lock, just acquire
                println!(
                    "  {} Stale lock for '{}' (session '{}' gone), acquiring",
                    style("!").yellow(),
                    repo_name,
                    lock_info.session
                );
            }
        }

        // Acquire lock for target session
        lock::acquire_lock(parent_dir, repo_name, &target_session.name)?;
        println!(
            "  {} Lock acquired: {} → {}",
            style("✓").green(),
            repo_name,
            target_session.name
        );
    }

    // Run teardown for old sessions that lost locks
    let mut teardown_sessions: Vec<String> = transfers.iter().map(|(_, s)| s.clone()).collect();
    teardown_sessions.sort();
    teardown_sessions.dedup();

    for old_session_name in &teardown_sessions {
        if let Ok(old_session) = session::load_session(
            &session::session_dir(parent_dir, old_session_name),
        ) {
            let old_dir = session::session_dir(parent_dir, old_session_name);
            let repo_names: Vec<String> =
                old_session.repos.iter().map(|r| r.name.clone()).collect();

            // Kill background processes for old session
            let bg_pids = session::load_background_pids(&old_dir);
            if !bg_pids.is_empty() {
                println!(
                    "\n  {} Killing {} background process(es) for '{}'...",
                    style("→").cyan(),
                    bg_pids.len(),
                    old_session_name
                );
                scripts::kill_background_pids(&bg_pids);
            }

            // Per-repo teardown scripts for old session
            for repo in &old_session.repos {
                if let Some(repo_config) = config.repos.get(&repo.name) {
                    for entry in &repo_config.teardown {
                        let script_path = parent_dir.join(&entry.path);
                        if script_path.exists() {
                            println!(
                                "  {} Running teardown for {}: {}...",
                                style("→").cyan(),
                                repo.name,
                                entry.path
                            );
                            if let Err(e) = scripts::run_script_entry(
                                "teardown",
                                entry,
                                &script_path,
                                &repo.worktree_path,
                                &old_session.name,
                                &old_session.branch,
                                &repo_names,
                                &[("SESH_REPO", repo.name.as_str())],
                            ) {
                                eprintln!(
                                    "  {} Teardown '{}' for {} failed: {}",
                                    style("!").yellow(),
                                    entry.path,
                                    repo.name,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            // Global teardown scripts for old session
            for entry in &config.scripts.teardown {
                let script_path = parent_dir.join(&entry.path);
                if script_path.exists() {
                    println!(
                        "\n  {} Running teardown for session '{}': {}...",
                        style("→").cyan(),
                        old_session_name,
                        entry.path
                    );
                    if let Err(e) = scripts::run_script_entry(
                        "teardown",
                        entry,
                        &script_path,
                        &old_dir,
                        &old_session.name,
                        &old_session.branch,
                        &repo_names,
                        &[],
                    ) {
                        eprintln!(
                            "  {} Teardown '{}' failed for '{}': {}",
                            style("!").yellow(),
                            entry.path,
                            old_session_name,
                            e
                        );
                    }
                }
            }
        }
    }

    // Run setup for the target session
    let repo_names: Vec<String> =
        target_session.repos.iter().map(|r| r.name.clone()).collect();

    // Global setup scripts
    for entry in &config.scripts.setup {
        let script_path = parent_dir.join(&entry.path);
        if script_path.exists() {
            println!(
                "\n  {} Running setup for session '{}': {}...",
                style("→").cyan(),
                target_session.name,
                entry.path
            );
            scripts::run_script_entry(
                "setup",
                entry,
                &script_path,
                &target_dir,
                &target_session.name,
                &target_session.branch,
                &repo_names,
                &[],
            )?;
        }
    }

    // Per-repo setup scripts
    for repo in &target_session.repos {
        if let Some(repo_config) = config.repos.get(&repo.name) {
            for entry in &repo_config.setup {
                let script_path = parent_dir.join(&entry.path);
                if script_path.exists() {
                    println!(
                        "  {} Running setup for {}: {}...",
                        style("→").cyan(),
                        repo.name,
                        entry.path
                    );
                    scripts::run_script_entry(
                        "setup",
                        entry,
                        &script_path,
                        &repo.worktree_path,
                        &target_session.name,
                        &target_session.branch,
                        &repo_names,
                        &[("SESH_REPO", repo.name.as_str())],
                    )?;
                }
            }
        }
    }

    println!(
        "\n{} Session '{}' is now active.",
        style("✔").green(),
        target_session.name
    );

    Ok(())
}
