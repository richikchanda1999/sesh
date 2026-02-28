use std::path::Path;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::config::SeshConfig;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Resolve user input that may be a Linear ticket, Sentry URL, or plain branch name.
pub async fn resolve_branch_input(
    input: &str,
    config: &SeshConfig,
    parent_dir: &Path,
) -> Result<String> {
    let input = input.trim();

    // Linear URL: https://linear.app/{workspace}/issue/{TEAM-123}/...
    if let Some(id) = parse_linear_url(input) {
        return branch_from_linear(&id, parent_dir).await;
    }

    // Sentry URL: https://{org}.sentry.io/issues/{id}/...
    if let Some((org, issue_id)) = parse_sentry_url(input) {
        let org = resolve_sentry_org(config, Some(&org));
        return branch_from_sentry(&org, &issue_id, parent_dir).await;
    }

    // Linear ID pattern: TEAM-123
    if is_linear_id(input) {
        return branch_from_linear(input, parent_dir).await;
    }

    // Plain text — return as-is
    Ok(input.to_string())
}

// ---------------------------------------------------------------------------
// URL / ID parsing
// ---------------------------------------------------------------------------

fn parse_linear_url(input: &str) -> Option<String> {
    // https://linear.app/{workspace}/issue/{TEAM-123}/optional-slug
    let url = input.strip_prefix("https://linear.app/")?;
    let parts: Vec<&str> = url.split('/').collect();
    // parts: [workspace, "issue", "TEAM-123", ...]
    if parts.len() >= 3 && parts[1] == "issue" {
        let id = parts[2];
        if is_linear_id(id) {
            return Some(id.to_uppercase());
        }
    }
    None
}

fn parse_sentry_url(input: &str) -> Option<(String, String)> {
    // https://{org}.sentry.io/issues/{id}/...
    let input = input.strip_prefix("https://")?;
    let (host, path) = input.split_once('/')?;
    let org = host.strip_suffix(".sentry.io")?;
    let parts: Vec<&str> = path.split('/').collect();
    // parts: ["issues", "12345", ...]
    if parts.len() >= 2 && parts[0] == "issues" {
        let id = parts[1];
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            return Some((org.to_string(), id.to_string()));
        }
    }
    None
}

fn is_linear_id(input: &str) -> bool {
    // Pattern: one or more uppercase letters, a dash, one or more digits (e.g. ENG-123)
    let Some((prefix, suffix)) = input.split_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// API calls
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LinearGraphqlResponse {
    data: Option<LinearData>,
}

#[derive(Deserialize)]
struct LinearData {
    issue: Option<LinearIssue>,
}

#[derive(Deserialize)]
struct LinearIssue {
    title: String,
    identifier: String,
}

async fn branch_from_linear(id: &str, parent_dir: &Path) -> Result<String> {
    let token = load_token(parent_dir, "linear_token")?;
    let client = Client::new();

    let query = format!(
        r#"{{"query":"{{ issue(id: \"{}\") {{ title identifier }} }}"}}"#,
        id
    );

    let resp = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", &token)
        .header("Content-Type", "application/json")
        .body(query)
        .send()
        .await
        .context("failed to call Linear API")?;

    if !resp.status().is_success() {
        bail!("Linear API returned status {}", resp.status());
    }

    let body: LinearGraphqlResponse = resp.json().await.context("failed to parse Linear response")?;

    let issue = body
        .data
        .and_then(|d| d.issue)
        .with_context(|| format!("Linear issue '{}' not found", id))?;

    let branch = format!("{}-{}", issue.identifier.to_lowercase(), slugify(&issue.title));
    Ok(truncate(&branch, 60))
}

#[derive(Deserialize)]
struct SentryIssue {
    title: String,
}

async fn branch_from_sentry(org: &str, issue_id: &str, parent_dir: &Path) -> Result<String> {
    let token = load_token(parent_dir, "sentry_token")?;
    let client = Client::new();

    let url = format!(
        "https://sentry.io/api/0/organizations/{}/issues/{}/",
        org, issue_id
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("failed to call Sentry API")?;

    if !resp.status().is_success() {
        bail!("Sentry API returned status {}", resp.status());
    }

    let issue: SentryIssue = resp.json().await.context("failed to parse Sentry response")?;

    let branch = format!("sentry-{}-{}", issue_id, slugify(&issue.title));
    Ok(truncate(&branch, 60))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_token(parent_dir: &Path, filename: &str) -> Result<String> {
    let path = parent_dir.join(".sesh/secrets").join(filename);
    let token = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "missing {} — create it at {}",
            filename,
            path.display()
        )
    })?;
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("{} is empty", path.display());
    }
    Ok(token)
}

fn resolve_sentry_org(config: &SeshConfig, url_org: Option<&str>) -> String {
    config
        .sentry
        .as_ref()
        .map(|s| s.org.clone())
        .or_else(|| url_org.map(|s| s.to_string()))
        .unwrap_or_default()
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Truncate at the last hyphen before max to avoid cutting mid-word
    let truncated = &s[..max];
    if let Some(pos) = truncated.rfind('-') {
        truncated[..pos].to_string()
    } else {
        truncated.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_linear_url() {
        assert_eq!(
            parse_linear_url("https://linear.app/myteam/issue/ENG-123/fix-login"),
            Some("ENG-123".to_string())
        );
        assert_eq!(
            parse_linear_url("https://linear.app/myteam/issue/CORE-1/"),
            Some("CORE-1".to_string())
        );
        assert_eq!(parse_linear_url("https://example.com"), None);
        assert_eq!(parse_linear_url("not a url"), None);
    }

    #[test]
    fn test_parse_sentry_url() {
        assert_eq!(
            parse_sentry_url("https://myorg.sentry.io/issues/12345/"),
            Some(("myorg".to_string(), "12345".to_string()))
        );
        assert_eq!(
            parse_sentry_url("https://myorg.sentry.io/issues/99/events"),
            Some(("myorg".to_string(), "99".to_string()))
        );
        assert_eq!(parse_sentry_url("https://sentry.io/issues/12345/"), None);
        assert_eq!(parse_sentry_url("https://myorg.sentry.io/settings/"), None);
    }

    #[test]
    fn test_is_linear_id() {
        assert!(is_linear_id("ENG-123"));
        assert!(is_linear_id("A-1"));
        assert!(!is_linear_id("eng-123")); // lowercase
        assert!(!is_linear_id("123"));
        assert!(!is_linear_id("ENG"));
        assert!(!is_linear_id("feature/test"));
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Fix Login Bug"), "fix-login-bug");
        assert_eq!(slugify("  hello  world  "), "hello-world");
        assert_eq!(slugify("foo---bar"), "foo-bar");
        assert_eq!(slugify("NullPointer in Handler!"), "nullpointer-in-handler");
    }

    #[test]
    fn test_truncate() {
        let short = "abc-def";
        assert_eq!(truncate(short, 60), "abc-def");

        let long = "eng-123-this-is-a-very-long-branch-name-that-exceeds-the-max-limit-significantly";
        let result = truncate(&long, 60);
        assert!(result.len() <= 60);
        // Should cut at a hyphen boundary
        assert!(!result.ends_with('-'));
    }
}
