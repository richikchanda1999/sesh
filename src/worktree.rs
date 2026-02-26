use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

fn run_git(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        bail!(
            "git {} failed (exit code {}): {}",
            args.join(" "),
            code,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

pub fn create_worktree(
    repo_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
    base_ref: &str,
) -> Result<()> {
    let wt = worktree_path.to_string_lossy();
    let result = run_git(
        repo_path,
        &["worktree", "add", &wt, "-b", branch_name, base_ref],
    );

    if let Err(e) = result {
        let repo_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| repo_path.display().to_string());
        bail!("failed to create worktree for repo '{}': {}", repo_name, e);
    }

    Ok(())
}

pub fn remove_worktree(repo_path: &Path, worktree_path: &Path) -> Result<()> {
    let wt = worktree_path.to_string_lossy();
    run_git(repo_path, &["worktree", "remove", &wt, "--force"])?;
    Ok(())
}

pub fn prune_worktrees(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["worktree", "prune"])?;
    Ok(())
}

pub fn branch_exists(repo_path: &Path, branch_name: &str) -> Result<bool> {
    let ref_name = format!("refs/heads/{}", branch_name);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--verify", &ref_name])
        .output()
        .with_context(|| format!("failed to run git rev-parse for branch '{}'", branch_name))?;

    Ok(output.status.success())
}

pub fn fetch_branch(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    run_git(repo_path, &["fetch", remote, branch])?;
    Ok(())
}

pub fn delete_branch(repo_path: &Path, branch_name: &str) -> Result<()> {
    run_git(repo_path, &["branch", "-D", branch_name])?;
    Ok(())
}

pub fn get_worktree_list(repo_path: &Path) -> Result<Vec<String>> {
    let output = run_git(repo_path, &["worktree", "list", "--porcelain"])?;

    let paths = output
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(|s| s.to_string())
        .collect();

    Ok(paths)
}

pub fn validate_branch_name(name: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .output()
        .context("failed to run git check-ref-format")?;

    if !output.status.success() {
        bail!("invalid branch name: '{}'", name);
    }

    Ok(())
}
