use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>, base: String) -> Result<()> {
    let session = pick_session(parent_dir, name)?;

    // Check gh is available
    let gh_check = Command::new("which").arg("gh").output();
    match gh_check {
        Ok(output) if !output.status.success() => bail!("GitHub CLI (gh) not found. Install it from https://cli.github.com"),
        Err(_) => bail!("GitHub CLI (gh) not found. Install it from https://cli.github.com"),
        _ => {}
    }

    for repo in &session.repos {
        println!("{}", style(format!("── {} ──", repo.name)).bold());

        if !repo.worktree_path.exists() {
            println!("  {}", style("(worktree missing, skipping)").red());
            continue;
        }

        let wt = repo.worktree_path.to_string_lossy();

        // Push branch
        println!("  Pushing branch '{}'...", session.branch);
        let push_output = Command::new("git")
            .args(["-C", &wt, "push", "-u", "origin", &session.branch])
            .output()
            .context("Failed to run git push")?;

        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            eprintln!("  {}: {}", style("Push failed").red(), stderr.trim());
            continue;
        }

        // Create PR
        println!("  Creating PR...");
        let pr_output = Command::new("gh")
            .args([
                "pr", "create",
                "--base", &base,
                "--head", &session.branch,
                "--title", &session.branch,
                "--fill",
            ])
            .current_dir(&repo.worktree_path)
            .output()
            .context("Failed to run gh pr create")?;

        if pr_output.status.success() {
            let url = String::from_utf8_lossy(&pr_output.stdout);
            println!("  {} {}", style("PR:").green(), url.trim());
        } else {
            let stderr = String::from_utf8_lossy(&pr_output.stderr);
            eprintln!("  {}: {}", style("PR creation failed").red(), stderr.trim());
        }

        println!();
    }

    Ok(())
}
