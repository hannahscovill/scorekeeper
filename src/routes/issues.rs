//! Issue reporting route handler.

use actix_web::{post, web, HttpRequest, HttpResponse};
use std::time::Duration;
use tracing::instrument;

use crate::middleware::auth::Claims;
use crate::models::error::AppError;
use crate::services::auth0::Auth0ManagementService;
use crate::services::github_issues::IssueRequest;
use crate::services::{GitHubIssueService, RateLimiter};

/// POST /issues - Create a GitHub issue on behalf of an authenticated user.
#[post("/issues")]
#[instrument(name = "create_issue", skip(body, service, rate_limiter, auth0_service), fields(user_id = %claims.sub))]
pub async fn create_issue(
    req: HttpRequest,
    claims: Claims,
    body: web::Json<IssueRequest>,
    service: web::Data<GitHubIssueService>,
    rate_limiter: web::Data<RateLimiter>,
    auth0_service: web::Data<Auth0ManagementService>,
) -> Result<HttpResponse, AppError> {
    // Rate limiting: 5 issues per IP per hour
    let source_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if !rate_limiter.check(&source_ip, 5, Duration::from_secs(3600)) {
        return Err(AppError::too_many_requests(
            "Rate limit exceeded. Please try again later.",
        ));
    }

    // Validate required fields
    if body.title.trim().is_empty() || body.description.trim().is_empty() {
        return Err(AppError::bad_request(
            "Title and description are required",
        ));
    }

    // Honeypot check — if filled, silently return 200
    if !body.website.is_empty() {
        tracing::info!("Honeypot triggered from IP {}", source_ip);
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "issueNumber": 0,
            "issueUrl": ""
        })));
    }

    // Resolve reporter info from JWT claims + Auth0 user profile
    let user_id = claims.sub.clone();
    let display_name = match auth0_service.get_user(&user_id).await {
        Ok(user) => user
            .user_metadata
            .map(|m| m.display_name)
            .filter(|n| !n.is_empty()),
        Err(e) => {
            tracing::warn!("Failed to fetch reporter profile: {}", e);
            None
        }
    };

    // Delegate to service (Turnstile verification + GitHub API call)
    let response = service
        .create_issue(&body, display_name.as_deref(), Some(&user_id))
        .await?;

    tracing::info!("Created issue #{} from IP {}", response.issue_number, source_ip);

    Ok(HttpResponse::Created().json(response))
}
