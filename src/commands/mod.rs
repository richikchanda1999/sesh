pub mod activate;
pub mod auth;
pub mod completions;
pub mod doctor;
pub mod init;
pub mod list;
pub mod pr;
pub mod resume;
pub mod start;
pub mod status;
pub mod stop;

use std::path::Path;

use anyhow::{bail, Result};
use dialoguer::Select;

use crate::session::{self, SessionInfo};

/// Pick a session by name, or interactively if name is None.
pub fn pick_session(parent_dir: &Path, name: Option<String>) -> Result<SessionInfo> {
    let sessions = session::list_sessions(parent_dir)?;
    if sessions.is_empty() {
        bail!("No sessions found.");
    }

    match name {
        Some(n) => {
            sessions
                .into_iter()
                .find(|s| s.name == n)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found.", n))
        }
        None => {
            let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
            let selection = Select::new()
                .with_prompt("Select a session")
                .items(&names)
                .default(0)
                .interact()?;
            Ok(sessions.into_iter().nth(selection).unwrap())
        }
    }
}
