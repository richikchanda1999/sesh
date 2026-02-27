use std::path::Path;

use anyhow::Result;
use console::style;

use crate::session;
use crate::vscode;

use super::pick_session;

pub fn run(parent_dir: &Path, name: Option<String>) -> Result<()> {
    let sess = pick_session(parent_dir, name)?;

    let paths: Vec<_> = sess.repos.iter().map(|r| r.worktree_path.clone()).collect();

    if paths.is_empty() {
        println!("No repos in session '{}'.", sess.name);
        return Ok(());
    }

    let sess_dir = session::session_dir(parent_dir, &sess.name);
    vscode::open_session_in_vscode(&sess_dir, &paths)?;

    println!("Opened VS Code for session '{}':", style(&sess.name).cyan());
    for repo in &sess.repos {
        println!("  {} -> {}", style(&repo.name).green(), repo.worktree_path.display());
    }

    Ok(())
}
