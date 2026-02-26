use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

fn run_script(label: &str, script_path: &Path, session_dir: &Path, branch: &str, repo_names: &[String]) -> Result<()> {
    if !script_path.exists() {
        bail!("{} script not found: {}", label, script_path.display());
    }

    let repos_csv = repo_names.join(",");
    let session_name = session_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let status = Command::new(script_path)
        .current_dir(session_dir)
        .env("SESH_SESSION", &session_name)
        .env("SESH_BRANCH", branch)
        .env("SESH_REPOS", &repos_csv)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("failed to execute {} script: {}", label, script_path.display()))?;

    if !status.success() {
        bail!(
            "{} script exited with status: {}",
            label,
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string())
        );
    }

    Ok(())
}

pub fn run_setup_script(script_path: &Path, session_dir: &Path, branch: &str, repo_names: &[String]) -> Result<()> {
    run_script("setup", script_path, session_dir, branch, repo_names)
}

pub fn run_teardown_script(script_path: &Path, session_dir: &Path, branch: &str, repo_names: &[String]) -> Result<()> {
    run_script("teardown", script_path, session_dir, branch, repo_names)
}
