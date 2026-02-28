use std::fs::{self, File};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::config::ScriptEntry;
use crate::session::BackgroundPid;

/// Build a Command with standard sesh env vars set.
fn base_command(
    script_path: &Path,
    cwd: &Path,
    session_name: &str,
    branch: &str,
    repo_names: &[String],
) -> Command {
    let repos_csv = repo_names.join(",");
    let mut cmd = Command::new(script_path);
    cmd.current_dir(cwd)
        .env("SESH_SESSION", session_name)
        .env("SESH_BRANCH", branch)
        .env("SESH_REPOS", &repos_csv);
    cmd
}

/// Run a script entry as a foreground process (blocking).
pub fn run_script_entry(
    label: &str,
    entry: &ScriptEntry,
    script_path: &Path,
    cwd: &Path,
    session_name: &str,
    branch: &str,
    repo_names: &[String],
    extra_env: &[(&str, &str)],
) -> Result<()> {
    if !script_path.exists() {
        bail!("{} script not found: {}", label, script_path.display());
    }

    let mut cmd = base_command(script_path, cwd, session_name, branch, repo_names);
    for &(key, val) in extra_env {
        cmd.env(key, val);
    }
    cmd.stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {} script: {}", label, script_path.display()))?;

    if !status.success() {
        bail!(
            "{} script '{}' exited with status: {}",
            label,
            entry.path,
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string())
        );
    }

    Ok(())
}

/// Spawn a script as a background process. Returns the PID.
/// stdout/stderr are redirected to `<log_dir>/<label>.log`.
pub fn spawn_background_script(
    entry: &ScriptEntry,
    script_path: &Path,
    cwd: &Path,
    log_dir: &Path,
    label: &str,
    session_name: &str,
    branch: &str,
    repo_names: &[String],
    extra_env: &[(&str, &str)],
) -> Result<u32> {
    if !script_path.exists() {
        bail!("background script not found: {}", script_path.display());
    }

    fs::create_dir_all(log_dir)
        .with_context(|| format!("failed to create log dir: {}", log_dir.display()))?;

    let log_path = log_dir.join(format!("{}.log", label));
    let log_file = File::create(&log_path)
        .with_context(|| format!("failed to create log file: {}", log_path.display()))?;
    let log_stderr = log_file
        .try_clone()
        .context("failed to clone log file handle")?;

    let mut cmd = base_command(script_path, cwd, session_name, branch, repo_names);
    for &(key, val) in extra_env {
        cmd.env(key, val);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_stderr);

    let child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn background script: {}", entry.path))?;

    Ok(child.id())
}

/// Kill background processes: SIGTERM first, wait up to 5s, then SIGKILL stragglers.
pub fn kill_background_pids(pids: &[BackgroundPid]) {
    use std::process::Command as Cmd;

    // Send SIGTERM to all
    for bp in pids {
        let _ = Cmd::new("kill")
            .arg("-TERM")
            .arg(bp.pid.to_string())
            .output();
    }

    // Wait up to 5 seconds for processes to exit
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let any_alive = pids.iter().any(|bp| is_process_alive(bp.pid));
        if !any_alive || std::time::Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }

    // SIGKILL any survivors
    for bp in pids {
        if is_process_alive(bp.pid) {
            let _ = Cmd::new("kill")
                .arg("-KILL")
                .arg(bp.pid.to_string())
                .output();
        }
    }
}

fn is_process_alive(pid: u32) -> bool {
    // kill -0 checks if process exists without sending a signal
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
