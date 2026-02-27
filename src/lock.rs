use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub session: String,
    pub locked_at: DateTime<Utc>,
}

fn locks_dir(parent_dir: &Path) -> PathBuf {
    parent_dir.join(".sesh/locks")
}

fn lock_path(parent_dir: &Path, repo_name: &str) -> PathBuf {
    locks_dir(parent_dir).join(format!("{}.lock", repo_name))
}

pub fn acquire_lock(parent_dir: &Path, repo_name: &str, session_name: &str) -> Result<()> {
    let dir = locks_dir(parent_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create locks directory: {}", dir.display()))?;

    let info = LockInfo {
        session: session_name.to_string(),
        locked_at: Utc::now(),
    };

    let path = lock_path(parent_dir, repo_name);
    let json = serde_json::to_string_pretty(&info).context("failed to serialize lock info")?;
    fs::write(&path, json)
        .with_context(|| format!("failed to write lock file: {}", path.display()))?;

    Ok(())
}

pub fn release_lock(parent_dir: &Path, repo_name: &str) -> Result<()> {
    let path = lock_path(parent_dir, repo_name);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove lock file: {}", path.display()))?;
    }
    Ok(())
}

pub fn check_lock(parent_dir: &Path, repo_name: &str) -> Result<Option<LockInfo>> {
    let path = lock_path(parent_dir, repo_name);
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read lock file: {}", path.display()))?;
    let info: LockInfo =
        serde_json::from_str(&contents).context("failed to parse lock file")?;
    Ok(Some(info))
}

/// List all lock files and their contents.
pub fn list_locks(parent_dir: &Path) -> Result<Vec<(String, LockInfo)>> {
    let dir = locks_dir(parent_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut locks = Vec::new();
    for entry in fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("lock") {
            if let Some(repo_name) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(info) = serde_json::from_str::<LockInfo>(&contents) {
                        locks.push((repo_name.to_string(), info));
                    }
                }
            }
        }
    }

    Ok(locks)
}
