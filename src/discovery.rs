use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub name: String,
    pub path: PathBuf,
    pub current_branch: String,
    pub is_dirty: bool,
}

pub fn discover_repos(parent_dir: &Path) -> Result<Vec<RepoInfo>> {
    let entries = std::fs::read_dir(parent_dir)
        .with_context(|| format!("failed to read directory: {}", parent_dir.display()))?;

    let mut repos = Vec::new();

    for entry in entries {
        let entry = entry.with_context(|| "failed to read directory entry")?;
        let path = entry.path();

        // Skip non-directories
        if !path.is_dir() {
            continue;
        }

        // Skip hidden directories (names starting with '.')
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.starts_with('.') => continue,
            Some(n) => n.to_string(),
            None => continue,
        };

        let git_path = path.join(".git");

        if git_path.is_dir() {
            // Regular git repo — include it
        } else if git_path.is_file() {
            // Worktree (.git is a file pointing to the real repo) — skip
            continue;
        } else {
            // No .git at all — skip
            continue;
        }

        let current_branch = git_branch(&path).unwrap_or_default();
        let is_dirty = git_is_dirty(&path).unwrap_or(false);

        repos.push(RepoInfo {
            name,
            path,
            current_branch,
            is_dirty,
        });
    }

    repos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
}

fn git_branch(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "branch", "--show-current"])
        .output()
        .with_context(|| format!("failed to run git branch in {}", repo_path.display()))?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_is_dirty(repo_path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .with_context(|| format!("failed to run git status in {}", repo_path.display()))?;

    Ok(!output.stdout.is_empty())
}
