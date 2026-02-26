use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: String,
    pub branch: String,
    pub repos: Vec<SessionRepo>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub parent_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRepo {
    pub name: String,
    pub worktree_path: PathBuf,
    pub original_repo_path: PathBuf,
}

pub fn session_dir(parent_dir: &Path, session_name: &str) -> PathBuf {
    parent_dir.join(".sesh/sessions").join(session_name)
}

pub fn save_session(session_dir: &Path, info: &SessionInfo) -> anyhow::Result<()> {
    fs::create_dir_all(session_dir)
        .with_context(|| format!("Failed to create session directory: {}", session_dir.display()))?;

    let json = serde_json::to_string_pretty(info).context("Failed to serialize session info")?;
    let path = session_dir.join("session.json");
    fs::write(&path, json)
        .with_context(|| format!("Failed to write session file: {}", path.display()))?;

    Ok(())
}

pub fn load_session(session_dir: &Path) -> anyhow::Result<SessionInfo> {
    let path = session_dir.join("session.json");
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;
    let info: SessionInfo =
        serde_json::from_str(&contents).context("Failed to parse session.json")?;
    Ok(info)
}

pub fn list_sessions(parent_dir: &Path) -> anyhow::Result<Vec<SessionInfo>> {
    let sessions_dir = parent_dir.join(".sesh/sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read sessions directory: {}", sessions_dir.display()))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            if let Ok(info) = load_session(&path) {
                sessions.push(info);
            }
        }
    }

    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(sessions)
}

pub fn delete_session_dir(session_dir: &Path) -> anyhow::Result<()> {
    fs::remove_dir_all(session_dir)
        .with_context(|| format!("Failed to remove session directory: {}", session_dir.display()))?;
    Ok(())
}

pub fn session_exists(parent_dir: &Path, session_name: &str) -> bool {
    session_dir(parent_dir, session_name)
        .join("session.json")
        .exists()
}
