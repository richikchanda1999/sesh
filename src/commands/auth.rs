use std::path::Path;

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Password;

pub fn run(parent_dir: &Path, provider: &str) -> Result<()> {
    let (filename, prompt, help) = match provider {
        "linear" => (
            "linear_token",
            "Linear API key",
            "Get one from: Linear → Settings → API → Personal API keys",
        ),
        "sentry" => (
            "sentry_token",
            "Sentry auth token",
            "Get one from: Sentry → Settings → Auth Tokens",
        ),
        _ => bail!("unknown provider: {}", provider),
    };

    let secrets_dir = parent_dir.join(".sesh/secrets");
    let token_path = secrets_dir.join(filename);

    // Show existing status
    if token_path.exists() {
        let existing = std::fs::read_to_string(&token_path).unwrap_or_default();
        let existing = existing.trim();
        if !existing.is_empty() {
            let masked = if existing.len() > 8 {
                format!("{}…{}", &existing[..4], &existing[existing.len() - 4..])
            } else {
                "****".to_string()
            };
            println!(
                "  {} Existing {} token: {}",
                style("ℹ").cyan(),
                provider,
                masked
            );
        }
    }

    println!("  {} {}", style("ℹ").cyan(), help);

    let token: String = Password::new()
        .with_prompt(prompt)
        .interact()
        .context("token input cancelled")?;

    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("token cannot be empty");
    }

    std::fs::create_dir_all(&secrets_dir)
        .with_context(|| format!("failed to create {}", secrets_dir.display()))?;
    std::fs::write(&token_path, &token)
        .with_context(|| format!("failed to write {}", token_path.display()))?;

    println!(
        "\n  {} {} token saved to {}",
        style("✓").green(),
        provider,
        token_path.display()
    );

    Ok(())
}
