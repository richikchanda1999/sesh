use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::McpServer;

#[derive(Serialize)]
struct McpServerEntry {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Serialize)]
struct McpConfigFile {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerEntry>,
}

pub fn write_mcp_config(
    worktree_path: &Path,
    original_repo_path: &Path,
    servers: &[McpServer],
) -> Result<()> {
    if servers.is_empty() {
        return Ok(());
    }

    let mcp_servers: HashMap<String, McpServerEntry> = servers
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                McpServerEntry {
                    kind: s.kind.clone(),
                    url: s.url.clone(),
                },
            )
        })
        .collect();

    let config = McpConfigFile { mcp_servers };
    let json = serde_json::to_string_pretty(&config)
        .context("failed to serialize MCP config")?;

    let dest = worktree_path.join(".mcp.json");
    std::fs::write(&dest, json)
        .with_context(|| format!("failed to write MCP config to {}", dest.display()))?;

    // Ensure .mcp.json is excluded from git in the original repo so it can
    // never be accidentally committed from any worktree.
    add_to_git_exclude(original_repo_path, ".mcp.json")?;

    Ok(())
}

/// Appends an entry to the repo's `.git/info/exclude` if not already present.
/// This is a local-only exclude mechanism that is never committed.
fn add_to_git_exclude(repo_path: &Path, pattern: &str) -> Result<()> {
    let exclude_dir = repo_path.join(".git/info");
    std::fs::create_dir_all(&exclude_dir)
        .with_context(|| format!("failed to create {}", exclude_dir.display()))?;

    let exclude_path = exclude_dir.join("exclude");
    let contents = std::fs::read_to_string(&exclude_path).unwrap_or_default();

    if contents.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)
        .with_context(|| format!("failed to open {}", exclude_path.display()))?;

    // Ensure we start on a new line
    if !contents.is_empty() && !contents.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{}", pattern)?;

    Ok(())
}
