//! GitHub issue creation service.
//!
//! Ported from the standalone Lambda at `fe/wordles-with-friends-client-web/issue-proxy/`.
//! Creates GitHub issues via GitHub App auth (production) or PAT (local dev),
//! with Turnstile CAPTCHA verification and per-IP rate limiting.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::models::AppError;

// ── Request / Response types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRequest {
    pub issue_type: String,
    pub title: String,
    pub description: String,
    pub turnstile_token: String,
    #[serde(default)]
    pub website: String, // honeypot
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub posthog_session_id: Option<String>,
    #[serde(default)]
    pub client_environment_name: Option<String>,
    #[serde(default)]
    pub client_commit_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueResponse {
    pub issue_number: u64,
    pub issue_url: String,
}

// ── Turnstile verification ──────────────────────────────────────────

#[derive(Deserialize)]
struct TurnstileVerifyResponse {
    success: bool,
}

// ── GitHub App authentication ───────────────────────────────────────

#[derive(Serialize)]
struct JwtClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

#[derive(Deserialize)]
struct InstallationTokenResponse {
    token: String,
}

// ── GitHub issue creation ───────────────────────────────────────────

#[derive(Serialize)]
struct GitHubCreateIssue {
    title: String,
    body: String,
    labels: Vec<String>,
}

#[derive(Deserialize)]
struct GitHubIssueResponse {
    number: u64,
    html_url: String,
}

// ── Issue templates (embedded at compile time) ──────────────────────

const TEMPLATE_BUG: &str = include_str!("../../templates/bug.md");
const TEMPLATE_FEATURE: &str = include_str!("../../templates/feature.md");
const TEMPLATE_QUESTION: &str = include_str!("../../templates/question.md");
const TEMPLATE_FOOTER: &str = include_str!("../../templates/footer.md");

struct IssueTemplate {
    title_prefix: String,
    label: String,
    body: String,
}

