use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};
use console::style;

use crate::session;

use super::pick_session;

pub fn run(
    parent_dir: &Path,
    session_name: Option<String>,
    script: Option<String>,
    follow: bool,
) -> Result<()> {
    let info = pick_session(parent_dir, session_name)?;
    let sess_dir = session::session_dir(parent_dir, &info.name);
    let log_dir = sess_dir.join("logs");

    if !log_dir.exists() {
        bail!("no logs directory for session '{}'", info.name);
    }

    match script {
        None => list_logs(&sess_dir, &log_dir),
        Some(label) => view_log(&log_dir, &label, follow),
    }
}

fn list_logs(sess_dir: &Path, log_dir: &Path) -> Result<()> {
    let pids = session::load_background_pids(sess_dir);

    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(log_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
        .collect();

    if entries.is_empty() {
        println!("No log files found.");
        return Ok(());
    }

    entries.sort_by_key(|e| e.file_name());

    println!("{}", style("Background script logs:").bold());
    println!();

    for entry in &entries {
        let path = entry.path();
        let label = path.file_stem().unwrap().to_string_lossy().to_string();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        // Find matching background PID entry
        let pid_entry = pids.iter().find(|p| p.label == label);

        let status = match pid_entry {
            Some(p) => {
                if is_process_running(p.pid) {
                    style("running").green().to_string()
                } else {
                    style("stopped").red().to_string()
                }
            }
            None => style("unknown").dim().to_string(),
        };

        let script_path = pid_entry
            .map(|p| p.script.as_str())
            .unwrap_or("?");

        println!(
            "  {} {} ({}, {}, {})",
            style("•").dim(),
            style(&label).cyan(),
            script_path,
            status,
            format_size(size),
        );
    }

    println!();
    println!(
        "View a log: {} {}",
        style("sesh log").dim(),
        style("<label>").dim()
    );

    Ok(())
}

fn view_log(log_dir: &Path, label: &str, follow: bool) -> Result<()> {
    // Try exact match first
    let exact = log_dir.join(format!("{}.log", label));
    let log_path = if exact.exists() {
        exact
    } else {
        // Fallback: substring match
        let mut matches: Vec<_> = std::fs::read_dir(log_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with(".log") && name.contains(label)
            })
            .collect();

        match matches.len() {
            0 => bail!("no log file matching '{}'", label),
            1 => matches.remove(0).path(),
            _ => {
                let names: Vec<String> = matches
                    .iter()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                bail!(
                    "ambiguous label '{}' — matches: {}",
                    label,
                    names.join(", ")
                );
            }
        }
    };

    if follow {
        let status = Command::new("tail")
            .args(["-f", &log_path.to_string_lossy()])
            .status()?;

        if !status.success() {
            bail!("tail exited with {}", status);
        }
    } else {
        let content = std::fs::read_to_string(&log_path)?;
        print!("{}", content);
    }

    Ok(())
}

fn is_process_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
