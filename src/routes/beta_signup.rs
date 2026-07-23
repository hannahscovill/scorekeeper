//! Anonymous internal test track signup route handler.
//!
//! Unlike `POST /profile/beta-signup` (authenticated, `src/routes/profile.rs`),
//! this endpoint requires no login — it lets an anonymous visitor give us an
//! email without creating a real account. The email is stored as an Auth0
//! "email" passwordless connection user (no password, never used to log in),
//! reusing the same `user_metadata` opt-in fields and write path
//! (`set_test_track_opt_in`) as the authenticated flow. Because this is a
//! public, unauthenticated form, it needs the same anti-abuse plumbing as
//! `POST /issues`: Turnstile CAPTCHA, per-IP rate limiting, and a honeypot
//! field.

use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::instrument;

use crate::models::error::AppError;
use crate::services::{Auth0ManagementService, GitHubIssueService, RateLimiter};

/// Request body for the anonymous internal test track signup endpoint.
/// At least one of `ios`/`android` must be true.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnonymousBetaSignupRequest {
    pub email: String,
    #[serde(default)]
    pub ios: bool,
    #[serde(default)]
    pub android: bool,
    pub turnstile_token: String,
    #[serde(default)]
    pub website: String, // honeypot
}

/// Response from the anonymous internal test track signup endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnonymousBetaSignupResponse {
    pub opt_in_test_track_ios: bool,
    pub opt_in_test_track_android: bool,
}

/// POST /beta-signup - Opt an anonymous visitor into the internal test track
/// by email, without requiring an account.
///
/// Idempotent per email: repeat submissions for platform(s) already opted
/// into are a no-op (no Auth0 write, no GitHub notification). The GitHub
/// notification never contains the email or any hint of it — only the Auth0
/// user ID, which is enough to look the signup up in the Auth0 dashboard.
#[post("/beta-signup")]
#[instrument(
    name = "anonymous_beta_signup",
    skip(body, auth0_service, github_issue_service, rate_limiter)
)]
pub async fn anonymous_beta_signup(
    req: HttpRequest,
    body: web::Json<AnonymousBetaSignupRequest>,
    auth0_service: web::Data<Auth0ManagementService>,
    github_issue_service: web::Data<GitHubIssueService>,
    rate_limiter: web::Data<RateLimiter>,
) -> Result<HttpResponse, AppError> {
    // Rate limiting: 5 signups per IP per hour (dedicated limiter, separate
    // quota from the /issues bug-report endpoint).
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

    // Honeypot check — if filled, silently return 200 without doing anything.
    if !body.website.is_empty() {
        tracing::info!("Honeypot triggered from IP {}", source_ip);
        return Ok(HttpResponse::Ok().json(AnonymousBetaSignupResponse {
            opt_in_test_track_ios: false,
            opt_in_test_track_android: false,
        }));
    }

    // Cheap sanity check, not full RFC validation.
    if body.email.trim().is_empty() || !body.email.contains('@') {
        return Err(AppError::bad_request("A valid email is required"));
    }
    if !body.ios && !body.android {
        return Err(AppError::bad_request(
            "At least one of ios or android must be true",
        ));
    }

    // CAPTCHA verification — the only anti-abuse gate on this public form
    // beyond rate limiting/honeypot, so it's a hard requirement (not
    // best-effort like the GitHub notification below).
    github_issue_service
        .verify_turnstile(&body.turnstile_token)
        .await?;

    let email = body.email.trim();

    let existing_lead = auth0_service.find_email_connection_user(email).await?;

    let (user_id, final_ios, final_android, has_new_signup) = match existing_lead {
        Some(existing) => {
            let metadata = existing.user_metadata.as_ref();
            let existing_ios = metadata.map(|m| m.opt_in_test_track_ios).unwrap_or(false);
            let existing_android = metadata
                .map(|m| m.opt_in_test_track_android)
                .unwrap_or(false);

            let newly_ios = body.ios && !existing_ios;
            let newly_android = body.android && !existing_android;

            if !newly_ios && !newly_android {
                // Nothing new — idempotent no-op, skip write and notification.
                (existing.user_id, existing_ios, existing_android, false)
            } else {
                let ts = chrono::Utc::now().to_rfc3339();
                auth0_service
                    .set_test_track_opt_in(
                        &existing.user_id,
                        newly_ios.then_some(ts.as_str()),
                        newly_android.then_some(ts.as_str()),
                    )
                    .await?;
                (
                    existing.user_id,
                    existing_ios || newly_ios,
                    existing_android || newly_android,
                    true,
                )
            }
        }
        None => {
            // Brand-new lead — always something new.
            let ts = chrono::Utc::now().to_rfc3339();
            let ios_ts = body.ios.then_some(ts.as_str());
            let android_ts = body.android.then_some(ts.as_str());
            let created = auth0_service
                .create_email_connection_lead(email, ios_ts, android_ts)
                .await?;
            (created.user_id, body.ios, body.android, true)
        }
    };

    if has_new_signup {
        // Best-effort GitHub notification — never blocks or fails the
        // response. reporter_display_name is always None: a passwordless
        // lead has no display name, and the email itself must never appear
        // in the issue.
        if let Err(e) = github_issue_service
            .notify_beta_signup(None, &user_id, final_ios, final_android)
            .await
        {
            tracing::warn!("Failed to notify GitHub of anonymous beta signup: {}", e);
        }
    }

    Ok(HttpResponse::Ok().json(AnonymousBetaSignupResponse {
        opt_in_test_track_ios: final_ios,
        opt_in_test_track_android: final_android,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_beta_signup_request_deserialization() {
        let json = r#"{"email": "test@example.com", "ios": true, "turnstileToken": "tok"}"#;
        let request: AnonymousBetaSignupRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.email, "test@example.com");
        assert!(request.ios);
        assert!(!request.android);
        assert_eq!(request.turnstile_token, "tok");
        assert_eq!(request.website, ""); // honeypot defaults to empty
    }

    #[test]
    fn test_anonymous_beta_signup_response_serialization() {
        let response = AnonymousBetaSignupResponse {
            opt_in_test_track_ios: true,
            opt_in_test_track_android: false,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"optInTestTrackIos\":true"));
        assert!(json.contains("\"optInTestTrackAndroid\":false"));
    }

    #[test]
    fn test_idempotency_logic_no_new_platforms() {
        // Mirrors the handler's "nothing new" branch condition.
        let existing_ios = true;
        let existing_android = true;
        let requested_ios = true;
        let requested_android = false;

        let newly_ios = requested_ios && !existing_ios;
        let newly_android = requested_android && !existing_android;

        assert!(!newly_ios);
        assert!(!newly_android);
    }

    #[test]
    fn test_idempotency_logic_additive_new_platform() {
        let existing_ios = true;
        let existing_android = false;
        let requested_ios = true;
        let requested_android = true;

        let newly_ios = requested_ios && !existing_ios;
        let newly_android = requested_android && !existing_android;

        assert!(!newly_ios);
        assert!(newly_android);
    }
}
