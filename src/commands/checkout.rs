use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::{Confirm, FuzzySelect, MultiSelect};
use serde::Deserialize;

use crate::config::SeshConfig;
use crate::discovery;
use crate::session;
use crate::worktree;

pub async fn run(
    parent_dir: &Path,
    branch_mode: bool,
    pr_mode: bool,
    all: bool,
    preset: Option<String>,
    no_setup: bool,
    no_vscode: bool,
) -> Result<()> {
    if !branch_mode && !pr_mode {
        bail!("specify either --branch or --pr");
    }

    // Load config
    let config_path = parent_dir.join("sesh.toml");
    let config = SeshConfig::load(&config_path)?;

    // Discover repos
    let repos = discovery::discover_repos(parent_dir)?;
    if repos.is_empty() {
        bail!("no git repos found in {}", parent_dir.display());
    }

    // Select repos
    let selected_repos = if all {
        repos.clone()
    } else if let Some(ref preset_name) = preset {
        let preset_repos = config
            .presets
            .get(preset_name)
            .with_context(|| format!("preset '{}' not found in sesh.toml", preset_name))?;
        repos
            .iter()
            .filter(|r| preset_repos.contains(&r.name))
            .cloned()
            .collect()
    } else {
        select_repos_interactive(&repos, &config)?
    };

    if selected_repos.is_empty() {
        bail!("no repos selected");
    }

    // Fetch all repos for fresh branch/PR data
    for repo in &selected_repos {
        print!(
            "  {} Fetching {}...",
            style("↓").dim(),
            repo.name
        );
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo.path)
            .args(["fetch", "--all", "--prune"])
            .output();
        match output {
            Ok(o) if o.status.success() => println!(" {}", style("done").green()),
            _ => println!(" {}", style("warning: fetch failed, continuing").yellow()),
        }
    }

    // Resolve branch name
    let branch_name = if branch_mode {
        pick_branch(&selected_repos)?
    } else {
        pick_pr_branch(&selected_repos)?
    };

    // Check for worktree conflicts
    match check_worktree_conflicts(parent_dir, &selected_repos, &branch_name)? {
        ConflictResult::OpenedExisting => return Ok(()),
        ConflictResult::NoConflict => {}
    }

    let session_name = session::sanitize_session_name(&branch_name, parent_dir);
    let sess_dir = session::session_dir(parent_dir, &session_name);

    println!(
        "\n{} Creating session {} (branch: {}) with {} repo(s)...\n",
        style("→").cyan().bold(),
        style(&session_name).green().bold(),
        style(&branch_name).cyan(),
        selected_repos.len()
    );

    // Create worktrees with mixed strategy
    let effective_base = &config.session.base_branch;
    let mut created_worktrees: Vec<(PathBuf, PathBuf)> = Vec::new();

    for repo in &selected_repos {
        let worktree_path = sess_dir.join(&repo.name);
        let has_local = worktree::branch_exists(&repo.path, &branch_name)?;
        let has_remote = worktree::remote_branch_exists(&repo.path, &branch_name)?;

        let result = if has_local || has_remote {
            // Existing branch — check out without -b
            worktree::checkout_existing_branch(&repo.path, &worktree_path, &branch_name)
        } else {
            // Branch doesn't exist in this repo — create new from base
            let repo_config = config.repos.get(&repo.name);
            let base_branch = repo_config
                .and_then(|rc| rc.base_branch.as_deref())
                .unwrap_or(effective_base);
            let base_ref = format!("origin/{}", base_branch);
            worktree::create_worktree(&repo.path, &worktree_path, &branch_name, &base_ref)
        };

        if let Err(e) = result {
            rollback_worktrees(&created_worktrees);
            return Err(e.context(format!("failed while setting up repo '{}'", repo.name)));
        }

        created_worktrees.push((repo.path.clone(), worktree_path.clone()));
        println!(
            "  {} Worktree created: {}{}",
            style("✓").green(),
            repo.name,
            if has_local || has_remote {
                ""
            } else {
                " (new branch)"
            }
        );
    }

    // Finalize session
    super::finalize_session(
        parent_dir,
        &config,
        &selected_repos,
        &branch_name,
        &session_name,
        &sess_dir,
        None,
        effective_base,
        no_setup,
        no_vscode,
    )?;

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

fn pick_branch(repos: &[discovery::RepoInfo]) -> Result<String> {
    let mut all_branches = BTreeSet::new();

    for repo in repos {
        let branches = worktree::list_all_branches(&repo.path)?;
        for b in branches {
            all_branches.insert(b);
        }
    }

    if all_branches.is_empty() {
        bail!("no branches found across selected repos");
    }

    let branch_list: Vec<String> = all_branches.into_iter().collect();

    let selection = FuzzySelect::new()
        .with_prompt("Select a branch")
        .items(&branch_list)
        .default(0)
        .interact()
        .context("branch selection cancelled")?;

    Ok(branch_list[selection].clone())
}

