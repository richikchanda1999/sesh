use std::path::Path;
use std::process::Command;

use anyhow::Result;
use console::style;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>) -> Result<()> {
    let session = pick_session(parent_dir, name)?;

    println!(
        "Session: {}  Branch: {}",
        style(&session.name).cyan().bold(),
        style(&session.branch).green(),
    );
    println!();

    for repo in &session.repos {
        println!("{}", style(format!("── {} ──", repo.name)).bold());
        println!("  Path: {}", repo.worktree_path.display());

        if !repo.worktree_path.exists() {
            println!("  {}", style("(worktree missing)").red());
            println!();
            continue;
        }

        // git status --short
        let wt = repo.worktree_path.to_string_lossy();
        let status_output = Command::new("git")
            .args(["-C", &wt, "status", "--short"])
            .output();

        match status_output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                if text.trim().is_empty() {
                    println!("  {}", style("Clean working tree").dim());
                } else {
                    for line in text.lines() {
                        println!("  {}", line);
                    }
                }
            }
            Err(e) => println!("  {}", style(format!("Failed to get status: {}", e)).red()),
        }

        // git log --oneline -5
        let log_output = Command::new("git")
            .args(["-C", &wt, "log", "--oneline", "-5"])
            .output();

        match log_output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                if !text.trim().is_empty() {
                    println!("  {}", style("Recent commits:").dim());
                    for line in text.lines() {
                        println!("    {}", line);
                    }
                }
            }
            Err(e) => println!("  {}", style(format!("Failed to get log: {}", e)).red()),
        }

        println!();
    }

    Ok(())
}
