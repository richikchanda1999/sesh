use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SeshConfig {
    pub session: SessionConfig,
    pub scripts: ScriptsConfig,
    pub mcp: McpConfig,
    pub repos: HashMap<String, RepoConfig>,
    pub presets: HashMap<String, Vec<String>>,
    pub sentry: Option<SentryConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SentryConfig {
    pub org: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub base_branch: String,
    pub shared_context: Vec<String>,
    pub copy: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            base_branch: "main".to_string(),
            shared_context: Vec::new(),
            copy: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ScriptsConfig {
    pub setup: Option<String>,
    pub teardown: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct McpConfig {
    pub servers: Vec<McpServer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServer {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RepoConfig {
    pub base_branch: Option<String>,
    pub copy: Vec<String>,
    pub symlink: Vec<String>,
    pub skip: bool,
    pub exclusive: bool,
    pub setup: Option<String>,
    pub teardown: Option<String>,
}

impl SeshConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        let config: SeshConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;

        Ok(config)
    }
}
