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

    if let Some(ref teardown) = config.scripts.teardown {
        let script_path = parent_dir.join(teardown);
        if script_path.exists() {
            for old_session_name in &teardown_sessions {
                if let Ok(old_session) = session::load_session(
                    &session::session_dir(parent_dir, old_session_name),
                ) {
                    let old_dir = session::session_dir(parent_dir, old_session_name);
                    let repo_names: Vec<String> =
                        old_session.repos.iter().map(|r| r.name.clone()).collect();
                    println!(
                        "\n  {} Running teardown for session '{}'...",
                        style("→").cyan(),
                        old_session_name
                    );
                    if let Err(e) = scripts::run_teardown_script(
                        &script_path,
                        &old_dir,
                        &old_session.branch,
                        &repo_names,
                    ) {
                        eprintln!(
                            "  {} Teardown failed for '{}': {}",
                            style("!").yellow(),
                            old_session_name,
                            e
                        );
                    }
                }
            }
        }
    }

    // Run setup for the target session
    if let Some(ref setup) = config.scripts.setup {
        let script_path = parent_dir.join(setup);
        if script_path.exists() {
            let repo_names: Vec<String> =
                target_session.repos.iter().map(|r| r.name.clone()).collect();
            println!(
                "\n  {} Running setup for session '{}'...",
                style("→").cyan(),
                target_session.name
            );
            scripts::run_setup_script(
                &script_path,
                &target_dir,
                &target_session.branch,
                &repo_names,
            )?;
        }
    }

    println!(
        "\n{} Session '{}' is now active.",
        style("✔").green(),
        target_session.name
    );

    Ok(())
}
