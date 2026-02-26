use std::path::Path;

use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, MultiSelect};

use crate::discovery;

pub fn run(parent_dir: &Path) -> Result<()> {
    let config_path = parent_dir.join("sesh.toml");

    if config_path.exists() {
        let overwrite = Confirm::new()
            .with_prompt("sesh.toml already exists. Overwrite?")
            .default(false)
            .interact()?;
        if !overwrite {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Discover repos
    let repos = discovery::discover_repos(parent_dir)?;
    if repos.is_empty() {
        println!("No git repos found in {}", parent_dir.display());
        return Ok(());
    }

    println!("Found {} repo(s):", repos.len());
    for repo in &repos {
        println!("  {} ({})", style(&repo.name).green(), repo.current_branch);
    }
    println!();

    // Ask for base branch
    let base_branch: String = Input::new()
        .with_prompt("Default base branch")
        .default("main".to_string())
        .interact_text()?;

    // Ask about MCP servers
    let mcp_options = vec!["sentry", "linear"];
    let mcp_selected = MultiSelect::new()
        .with_prompt("Include MCP servers (space to select, enter to confirm)")
        .items(&mcp_options)
        .interact()?;

    // Build TOML content
    let mut toml = String::new();

    // [session]
    toml.push_str("[session]\n");
    toml.push_str(&format!("base_branch = \"{}\"\n", base_branch));
    toml.push_str("shared_context = []\n");
    toml.push('\n');

    // [scripts]
    toml.push_str("[scripts]\n");
    toml.push_str("# setup = \"./scripts/setup.sh\"\n");
    toml.push_str("# teardown = \"./scripts/teardown.sh\"\n");
    toml.push('\n');

    // MCP servers
    for &idx in &mcp_selected {
        let name = mcp_options[idx];
        toml.push_str("[[mcp.servers]]\n");
        match name {
            "sentry" => {
                toml.push_str("name = \"sentry\"\n");
                toml.push_str("type = \"http\"\n");
                toml.push_str("url = \"https://mcp.sentry.dev/mcp\"\n");
            }
            "linear" => {
                toml.push_str("name = \"linear\"\n");
                toml.push_str("type = \"http\"\n");
                toml.push_str("url = \"https://mcp.linear.app/mcp\"\n");
            }
            _ => {}
        }
        toml.push('\n');
    }

    // [repos.*]
    for repo in &repos {
        toml.push_str(&format!("[repos.{}]\n", repo.name));
        toml.push_str("copy = []\n");
        toml.push_str("symlink = []\n");
        toml.push('\n');
    }

    std::fs::write(&config_path, &toml)?;
    println!(
        "{} Created {}",
        style("✔").green(),
        config_path.display(),
    );

    Ok(())
}