#[derive(Debug, Deserialize)]
struct GhPr {
    number: u64,
    title: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
}

struct PrDisplayItem {
    repo_name: String,
    number: u64,
    title: String,
    branch: String,
}

fn pick_pr_branch(repos: &[discovery::RepoInfo]) -> Result<String> {
    // Check gh is available
    let gh_check = Command::new("which").arg("gh").output();
    match gh_check {
        Ok(output) if !output.status.success() => {
            bail!("GitHub CLI (gh) not found. Install it from https://cli.github.com")
        }
        Err(_) => bail!("GitHub CLI (gh) not found. Install it from https://cli.github.com"),
        _ => {}
    }

    let mut pr_items: Vec<PrDisplayItem> = Vec::new();

    for repo in repos {
        let output = Command::new("gh")
            .args([
                "pr", "list",
                "--json", "number,title,headRefName",
                "--state", "open",
            ])
            .current_dir(&repo.path)
            .output()
            .with_context(|| format!("failed to run gh pr list in {}", repo.name))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "  {} Failed to list PRs for {}: {}",
                style("!").yellow(),
                repo.name,
                stderr.trim()
            );
            continue;
        }

        let prs: Vec<GhPr> = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("failed to parse PR list for {}", repo.name))?;

        for pr in prs {
            pr_items.push(PrDisplayItem {
                repo_name: repo.name.clone(),
                number: pr.number,
                title: pr.title,
                branch: pr.head_ref_name,
            });
        }
    }

    if pr_items.is_empty() {
        bail!("no open PRs found across selected repos");
    }

    let labels: Vec<String> = pr_items
        .iter()
        .map(|p| {
            format!(
                "{}: #{} {} ({})",
                p.repo_name, p.number, p.title, p.branch
            )
        })
        .collect();

    let selection = FuzzySelect::new()
        .with_prompt("Select a PR")
        .items(&labels)
        .default(0)
        .interact()
        .context("PR selection cancelled")?;

    Ok(pr_items[selection].branch.clone())
}

enum ConflictResult {
    NoConflict,
    OpenedExisting,
}

fn check_worktree_conflicts(
    parent_dir: &Path,
    repos: &[discovery::RepoInfo],
    branch_name: &str,
) -> Result<ConflictResult> {
    let mut conflicting_repos = Vec::new();

    for repo in repos {
        if worktree::is_branch_on_worktree(&repo.path, branch_name)? {
            conflicting_repos.push(repo.name.clone());
        }
    }

    if conflicting_repos.is_empty() {
        return Ok(ConflictResult::NoConflict);
    }

    // Check if a sesh session owns this branch
    if let Some(existing) = session::find_session_by_branch(parent_dir, branch_name) {
        println!(
            "\n  {} Branch '{}' is already used by session '{}'.",
            style("!").yellow(),
            style(branch_name).cyan(),
            style(&existing.name).green(),
        );

        let open = Confirm::new()
            .with_prompt("Open that session in VS Code instead?")
            .default(true)
            .interact()
            .context("confirmation cancelled")?;

        if open {
            let sess_dir = session::session_dir(parent_dir, &existing.name);
            let paths: Vec<PathBuf> = existing
                .repos
                .iter()
                .map(|r| r.worktree_path.clone())
                .collect();
            crate::vscode::open_session_in_vscode(&sess_dir, &paths)?;
            println!(
                "  {} Opened session '{}' in VS Code.",
                style("✓").green(),
                existing.name
            );
            return Ok(ConflictResult::OpenedExisting);
        } else {
            bail!(
                "branch '{}' is in use by session '{}'. Run `sesh stop {}` first.",
                branch_name,
                existing.name,
                existing.name
            );
        }
    }

    // Branch on a worktree but not managed by sesh
    bail!(
        "branch '{}' is already checked out in a worktree not managed by sesh (in: {})",
        branch_name,
        conflicting_repos.join(", ")
    );
}

fn rollback_worktrees(created: &[(PathBuf, PathBuf)]) {
    eprintln!(
        "\n  {} Rolling back created worktrees...",
        style("✗").red()
    );
    for (repo_path, worktree_path) in created.iter().rev() {
        if let Err(e) = worktree::remove_worktree(repo_path, worktree_path) {
            eprintln!(
                "    Failed to remove worktree {}: {}",
                worktree_path.display(),
                e
            );
        }
    }
}
