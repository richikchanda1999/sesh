use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

/// Open VS Code with the appropriate strategy:
/// - 1 repo: open the single worktree path directly
/// - 2+ repos: open the session directory so all repos appear as folders in one window
pub fn open_session_in_vscode(session_dir: &Path, worktree_paths: &[PathBuf]) -> Result<()> {
    if worktree_paths.is_empty() {
        return Ok(());
    }

    let path = if worktree_paths.len() == 1 {
        worktree_paths[0].clone()
    } else {
        session_dir.to_path_buf()
    };

    if let Err(e) = Command::new("code").arg(&path).spawn() {
        eprintln!("warning: VS Code launch failed: {}: {}", path.display(), e);
    }

    Ok(())
}
