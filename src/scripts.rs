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

pub fn run_setup_script_with_env(
    script_path: &Path,
    session_dir: &Path,
    branch: &str,
    repo_names: &[String],
    exclusive_skipped: &[String],
) -> Result<()> {
    if !script_path.exists() {
        bail!("setup script not found: {}", script_path.display());
    }

    let repos_csv = repo_names.join(",");
    let session_name = session_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut cmd = Command::new(script_path);
    cmd.current_dir(session_dir)
        .env("SESH_SESSION", &session_name)
        .env("SESH_BRANCH", branch)
        .env("SESH_REPOS", &repos_csv)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    if !exclusive_skipped.is_empty() {
        cmd.env("SESH_EXCLUSIVE_SKIP", exclusive_skipped.join(","));
    }

    let status = cmd.status()
        .with_context(|| format!("failed to execute setup script: {}", script_path.display()))?;

    if !status.success() {
        bail!(
            "setup script exited with status: {}",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string())
        );
    }

    Ok(())
}

pub fn run_teardown_script(script_path: &Path, session_dir: &Path, branch: &str, repo_names: &[String]) -> Result<()> {
    run_script("teardown", script_path, session_dir, branch, repo_names)
}
