use std::path::Path;

use anyhow::Result;
use console::style;

use crate::session;

pub fn run(parent_dir: &Path, active: bool) -> Result<()> {
    let mut sessions = session::list_sessions(parent_dir)?;

    if active {
        sessions.retain(|s| {
            s.repos.iter().any(|r| r.worktree_path.exists())
        });
    }

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    // Print table header
    println!(
        "{:<20} {:<25} {:<6} {}",
        style("Name").bold().underlined(),
        style("Branch").bold().underlined(),
        style("Repos").bold().underlined(),
        style("Created").bold().underlined(),
    );

    for session in &sessions {
        let created = session.created_at.format("%Y-%m-%d %H:%M");
        println!(
            "{:<20} {:<25} {:<6} {}",
            session.name,
            session.branch,
            session.repos.len(),
            created,
        );
    }

    Ok(())
}
