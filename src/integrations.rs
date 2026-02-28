use std::path::Path;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::config::SeshConfig;
use crate::session::IssueContext;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub struct BranchResolution {
    pub branch: String,
    pub issue: Option<IssueContext>,
}

/// Resolve user input that may be a Linear ticket, Sentry URL, or plain branch name.
pub async fn resolve_branch_input(
    input: &str,
    config: &SeshConfig,
    parent_dir: &Path,
) -> Result<BranchResolution> {
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
    Ok(BranchResolution {
        branch: input.to_string(),
        issue: None,
    })
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
struct LinearIssueResponse {
    data: Option<LinearIssueData>,
}

#[derive(Deserialize)]
struct LinearIssueData {
    issue: Option<LinearIssue>,
}

#[derive(Deserialize)]
struct LinearIssue {
    title: String,
    identifier: String,
    #[serde(default)]
    state: Option<LinearState>,
    #[serde(default)]
    labels: Option<LinearLabelConnection>,
}

#[derive(Deserialize)]
struct LinearState {
    name: String,
    #[serde(rename = "type")]
    state_type: String,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Deserialize)]
struct LinearLabelConnection {
    nodes: Vec<LinearLabel>,
}

#[derive(Deserialize)]
struct LinearLabel {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Deserialize)]
struct LinearViewerResponse {
    data: Option<LinearViewerData>,
}

#[derive(Deserialize)]
struct LinearViewerData {
    viewer: Option<LinearViewer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearViewer {
    assigned_issues: Option<LinearIssueConnection>,
}

#[derive(Deserialize)]
struct LinearIssueConnection {
    nodes: Vec<LinearIssue>,
}

pub struct LinearIssueSummary {
    pub identifier: String,
    pub title: String,
    pub state_name: String,
    pub state_type: String,
    pub state_color: Option<String>,
    pub labels: Vec<LinearLabelSummary>,
}

pub struct LinearLabelSummary {
    pub name: String,
    pub color: Option<String>,
}

async fn branch_from_linear(id: &str, parent_dir: &Path) -> Result<BranchResolution> {
    let token = load_token(parent_dir, "linear_token")?;
    let client = Client::new();

    let query = format!(
        r#"{{"query":"{{ issue(id: \"{}\") {{ title identifier state {{ name type }} labels {{ nodes {{ name }} }} }} }}"}}"#,
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

    let body: LinearIssueResponse = resp.json().await.context("failed to parse Linear response")?;

    let issue = body
        .data
        .and_then(|d| d.issue)
        .with_context(|| format!("Linear issue '{}' not found", id))?;

    let branch = format!("{}-{}", issue.identifier.to_lowercase(), slugify(&issue.title));

    let issue_ctx = IssueContext {
        provider: "linear".to_string(),
        identifier: issue.identifier,
        title: issue.title,
        state: issue.state.map(|s| s.name),
        labels: issue
            .labels
            .map(|l| l.nodes.into_iter().map(|n| n.name).collect())
            .unwrap_or_default(),
    };

    Ok(BranchResolution {
        branch: truncate(&branch, 60),
        issue: Some(issue_ctx),
    })
}

#[derive(Deserialize)]
struct SentryIssue {
    title: String,
}

async fn branch_from_sentry(org: &str, issue_id: &str, parent_dir: &Path) -> Result<BranchResolution> {
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

    let issue_ctx = IssueContext {
        provider: "sentry".to_string(),
        identifier: format!("sentry-{}", issue_id),
        title: issue.title,
        state: None,
        labels: Vec::new(),
    };

    Ok(BranchResolution {
        branch: truncate(&branch, 60),
        issue: Some(issue_ctx),
    })
}

/// Fetch the authenticated user's assigned Linear issues (active states only).
pub async fn list_linear_issues(parent_dir: &Path) -> Result<Vec<LinearIssueSummary>> {
    let token = load_token(parent_dir, "linear_token")?;
    let client = Client::new();

    let graphql_query = r#"{ viewer { assignedIssues(filter: { state: { type: { in: ["started", "unstarted", "backlog"] } } }, first: 50, orderBy: updatedAt) { nodes { identifier title state { name type color } labels { nodes { name color } } } } } }"#;

    let body = serde_json::json!({ "query": graphql_query });

    let resp = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", &token)
        .json(&body)
        .send()
        .await
        .context("failed to call Linear API")?;

    if !resp.status().is_success() {
        bail!("Linear API returned status {}", resp.status());
    }

    let body: LinearViewerResponse = resp.json().await.context("failed to parse Linear response")?;

    let issues = body
        .data
        .and_then(|d| d.viewer)
        .and_then(|v| v.assigned_issues)
        .map(|c| c.nodes)
        .unwrap_or_default();

    let mut summaries: Vec<LinearIssueSummary> = issues
        .into_iter()
        .map(|i| {
            let (state_name, state_type, state_color) = match i.state {
                Some(s) => (s.name, s.state_type, s.color),
                None => ("Unknown".to_string(), "unknown".to_string(), None),
            };
            let labels = i
                .labels
                .map(|l| {
                    l.nodes
                        .into_iter()
                        .map(|n| LinearLabelSummary {
                            name: n.name,
                            color: n.color,
                        })
                        .collect()
                })
                .unwrap_or_default();
            LinearIssueSummary {
                identifier: i.identifier,
                title: i.title,
                state_name,
                state_type,
                state_color,
                labels,
            }
        })
        .collect();

    // Sort: started first, then unstarted, then backlog
    summaries.sort_by_key(|i| state_sort_key(&i.state_type));

    Ok(summaries)
}

/// Generate a branch name from a selected Linear issue.
pub fn branch_name_from_linear_issue(issue: &LinearIssueSummary) -> String {
    let branch = format!("{}-{}", issue.identifier.to_lowercase(), slugify(&issue.title));
    truncate(&branch, 60)
}

/// Build an IssueContext from a LinearIssueSummary (used by the --linear picker path).
pub fn issue_context_from_linear_summary(summary: &LinearIssueSummary) -> IssueContext {
    IssueContext {
        provider: "linear".to_string(),
        identifier: summary.identifier.clone(),
        title: summary.title.clone(),
        state: Some(summary.state_name.clone()),
        labels: summary.labels.iter().map(|l| l.name.clone()).collect(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render text with true color (24-bit) ANSI if a hex color is provided.
pub fn color_text(text: &str, hex: Option<&str>) -> String {
    match hex.and_then(parse_hex_color) {
        Some((r, g, b)) => format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text),
        None => text.to_string(),
    }
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn state_sort_key(state_type: &str) -> u8 {
    match state_type {
        "started" => 0,
        "unstarted" => 1,
        "backlog" => 2,
        _ => 3,
    }
}

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
