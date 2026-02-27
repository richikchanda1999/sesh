use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn generate_context(
    session_dir: &Path,
    session_name: &str,
    repos: &[(String, PathBuf)],
    shared_context_files: &[String],
    parent_dir: &Path,
) -> Result<()> {
    let context_dir = session_dir.join("context");
    std::fs::create_dir_all(&context_dir)
        .with_context(|| format!("failed to create context dir: {}", context_dir.display()))?;

    // Build .sesh-context.md content
    let mut content = format!("# Session: {}\n\n## Repositories\n\n", session_name);
    for (name, path) in repos {
        content.push_str(&format!("- **{}**: `{}`\n", name, path.display()));
    }

    let context_file = context_dir.join(".sesh-context.md");
    std::fs::write(&context_file, &content)
        .with_context(|| format!("failed to write {}", context_file.display()))?;

    // Symlink shared context files into the session context/ directory (not into worktrees)
    for filename in shared_context_files {
        let source = parent_dir.join(filename);
        if !source.exists() {
            continue;
        }
        let link = context_dir.join(filename);
        if !link.exists() {
            symlink(&source, &link).with_context(|| {
                format!(
                    "failed to symlink {} -> {}",
                    link.display(),
                    source.display()
                )
            })?;
        }
    }

    Ok(())
}
