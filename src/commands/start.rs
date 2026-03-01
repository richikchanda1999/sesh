use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::{FuzzySelect, Input, MultiSelect};

use crate::config::SeshConfig;
use crate::discovery;
use crate::integrations;
use crate::session::{self, IssueContext};
use crate::worktree;

pub async fn run(
    parent_dir: &Path,
    branch: Option<String>,
    from: Option<String>,
    all: bool,
    preset: Option<String>,
    no_setup: bool,
    no_vscode: bool,
    linear: bool,
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

    // 4. Get branch name (resolves Linear/Sentry inputs, validates, checks for conflicts)
    let (branch_name, issue_context) = resolve_branch_name(
        branch.as_deref(),
        parent_dir,
        &selected_repos,
        &config,
        linear,
    )
    .await?;

    let effective_base = from.as_deref().unwrap_or(&config.session.base_branch);

    // Sanitize branch name into a flat folder name
    let session_name = session::sanitize_session_name(&branch_name, parent_dir);
    let sess_dir = session::session_dir(parent_dir, &session_name);

    println!(
        "\n{} Creating session {} (branch: {}) with {} repo(s)...\n",
        style("→").cyan().bold(),
        style(&session_name).green().bold(),
        style(&branch_name).cyan(),
        selected_repos.len()
    );

    // 5. Per-repo: validate base branch, fetch, create worktree
    let mut created_worktrees: Vec<(PathBuf, PathBuf)> = Vec::new(); // (repo_path, worktree_path)

    for repo in &selected_repos {
        let repo_config = config.repos.get(&repo.name);
        let base_branch = from.as_deref()
            .or_else(|| repo_config.and_then(|rc| rc.base_branch.as_deref()))
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

        // Create worktree with new branch (branch guaranteed not to exist after resolve_branch_name)
        if let Err(e) = worktree::create_worktree(&repo.path, &worktree_path, &branch_name, &base_ref) {
            rollback_worktrees(&created_worktrees);
            return Err(e.context(format!("failed while setting up repo '{}'", repo.name)));
        }

        created_worktrees.push((repo.path.clone(), worktree_path.clone()));
        println!("  {} Worktree created: {}", style("✓").green(), repo.name);
    }

    // 6. Finalize session (save, copy files, MCP, context, locks, scripts, VS Code, summary)
    super::finalize_session(
        parent_dir,
        &config,
        &selected_repos,
        &branch_name,
        &session_name,
        &sess_dir,
        issue_context,
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

async fn resolve_branch_name(
    flag_branch: Option<&str>,
    parent_dir: &Path,
    selected_repos: &[discovery::RepoInfo],
    config: &SeshConfig,
    linear: bool,
) -> Result<(String, Option<IssueContext>)> {
    let is_interactive = flag_branch.is_none() && !linear;

    // --linear: pick from assigned tickets (re-prompt on conflict)
    if linear {
        println!("  {} Fetching Linear tickets...", style("↓").dim());
        let issues = integrations::list_linear_issues(parent_dir).await?;
        if issues.is_empty() {
            bail!("no assigned Linear issues found");
        }

        loop {
            let (candidate, issue_ctx) = pick_linear_ticket(&issues)?;
            let resolved = apply_prefix(config, &candidate);

            if let Err(e) = worktree::validate_branch_name(&resolved) {
                println!(
                    "  {} '{}' is not a valid git branch name: {}",
                    style("✗").red(), resolved, e
                );
                continue;
            }
            if let Some(existing) = session::find_session_by_branch(parent_dir, &resolved) {
                println!(
                    "  {} Session '{}' already uses branch '{}'. Pick a different ticket.",
                    style("✗").red(), existing.name, resolved
                );
                continue;
            }
            let mut conflicts = Vec::new();
            for repo in selected_repos {
                if worktree::branch_exists(&repo.path, &resolved)? {
                    conflicts.push(repo.name.clone());
                }
            }
            if !conflicts.is_empty() {
                println!(
                    "  {} Branch '{}' already exists in: {}. Pick a different ticket.",
                    style("✗").red(), resolved, conflicts.join(", ")
                );
                continue;
            }
            return Ok((resolved, Some(issue_ctx)));
        }
    }

    loop {
        // 1. Get candidate
        let candidate = match flag_branch {
            Some(b) => b.to_string(),
            None => prompt_branch_name()?,
        };

        // 2. Resolve Linear/Sentry → branch name + optional issue context
        let resolution = integrations::resolve_branch_input(&candidate, config, parent_dir).await?;

        // 3. Apply branch prefix
        let branch_name = apply_prefix(config, &resolution.branch);

        // 4. Validate git branch name
        if let Err(e) = worktree::validate_branch_name(&branch_name) {
            if is_interactive {
                println!(
                    "  {} '{}' is not a valid git branch name: {}",
                    style("✗").red(),
                    branch_name,
                    e
                );
                continue;
            }
            bail!("'{}' is not a valid git branch name: {}", branch_name, e);
        }

        // 5. Check session-level duplicate
        if let Some(existing) = session::find_session_by_branch(parent_dir, &branch_name) {
            if is_interactive {
                println!(
                    "  {} Session '{}' already uses branch '{}'. Choose a different name.",
                    style("✗").red(),
                    existing.name,
                    branch_name
                );
                continue;
            }
            bail!(
                "session '{}' already uses branch '{}'. Use `sesh stop {}` first or choose a different branch.",
                existing.name, branch_name, existing.name
            );
        }

        // 6. Check branch existence in ALL selected repos
        let mut conflicts = Vec::new();
        for repo in selected_repos {
            if worktree::branch_exists(&repo.path, &branch_name)? {
                conflicts.push(repo.name.clone());
            }
        }

        if !conflicts.is_empty() {
            if is_interactive {
                println!(
                    "  {} Branch '{}' already exists in: {}. Choose a different name.",
                    style("✗").red(),
                    branch_name,
                    conflicts.join(", ")
                );
                continue;
            }
            bail!(
                "branch '{}' already exists in: {}",
                branch_name,
                conflicts.join(", ")
            );
        }

        return Ok((branch_name, resolution.issue));
    }
}

fn pick_linear_ticket(issues: &[integrations::LinearIssueSummary]) -> Result<(String, IssueContext)> {
    let labels: Vec<String> = issues
        .iter()
        .map(|i| {
            let state_colored = integrations::color_text(
                &i.state_name,
                i.state_color.as_deref(),
            );
            let label_str = if i.labels.is_empty() {
                String::new()
            } else {
                let colored_labels: Vec<String> = i.labels.iter()
                    .map(|l| integrations::color_text(&l.name, l.color.as_deref()))
                    .collect();
                format!(" [{}]", colored_labels.join(", "))
            };
            format!("{} {} — {}{}", i.identifier, state_colored, i.title, label_str)
        })
        .collect();

    let selection = FuzzySelect::new()
        .with_prompt("Select a Linear ticket")
        .items(&labels)
        .default(0)
        .interact()
        .context("ticket selection cancelled")?;

    let branch = integrations::branch_name_from_linear_issue(&issues[selection]);
    let issue_ctx = integrations::issue_context_from_linear_summary(&issues[selection]);
    Ok((branch, issue_ctx))
}

fn apply_prefix(config: &SeshConfig, branch: &str) -> String {
    match &config.session.branch_prefix {
        Some(prefix) if !branch.starts_with(prefix.as_str()) => format!("{}{}", prefix, branch),
        _ => branch.to_string(),
    }
}

fn rollback_worktrees(created: &[(PathBuf, PathBuf)]) {
    eprintln!("\n  {} Rolling back created worktrees...", style("✗").red());
    for (repo_path, worktree_path) in created.iter().rev() {
        if let Err(e) = worktree::remove_worktree(repo_path, worktree_path) {
            eprintln!("    Failed to remove worktree {}: {}", worktree_path.display(), e);
        }
    }
}

