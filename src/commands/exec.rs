use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};
use console::style;

use super::pick_session;

pub fn run(parent_dir: &Path, session_name: Option<String>, command: &str) -> Result<()> {
    let info = pick_session(parent_dir, session_name)?;

    let repos: Vec<_> = info
        .repos
        .iter()
        .filter(|r| r.worktree_path.exists())
        .collect();

    if repos.is_empty() {
        bail!("no worktrees found on disk for session '{}'", info.name);
    }

    // Spawn all commands in parallel
    let handles: Vec<_> = repos
        .iter()
        .map(|repo| {
            let name = repo.name.clone();
            let cwd = repo.worktree_path.clone();
            let cmd = command.to_string();

            std::thread::spawn(move || {
                let output = Command::new("sh")
                    .args(["-c", &cmd])
                    .current_dir(&cwd)
                    .output();

                (name, output)
            })
        })
        .collect();

    // Collect results and print sequentially
    let mut any_failed = false;

    for handle in handles {
        let (name, result) = handle.join().expect("thread panicked");
        match result {
            Ok(output) => {
                println!("{}", style(format!("── {} ──", name)).cyan().bold());

                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    print!("{}", stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    eprint!("{}", stderr);
                }

                if !output.status.success() {
                    println!(
                        "{} exited with {}",
                        style(&name).red(),
                        output.status
                    );
                    any_failed = true;
                }

                println!();
            }
            Err(e) => {
                println!("{}", style(format!("── {} ──", name)).cyan().bold());
                println!("{} failed to execute: {}", style(&name).red(), e);
                any_failed = true;
                println!();
            }
        }
    }

    if any_failed {
        bail!("one or more commands failed");
    }

    Ok(())
}
