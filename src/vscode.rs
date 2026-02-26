use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;

pub fn check_vscode_available() -> bool {
    Command::new("which")
        .arg("code")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn open_in_vscode(paths: &[PathBuf]) -> Result<()> {
    let mut errors = Vec::new();

    for path in paths {
        if let Err(e) = Command::new("code").arg(path).spawn() {
            errors.push(format!("{}: {}", path.display(), e));
        }
    }

    if !errors.is_empty() {
        eprintln!("warning: some VS Code launches failed:\n  {}", errors.join("\n  "));
    }

    Ok(())
}
