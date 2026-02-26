use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use console::style;
use dialoguer::{Input, MultiSelect};

use crate::config::SeshConfig;
use crate::context;
use crate::discovery;
use crate::mcp;
use crate::scripts;
use crate::session::{self, SessionInfo, SessionRepo};
use crate::vscode;
use crate::worktree;

pub fn run(
    parent_dir: &Path,
    branch: Option<String>,
    all: bool,
    preset: Option<String>,
    no_setup: bool,
    no_vscode: bool,
) -> Result<()> {
    // 1. Load config
    let config_path = parent_dir.join("sesh.toml");
    let config = SeshConfig::load(&config_path)?;

    // 2. Discover repos
    let repos = discovery::discover_repos(parent_dir)?;
    if repos.is_empty() {
        bail!("no git repos found in {}", parent_dir.display());
    }

    // 3. Select repos
    let selected_repos = if all {
        repos.clone()
    } else if let Some(ref preset_name) = preset {
        let preset_repos = config.presets.get(preset_name)
            .with_context(|| format!("preset '{}' not found in sesh.toml", preset_name))?;
        repos.iter()
            .filter(|r| preset_repos.contains(&r.name))
            .cloned()
            .collect()
    } else {
        select_repos_interactive(&repos, &config)?
    };

    if selected_repos.is_empty() {
        bail!("no repos selected");
    }

    // 4. Get branch name
    let branch_name = match branch {
        Some(b) => b,
        None => prompt_branch_name()?,
    };

    worktree::validate_branch_name(&branch_name)
        .with_context(|| format!("'{}' is not a valid git branch name", branch_name))?;

    // Check session doesn't already exist
    if session::session_exists(parent_dir, &branch_name) {
        bail!("session '{}' already exists. Use `sesh stop {}` first or choose a different name.", branch_name, branch_name);
    }

    let sess_dir = session::session_dir(parent_dir, &branch_name);

    println!(
        "\n{} Creating session {} with {} repo(s)...\n",
        style("→").cyan().bold(),
        style(&branch_name).green().bold(),
        selected_repos.len()
    );

    // 5. Per-repo: validate base branch, fetch, create worktree
    let mut created_worktrees: Vec<(PathBuf, PathBuf)> = Vec::new(); // (repo_path, worktree_path)

    for repo in &selected_repos {
        let repo_config = config.repos.get(&repo.name);
        let base_branch = repo_config
            .and_then(|rc| rc.base_branch.as_deref())
            .unwrap_or(&config.session.base_branch);

        let worktree_path = sess_dir.join(&repo.name);
        let base_ref = format!("origin/{}", base_branch);

        // Fetch
        print!("  {} Fetching {}/{}...", style("↓").dim(), repo.name, base_branch);
        if let Err(e) = worktree::fetch_branch(&repo.path, "origin", base_branch) {
            println!(" {}", style("warning: fetch failed, continuing").yellow());
            eprintln!("    {}", e);
        } else {
            println!(" {}", style("done").green());
        }

        // Check if branch already exists
        if worktree::branch_exists(&repo.path, &branch_name)? {
            println!(
                "  {} Branch '{}' already exists in {}; will use existing branch",
                style("!").yellow(),
                branch_name,
                repo.name
            );
            // Use existing branch: worktree add <path> <existing-branch>
            let wt = worktree_path.to_string_lossy();
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo.path)
                .args(["worktree", "add", &wt, &branch_name])
                .output()
                .context("failed to run git worktree add")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                rollback_worktrees(&created_worktrees);
                bail!(
                    "failed to create worktree for '{}': {}",
                    repo.name,
                    stderr.trim()
                );
            }
        } else {
            // Create worktree with new branch
            if let Err(e) = worktree::create_worktree(&repo.path, &worktree_path, &branch_name, &base_ref) {
                rollback_worktrees(&created_worktrees);
                return Err(e.context(format!("failed while setting up repo '{}'", repo.name)));
            }
        }

        created_worktrees.push((repo.path.clone(), worktree_path.clone()));
        println!("  {} Worktree created: {}", style("✓").green(), repo.name);
    }

    // 6. Copy/symlink per-repo files
    for repo in &selected_repos {
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

    // 7. Write .mcp.json per worktree
    let servers = &config.mcp.servers;
    if !servers.is_empty() {
        for repo in &selected_repos {
            let worktree_path = sess_dir.join(&repo.name);
            mcp::write_mcp_config(&worktree_path, servers)
                .with_context(|| format!("failed to write .mcp.json for {}", repo.name))?;
        }
        println!("  {} MCP config written ({} server(s))", style("✓").green(), servers.len());
    }

    // 8. Generate context
    let repo_pairs: Vec<(String, PathBuf)> = selected_repos
        .iter()
        .map(|r| (r.name.clone(), sess_dir.join(&r.name)))
        .collect();

    context::generate_context(
        &sess_dir,
        &branch_name,
        &repo_pairs,
        &config.session.shared_context,
        parent_dir,
    )?;
    println!("  {} Session context generated", style("✓").green());

    // 9. Run setup script
    if !no_setup {
        if let Some(ref setup_script) = config.scripts.setup {
            let script_path = parent_dir.join(setup_script);
            let repo_names: Vec<String> = selected_repos.iter().map(|r| r.name.clone()).collect();
            println!("\n  {} Running setup script...", style("→").cyan());
            scripts::run_setup_script(&script_path, &sess_dir, &branch_name, &repo_names)?;
        }
    }

    // 10. Save session
    let session_info = SessionInfo {
        name: branch_name.clone(),
        branch: branch_name.clone(),
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
    };

    session::save_session(&sess_dir, &session_info)?;

    // 11. Open VS Code
    if !no_vscode {
        let paths: Vec<PathBuf> = selected_repos
            .iter()
            .map(|r| sess_dir.join(&r.name))
            .collect();
        vscode::open_in_vscode(&paths)?;
    }

    // 12. Summary
    println!("\n{}", style("Session created successfully!").green().bold());
    println!();
    println!(
        "  {:<16} {}",
        style("Session:").bold(),
        branch_name
    );
    println!(
        "  {:<16} {}",
        style("Location:").bold(),
        sess_dir.display()
    );
    println!();
    for repo in &selected_repos {
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

fn select_repos_interactive(
    repos: &[discovery::RepoInfo],
    config: &SeshConfig,
) -> Result<Vec<discovery::RepoInfo>> {
    let labels: Vec<String> = repos
        .iter()
        .map(|r| {
            let branch = if r.current_branch.is_empty() {
                "detached".to_string()
            } else {
                r.current_branch.clone()
            };
            let dirty = if r.is_dirty { " *" } else { "" };
            format!("{} ({}{})", r.name, branch, dirty)
        })
        .collect();

    // Pre-select repos not marked as skip
    let defaults: Vec<bool> = repos
        .iter()
        .map(|r| {
            config
                .repos
                .get(&r.name)
                .map(|rc| !rc.skip)
                .unwrap_or(true)
        })
        .collect();

    let selections = MultiSelect::new()
        .with_prompt("Select repos for this session")
        .items(&labels)
        .defaults(&defaults)
        .interact()
        .context("repo selection cancelled")?;

    Ok(selections.into_iter().map(|i| repos[i].clone()).collect())
}

fn prompt_branch_name() -> Result<String> {
    let name: String = Input::new()
        .with_prompt("Branch name")
        .interact_text()
        .context("branch name input cancelled")?;

    Ok(name.trim().to_string())
}

fn rollback_worktrees(created: &[(PathBuf, PathBuf)]) {
    eprintln!("\n  {} Rolling back created worktrees...", style("✗").red());
    for (repo_path, worktree_path) in created.iter().rev() {
        if let Err(e) = worktree::remove_worktree(repo_path, worktree_path) {
            eprintln!("    Failed to remove worktree {}: {}", worktree_path.display(), e);
        }
    }
}
