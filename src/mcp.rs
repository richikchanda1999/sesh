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

pub fn write_mcp_config(worktree_path: &Path, servers: &[McpServer]) -> Result<()> {
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

    Ok(())
}
