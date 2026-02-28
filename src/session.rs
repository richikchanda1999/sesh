use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssueContext {
    pub provider: String,
    pub identifier: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: String,
    pub branch: String,
    pub repos: Vec<SessionRepo>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub parent_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<IssueContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
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

/// Sanitize a branch name into a flat folder name suitable for use as a session directory.
/// Replaces `/` with `-`, strips leading `.` and `..`, and appends `-2`, `-3`, etc. on collision.
pub fn sanitize_session_name(branch: &str, parent_dir: &Path) -> String {
    let mut name = branch.replace('/', "-");

    // Strip leading dots
    name = name.trim_start_matches('.').to_string();

    // If stripping left us empty, use a fallback
    if name.is_empty() {
        name = "session".to_string();
    }

    // Collect existing session folder names to detect collisions
    let sessions_dir = parent_dir.join(".sesh/sessions");
    let mut existing: HashSet<String> = HashSet::new();
    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(dir_name) = entry.file_name().to_str() {
                    existing.insert(dir_name.to_string());
                }
            }
        }
    }

    if !existing.contains(&name) {
        return name;
    }

    // Collision: append -2, -3, etc.
    let mut counter = 2;
    loop {
        let candidate = format!("{}-{}", name, counter);
        if !existing.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Check if any existing session already uses the given branch name.
pub fn find_session_by_branch(parent_dir: &Path, branch: &str) -> Option<SessionInfo> {
    let sessions = list_sessions(parent_dir).ok()?;
    sessions.into_iter().find(|s| s.branch == branch)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundPid {
    pub pid: u32,
    pub label: String,
    pub script: String,
}

pub fn save_background_pids(session_dir: &Path, pids: &[BackgroundPid]) -> anyhow::Result<()> {
    let path = session_dir.join("background_pids.json");
    let json = serde_json::to_string_pretty(pids).context("Failed to serialize background PIDs")?;
    fs::write(&path, json)
        .with_context(|| format!("Failed to write background PIDs: {}", path.display()))?;
    Ok(())
}

pub fn load_background_pids(session_dir: &Path) -> Vec<BackgroundPid> {
    let path = session_dir.join("background_pids.json");
    if !path.exists() {
        return Vec::new();
    }
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}