fn parse_template(raw: &str) -> IssueTemplate {
    let mut title_prefix = String::new();
    let mut label = String::new();
    let mut body = raw.to_string();

    // Parse YAML frontmatter (between --- delimiters)
    if let Some(stripped) = raw.strip_prefix("---") {
        if let Some(end) = stripped.find("---") {
            let frontmatter = &stripped[..end];
            body = stripped[end + 3..].trim_start().to_string();

            for line in frontmatter.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let value = value.trim().trim_matches('"');
                    match key.trim() {
                        "title_prefix" => title_prefix = value.to_string(),
                        "label" => label = value.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    IssueTemplate {
        title_prefix,
        label,
        body,
    }
}

fn get_template(issue_type: &str) -> IssueTemplate {
    let raw = match issue_type {
        "bug" => TEMPLATE_BUG,
        "feature" => TEMPLATE_FEATURE,
        _ => TEMPLATE_QUESTION,
    };
    parse_template(raw)
}

fn build_issue_body(
    issue_type: &str,
    description: &str,
    reporter_display_name: Option<&str>,
    reporter_user_id: &str,
    posthog_session_id: Option<&str>,
    user_agent: Option<&str>,
    page_url: Option<&str>,
    client_environment_name: Option<&str>,
    client_commit_hash: Option<&str>,
    server_environment_name: Option<&str>,
    server_commit_hash: Option<&str>,
) -> String {
    let template = get_template(issue_type);
    let body = template.body.replace("{description}", description);
    let mut result = body.trim_end().to_string();

    // Reporter section
    result.push_str("\n\n## Reporter\n");
    if let Some(name) = reporter_display_name {
        result.push_str(&format!("- Display Name: {}\n", name));
    }
    result.push_str(&format!("- User ID: `{}`\n", reporter_user_id));
    if let Some(session_id) = posthog_session_id {
        result.push_str(&format!("- Posthog Session ID: `{}`\n", session_id));
    }

    // Environment section
    let has_client = user_agent.is_some()
        || page_url.is_some()
        || client_environment_name.is_some()
        || client_commit_hash.is_some();
    let has_server = server_environment_name.is_some() || server_commit_hash.is_some();

    if has_client || has_server {
        result.push_str("\n## Environment\n");

        if has_client {
            result.push_str("\n### Client\n");
            if let Some(ua) = user_agent {
                result.push_str(&format!("- Browser: {}\n", ua));
            }
            if let Some(url) = page_url {
                result.push_str(&format!("- URL: {}\n", url));
            }
            if let Some(env) = client_environment_name {
                result.push_str(&format!("- Environment Name: `{}`\n", env));
            }
            if let Some(hash) = client_commit_hash {
                result.push_str(&format!("- Commit Hash: `{}`\n", hash));
            }
        }

        if has_server {
            result.push_str("\n### Server\n");
            if let Some(env) = server_environment_name {
                result.push_str(&format!("- Environment Name: `{}`\n", env));
            }
            if let Some(hash) = server_commit_hash {
                result.push_str(&format!("- Commit Hash: `{}`\n", hash));
            }
        }
    }

    format!("{}{}", result.trim_end(), TEMPLATE_FOOTER)
}

fn issue_label(issue_type: &str) -> String {
    get_template(issue_type).label
}

fn issue_title_prefix(issue_type: &str) -> String {
    get_template(issue_type).title_prefix
}

// ── Rate limiting (in-memory, per server instance) ──────────────────

pub struct RateLimiter {
    requests: Mutex<HashMap<String, Vec<Instant>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
        }
    }

    pub fn check(&self, ip: &str, max_requests: usize, window: Duration) -> bool {
        let mut map = self.requests.lock().unwrap();
        let now = Instant::now();
        let entries = map.entry(ip.to_string()).or_default();

        // Remove expired entries
        entries.retain(|t| now.duration_since(*t) < window);

        if entries.len() >= max_requests {
            return false;
        }

        entries.push(now);
        true
    }
}

// ── GitHub Issue Service ────────────────────────────────────────────

pub struct GitHubIssueService {
    client: reqwest::Client,
    github_app_id: Option<String>,
    github_installation_id: Option<String>,
    github_private_key: Option<String>,
    github_token: Option<String>,
    github_repo: String,
    turnstile_secret_key: Option<String>,
    turnstile_verify_url: String,
    server_commit_hash: Option<String>,
    server_environment_name: Option<String>,
}

impl GitHubIssueService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        github_app_id: Option<String>,
        github_installation_id: Option<String>,
        github_private_key: Option<String>,
        github_token: Option<String>,
        github_repo: String,
        turnstile_secret_key: Option<String>,
        turnstile_verify_url: String,
        server_commit_hash: Option<String>,
        server_environment_name: Option<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            github_app_id,
            github_installation_id,
            github_private_key,
            github_token,
            github_repo,
            turnstile_secret_key,
            turnstile_verify_url,
            server_commit_hash,
            server_environment_name,
        }
    }

    /// Verify a Turnstile CAPTCHA token. Returns Ok(()) on success.
    pub async fn verify_turnstile(&self, token: &str) -> Result<(), AppError> {
        let secret = match &self.turnstile_secret_key {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(()), // No secret configured, skip verification
        };

        let resp = self
            .client
            .post(&self.turnstile_verify_url)
            .form(&[("secret", secret.as_str()), ("response", token)])
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Turnstile verification error: {}", e);
                AppError::internal("Verification service error")
            })?
            .json::<TurnstileVerifyResponse>()
            .await
            .map_err(|e| {
                tracing::error!("Turnstile response parse error: {}", e);
                AppError::internal("Verification service error")
            })?;

        if resp.success {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Turnstile verification failed".to_string(),
            ))
        }
    }

    /// Get a GitHub installation token via GitHub App JWT auth.
    async fn get_installation_token(
        &self,
        app_id: &str,
        installation_id: &str,
        private_key: &str,
    ) -> Result<String, AppError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            iat: now - 60,  // 1 minute in the past to account for clock drift
            exp: now + 600, // 10 minutes
            iss: app_id.to_string(),
        };

        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|e| AppError::internal(format!("Invalid private key: {}", e)))?;

        let jwt = encode(&header, &claims, &key)
            .map_err(|e| AppError::internal(format!("JWT encoding failed: {}", e)))?;

        let resp = self
            .client
            .post(format!(
                "https://api.github.com/app/installations/{}/access_tokens",
                installation_id
            ))
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "wordles-issue-proxy/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| AppError::internal(format!("GitHub API request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::internal(format!(
                "Failed to get installation token: {} - {}",
                status, body
            )));
        }

        let token_resp: InstallationTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::internal(format!("Failed to parse token response: {}", e)))?;

        Ok(token_resp.token)
    }

    /// Resolve the GitHub token: PAT for local dev, or GitHub App for production.
    async fn get_github_token(&self) -> Result<String, AppError> {
        // Prefer PAT (local dev)
        if let Some(pat) = &self.github_token {
            return Ok(pat.clone());
        }

        // Fall back to GitHub App auth
        match (
            &self.github_app_id,
            &self.github_installation_id,
            &self.github_private_key,
        ) {
            (Some(app_id), Some(installation_id), Some(private_key)) if !private_key.is_empty() => {
                self.get_installation_token(app_id, installation_id, private_key)
                    .await
            }
            _ => {
                tracing::error!("No GitHub credentials configured");
                Err(AppError::internal("Server configuration error"))
            }
        }
    }

    /// Create a GitHub issue. Handles Turnstile verification and GitHub API call.
    /// Reporter info is resolved from the authenticated user's JWT and Auth0 profile.
    pub async fn create_issue(
        &self,
        request: &IssueRequest,
        reporter_display_name: Option<&str>,
        reporter_user_id: &str,
    ) -> Result<IssueResponse, AppError> {
        // Verify Turnstile token
        self.verify_turnstile(&request.turnstile_token).await?;

        // Get GitHub token
        let github_token = self.get_github_token().await?;

        let full_title = format!(
            "{} {}",
            issue_title_prefix(&request.issue_type),
            request.title
        );

        let create_issue = GitHubCreateIssue {
            title: full_title,
            body: build_issue_body(
                &request.issue_type,
                &request.description,
                reporter_display_name,
                reporter_user_id,
                request.posthog_session_id.as_deref(),
                request.user_agent.as_deref(),
                request.page_url.as_deref(),
                request.client_environment_name.as_deref(),
                request.client_commit_hash.as_deref(),
                self.server_environment_name.as_deref(),
                self.server_commit_hash.as_deref(),
            ),
            labels: vec![issue_label(&request.issue_type)],
        };

        let github_resp = self
            .client
            .post(format!(
                "https://api.github.com/repos/{}/issues",
                self.github_repo
            ))
            .header("Authorization", format!("Bearer {}", github_token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "wordles-issue-proxy/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&create_issue)
            .send()
            .await
            .map_err(|e| AppError::bad_gateway(format!("GitHub API request failed: {}", e)))?;

        if !github_resp.status().is_success() {
            let status = github_resp.status();
            let body_text = github_resp.text().await.unwrap_or_default();
            tracing::error!("GitHub API error: {} - {}", status, body_text);
            return Err(AppError::bad_gateway("Failed to create issue"));
        }

        let issue: GitHubIssueResponse = github_resp.json().await.map_err(|e| {
            AppError::bad_gateway(format!("Failed to parse GitHub response: {}", e))
        })?;

        tracing::info!("Created issue #{}", issue.number);

        Ok(IssueResponse {
            issue_number: issue.number,
            issue_url: issue.html_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_template_has_required_fields() {
        let template = get_template("bug");
        assert!(
            !template.title_prefix.is_empty(),
            "bug template missing title_prefix"
        );
        assert!(!template.label.is_empty(), "bug template missing label");
        assert!(
            template.body.contains("{description}"),
            "bug template missing {{description}} placeholder"
        );
    }

    #[test]
    fn feature_template_has_required_fields() {
        let template = get_template("feature");
        assert!(
            !template.title_prefix.is_empty(),
            "feature template missing title_prefix"
        );
        assert!(!template.label.is_empty(), "feature template missing label");
        assert!(
            template.body.contains("{description}"),
            "feature template missing {{description}} placeholder"
        );
    }

    #[test]
    fn question_template_has_required_fields() {
        let template = get_template("question");
        assert!(
            !template.title_prefix.is_empty(),
            "question template missing title_prefix"
        );
        assert!(
            !template.label.is_empty(),
            "question template missing label"
        );
        assert!(
            template.body.contains("{description}"),
            "question template missing {{description}} placeholder"
        );
    }

    #[test]
    fn build_issue_body_replaces_description() {
        let body = build_issue_body(
            "bug",
            "Test description here",
            None,
            "auth0|test",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            body.contains("Test description here"),
            "description not inserted into body"
        );
        assert!(!body.contains("{description}"), "placeholder not replaced");
    }

    #[test]
    fn build_issue_body_includes_footer() {
        let body = build_issue_body(
            "bug",
            "Test",
            None,
            "auth0|test",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(body.contains("Submitted via"), "footer not appended");
    }

    #[test]
    fn build_issue_body_includes_client_environment_section() {
        let body = build_issue_body(
            "bug",
            "Test",
            None,
            "auth0|test",
            None,
            Some("Mozilla/5.0 (Macintosh)"),
            Some("https://wordles.dev/gamemaker"),
            Some("production"),
            Some("abc1234"),
            None,
            None,
        );
        assert!(
            body.contains("## Environment"),
            "missing environment section"
        );
        assert!(body.contains("### Client"), "missing client subsection");
        assert!(
            body.contains("- Browser: Mozilla/5.0 (Macintosh)"),
            "missing browser info"
        );
        assert!(
            body.contains("- URL: https://wordles.dev/gamemaker"),
            "missing page URL"
        );
        assert!(
            body.contains("- Environment Name: `production`"),
            "missing client environment name"
        );
        assert!(
            body.contains("- Commit Hash: `abc1234`"),
            "missing client commit hash"
        );
    }

    #[test]
    fn build_issue_body_includes_server_environment_section() {
        let body = build_issue_body(
            "bug",
            "Test",
            None,
            "auth0|test",
            None,
            None,
            None,
            None,
            None,
            Some("production"),
            Some("def5678"),
        );
        assert!(
            body.contains("## Environment"),
            "missing environment section"
        );
        assert!(body.contains("### Server"), "missing server subsection");
        assert!(
            body.contains("- Environment Name: `production`"),
            "missing server environment name"
        );
        assert!(
            body.contains("- Commit Hash: `def5678`"),
            "missing server commit hash"
        );
    }

    #[test]
    fn build_issue_body_includes_reporter_section() {
        let body = build_issue_body(
            "bug",
            "Test",
            Some("Hannah"),
            "auth0|abc123",
            Some("posthog-session-123"),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(body.contains("## Reporter"), "missing reporter section");
        assert!(
            body.contains("- Display Name: Hannah"),
            "missing reporter display name"
        );
        assert!(
            body.contains("- User ID: `auth0|abc123`"),
            "missing reporter user ID"
        );
        assert!(
            body.contains("- Posthog Session ID: `posthog-session-123`"),
            "missing posthog session ID"
        );
    }

    #[test]
    fn build_issue_body_always_includes_reporter_user_id() {
        let body = build_issue_body(
            "bug",
            "Test",
            None,
            "auth0|xyz789",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            body.contains("## Reporter"),
            "reporter section should always appear"
        );
        assert!(
            body.contains("- User ID: `auth0|xyz789`"),
            "user ID should always be present"
        );
        assert!(
            !body.contains("- Display Name:"),
            "display name should be absent when not provided"
        );
    }

    #[test]
    fn build_issue_body_omits_environment_when_absent() {
        let body = build_issue_body(
            "bug",
            "Test",
            None,
            "auth0|test",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            !body.contains("## Environment"),
            "environment section should not appear"
        );
    }

    #[test]
    fn build_issue_body_includes_both_client_and_server() {
        let body = build_issue_body(
            "bug",
            "Test",
            Some("hannah"),
            "auth0|123abc",
            Some("posthog-session-id"),
            Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
            Some("http://localhost:3000/profile"),
            Some("production"),
            Some("123abc0"),
            Some("production"),
            Some("123abc0"),
        );
        assert!(body.contains("## Reporter"), "missing reporter section");
        assert!(
            body.contains("- Display Name: hannah"),
            "missing display name"
        );
        assert!(
            body.contains("- User ID: `auth0|123abc`"),
            "missing user ID"
        );
        assert!(
            body.contains("- Posthog Session ID: `posthog-session-id`"),
            "missing posthog session ID"
        );
        assert!(body.contains("### Client"), "missing client subsection");
        assert!(body.contains("### Server"), "missing server subsection");
        assert!(body.contains("Submitted via"), "missing footer");
    }

    #[test]
    fn parse_template_handles_frontmatter() {
        let raw = "---\ntitle_prefix: [Test]\nlabel: test-label\n---\n\nBody content";
        let template = parse_template(raw);
        assert_eq!(template.title_prefix, "[Test]");
        assert_eq!(template.label, "test-label");
        assert_eq!(template.body, "Body content");
    }

    #[test]
    fn parse_template_handles_missing_frontmatter() {
        let raw = "Just a body without frontmatter";
        let template = parse_template(raw);
        assert!(template.title_prefix.is_empty());
        assert!(template.label.is_empty());
        assert_eq!(template.body, raw);
    }

    #[test]
    fn rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new();
        let window = Duration::from_secs(3600);

        for _ in 0..5 {
            assert!(limiter.check("192.168.1.1", 5, window));
        }
    }

    #[test]
    fn rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new();
        let window = Duration::from_secs(3600);

        for _ in 0..5 {
            assert!(limiter.check("192.168.1.1", 5, window));
        }
        // 6th request should be blocked
        assert!(!limiter.check("192.168.1.1", 5, window));
    }

    #[test]
    fn rate_limiter_tracks_ips_independently() {
        let limiter = RateLimiter::new();
        let window = Duration::from_secs(3600);

        for _ in 0..5 {
            limiter.check("192.168.1.1", 5, window);
        }
        // Different IP should still be allowed
        assert!(limiter.check("192.168.1.2", 5, window));
    }

    #[test]
    fn unknown_issue_type_falls_back_to_question() {
        let template = get_template("unknown");
        let question = get_template("question");
        assert_eq!(template.title_prefix, question.title_prefix);
        assert_eq!(template.label, question.label);
    }

    #[test]
    fn issue_title_prefix_matches_template() {
        assert_eq!(issue_title_prefix("bug"), get_template("bug").title_prefix);
        assert_eq!(
            issue_title_prefix("feature"),
            get_template("feature").title_prefix
        );
    }

    #[test]
    fn issue_label_matches_template() {
        assert_eq!(issue_label("bug"), get_template("bug").label);
        assert_eq!(issue_label("feature"), get_template("feature").label);
    }
}
