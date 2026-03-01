pub mod activate;
pub mod auth;
pub mod checkout;
pub mod completions;
pub mod doctor;
pub mod exec;
pub mod init;
pub mod list;
pub mod log;
pub mod pr;
pub mod resume;
pub mod start;
pub mod status;
pub mod stop;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use console::style;
use dialoguer::Select;

use crate::config::SeshConfig;
use crate::context;
use crate::discovery;
use crate::lock;
use crate::mcp;
use crate::scripts;
use crate::session::{self, BackgroundPid, IssueContext, SessionInfo, SessionRepo};
use crate::vscode;

/// Pick a session by name, or interactively if name is None.
pub fn pick_session(parent_dir: &Path, name: Option<String>) -> Result<SessionInfo> {
    let sessions = session::list_sessions(parent_dir)?;
    if sessions.is_empty() {
        bail!("No sessions found.");
    }

    match name {
        Some(n) => {
            sessions
                .into_iter()
                .find(|s| s.name == n)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found.", n))
        }
        None => {
            let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
            let selection = Select::new()
                .with_prompt("Select a session")
                .items(&names)
                .default(0)
                .interact()?;
            Ok(sessions.into_iter().nth(selection).unwrap())
        }
    }
}

/// Shared session finalization: save session, copy/symlink files, MCP config,
/// context generation, parent-dir copies, exclusive locks, setup scripts,
/// VS Code launch, and summary output.
pub fn finalize_session(
    parent_dir: &Path,
    config: &SeshConfig,
    selected_repos: &[discovery::RepoInfo],
    branch_name: &str,
    session_name: &str,
    sess_dir: &Path,
    issue_context: Option<IssueContext>,
    effective_base: &str,
    no_setup: bool,
    no_vscode: bool,
) -> Result<()> {
    // Save session early so `sesh stop` can always find it for cleanup
    let session_info = SessionInfo {
        name: session_name.to_string(),
        branch: branch_name.to_string(),
        repos: selected_repos
            .iter()
            .map(|r| SessionRepo {
                name: r.name.clone(),
                worktree_path: sess_dir.join(&r.name),
                original_repo_path: r.path.clone(),
            })
            .collect(),
        created_at: Utc::now(),
        parent_dir: parent_dir.to_path_buf(),
        issue: issue_context,
        base_branch: Some(effective_base.to_string()),
    };

    session::save_session(sess_dir, &session_info)?;

    // Copy/symlink per-repo files
    for repo in selected_repos {
        if let Some(repo_config) = config.repos.get(&repo.name) {
            let worktree_path = sess_dir.join(&repo.name);

            // Copy files
            for file in &repo_config.copy {
                let src = repo.path.join(file);
                let dst = worktree_path.join(file);
                if src.exists() {
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).ok();
                    }
                    if let Err(e) = std::fs::copy(&src, &dst) {
                        eprintln!(
                            "  {} Failed to copy {} in {}: {}",
                            style("!").yellow(),
                            file,
                            repo.name,
                            e
                        );
                    } else {
                        println!("  {} Copied {} → {}", style("·").dim(), file, repo.name);
                    }
                }
            }

            // Symlink files/dirs
            for item in &repo_config.symlink {
                let src = repo.path.join(item);
                let dst = worktree_path.join(item);
                if src.exists() && !dst.exists() {
                    if let Err(e) = std::os::unix::fs::symlink(&src, &dst) {
                        eprintln!(
                            "  {} Failed to symlink {} in {}: {}",
                            style("!").yellow(),
                            item,
                            repo.name,
                            e
                        );
                    } else {
                        println!("  {} Symlinked {} → {}", style("·").dim(), item, repo.name);
                    }
                }
            }
        }
    }

    // Write .mcp.json per worktree
    let servers = &config.mcp.servers;
    if !servers.is_empty() {
        for repo in selected_repos {
            let worktree_path = sess_dir.join(&repo.name);
            mcp::write_mcp_config(&worktree_path, &repo.path, servers)
                .with_context(|| format!("failed to write .mcp.json for {}", repo.name))?;
        }
        println!(
            "  {} MCP config written ({} server(s))",
            style("✓").green(),
            servers.len()
        );
    }

    // Generate context
    let repo_pairs: Vec<(String, PathBuf)> = selected_repos
        .iter()
        .map(|r| (r.name.clone(), sess_dir.join(&r.name)))
        .collect();

    context::generate_context(
        sess_dir,
        branch_name,
        &repo_pairs,
        &config.session.shared_context,
        parent_dir,
        session_info.issue.as_ref(),
        Some(effective_base),
    )?;
    println!("  {} Session context generated", style("✓").green());

    // Copy parent-dir files into session directory
    if !config.session.copy.is_empty() {
        for file in &config.session.copy {
            let src = parent_dir.join(file);
            let dst = sess_dir.join(file);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                if src.is_dir() {
                    if let Err(e) = copy_dir_recursive(&src, &dst) {
                        eprintln!(
                            "  {} Failed to copy dir {} to session: {}",
                            style("!").yellow(),
                            file,
                            e
                        );
                    } else {
                        println!("  {} Copied {} → session", style("·").dim(), file);
                    }
                } else if let Err(e) = std::fs::copy(&src, &dst) {
                    eprintln!(
                        "  {} Failed to copy {} to session: {}",
                        style("!").yellow(),
                        file,
                        e
                    );
                } else {
                    println!("  {} Copied {} → session", style("·").dim(), file);
                }
            }
        }
    }

    // Acquire exclusive locks
    let mut exclusive_skipped: Vec<String> = Vec::new();
    for repo in selected_repos {
        let is_exclusive = config
            .repos
            .get(&repo.name)
            .map(|rc| rc.exclusive)
            .unwrap_or(false);
        if !is_exclusive {
            continue;
        }

        match lock::check_lock(parent_dir, &repo.name)? {
            None => {
                lock::acquire_lock(parent_dir, &repo.name, session_name)?;
                println!(
                    "  {} Exclusive lock acquired: {}",
                    style("✓").green(),
                    repo.name
                );
            }
            Some(lock_info) => {
                if session::session_exists(parent_dir, &lock_info.session) {
                    println!(
                        "  {} Exclusive repo '{}' is locked by session '{}' — skipping services",
                        style("!").yellow(),
                        repo.name,
                        lock_info.session
                    );
                    exclusive_skipped.push(repo.name.clone());
                } else {
                    lock::acquire_lock(parent_dir, &repo.name, session_name)?;
                    println!(
                        "  {} Stale lock for '{}' reclaimed (session '{}' gone)",
                        style("✓").green(),
                        repo.name,
                        lock_info.session
                    );
                }
            }
        }
    }

    // Run setup scripts
    if !no_setup {
        let repo_names: Vec<String> = selected_repos.iter().map(|r| r.name.clone()).collect();
        let mut bg_pids: Vec<BackgroundPid> = Vec::new();
        let log_dir = sess_dir.join("logs");

        let exclusive_skip_csv = exclusive_skipped.join(",");

        // Global setup scripts
        for entry in &config.scripts.setup {
            let script_path = parent_dir.join(&entry.path);
            let extra_env: Vec<(&str, &str)> = if !exclusive_skipped.is_empty() {
                vec![("SESH_EXCLUSIVE_SKIP", exclusive_skip_csv.as_str())]
            } else {
                vec![]
            };

            if entry.background {
                let label = format!("global-setup-{}", sanitize_label(&entry.path));
                println!(
                    "  {} Spawning background: {}...",
                    style("→").cyan(),
                    entry.path
                );
                let pid = scripts::spawn_background_script(
                    entry,
                    &script_path,
                    sess_dir,
                    &log_dir,
                    &label,
                    session_name,
                    branch_name,
                    &repo_names,
                    &extra_env,
                )?;
                bg_pids.push(BackgroundPid {
                    pid,
                    label: label.clone(),
                    script: entry.path.clone(),
                });
                println!(
                    "  {} Background PID {} ({})",
                    style("✓").green(),
                    pid,
                    entry.path
                );
            } else {
                println!(
                    "\n  {} Running setup: {}...",
                    style("→").cyan(),
                    entry.path
                );
                scripts::run_script_entry(
                    "setup",
                    entry,
                    &script_path,
                    sess_dir,
                    session_name,
                    branch_name,
                    &repo_names,
                    &extra_env,
                )?;
            }
        }

        // Per-repo setup scripts
        for repo in selected_repos {
            if let Some(repo_config) = config.repos.get(&repo.name) {
                let worktree_path = sess_dir.join(&repo.name);
                let repo_env_name = repo.name.clone();

                for entry in &repo_config.setup {
                    let script_path = parent_dir.join(&entry.path);
                    let extra_env: Vec<(&str, &str)> =
                        vec![("SESH_REPO", repo_env_name.as_str())];

                    if entry.background {
                        let label =
                            format!("{}-setup-{}", repo.name, sanitize_label(&entry.path));
                        println!(
                            "  {} Spawning background for {}: {}...",
                            style("→").cyan(),
                            repo.name,
                            entry.path
                        );
                        let pid = scripts::spawn_background_script(
                            entry,
                            &script_path,
                            &worktree_path,
                            &log_dir,
                            &label,
                            session_name,
                            branch_name,
                            &repo_names,
                            &extra_env,
                        )?;
                        bg_pids.push(BackgroundPid {
                            pid,
                            label: label.clone(),
                            script: entry.path.clone(),
                        });
                        println!(
                            "  {} Background PID {} ({}/{})",
                            style("✓").green(),
                            pid,
                            repo.name,
                            entry.path
                        );
                    } else {
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
                            &worktree_path,
                            session_name,
                            branch_name,
                            &repo_names,
                            &extra_env,
                        )?;
                    }
                }
            }
        }

        // Save background PIDs
        if !bg_pids.is_empty() {
            session::save_background_pids(sess_dir, &bg_pids)?;
            println!(
                "  {} {} background process(es) started",
                style("✓").green(),
                bg_pids.len()
            );
        }
    }

    // Open VS Code
    if !no_vscode {
        let paths: Vec<PathBuf> = selected_repos
            .iter()
            .map(|r| sess_dir.join(&r.name))
            .collect();
        vscode::open_session_in_vscode(sess_dir, &paths)?;
    }

    // Summary
    println!(
        "\n{}",
        style("Session created successfully!").green().bold()
    );
    println!();
    println!(
        "  {:<16} {}",
        style("Session:").bold(),
        session_name
    );
    println!(
        "  {:<16} {}",
        style("Branch:").bold(),
        branch_name
    );
    println!(
        "  {:<16} {}",
        style("Location:").bold(),
        sess_dir.display()
    );
    println!();
    for repo in selected_repos {
        println!(
            "  {} {} → {}",
            style("•").dim(),
            style(&repo.name).cyan(),
            sess_dir.join(&repo.name).display()
        );
    }
    println!();

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn sanitize_label(path: &str) -> String {
    path.replace('/', "-")
        .replace('\\', "-")
        .trim_start_matches(['.', '-'])
        .trim_end_matches(".sh")
        .to_string()
}
