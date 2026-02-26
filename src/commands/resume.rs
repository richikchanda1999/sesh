use std::path::Path;

use anyhow::Result;
use console::style;

use crate::vscode;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>) -> Result<()> {
    let session = pick_session(parent_dir, name)?;

    let paths: Vec<_> = session.repos.iter().map(|r| r.worktree_path.clone()).collect();

    if paths.is_empty() {
        println!("No repos in session '{}'.", session.name);
        return Ok(());
    }

    vscode::open_in_vscode(&paths)?;

    println!("Opened VS Code for session '{}':", style(&session.name).cyan());
    for repo in &session.repos {
        println!("  {} -> {}", style(&repo.name).green(), repo.worktree_path.display());
    }

    Ok(())
}
