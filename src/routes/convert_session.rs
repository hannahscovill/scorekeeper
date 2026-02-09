//! Route handler for converting anonymous session games to authenticated user.

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument;

use crate::db::PuzzleDatabase;
use crate::middleware::auth::Claims;
use crate::models::error::AppError;
use crate::models::guess::GameState;

#[derive(Debug, Deserialize)]
pub struct ConvertSessionRequest {
    session_id: String,
}

#[derive(Debug, Serialize)]
pub struct ConvertSessionResponse {
    converted: usize,
    conflicts_resolved: usize,
    puzzle_dates_affected: Vec<String>,
}

/// POST /convert-session - Convert anonymous session games to authenticated user.
///
/// Requires JWT authentication (Claims extractor).
/// Moves all game states from the session_id to the authenticated user's account.
#[post("/convert-session")]
#[instrument(
    name = "convert_session",
    skip(claims, body, puzzle_db),
    fields(
        user_id = %claims.sub,
        session_id = %body.session_id,
        games_found = tracing::field::Empty,
        games_converted = tracing::field::Empty,
        conflicts_resolved = tracing::field::Empty,
    )
)]
pub async fn convert_session(
    claims: Claims,
    body: web::Json<ConvertSessionRequest>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    let auth_user_id = &claims.sub;
    let session_id = &body.session_id;

    // Validate session_id is a UUID format (reject anything that looks like an Auth0 sub)
    if session_id.contains('|') {
        return Err(AppError::bad_request(
            "Invalid session_id: must be a session cookie UUID, not an auth provider ID",
        ));
    }

    if uuid::Uuid::parse_str(session_id).is_err() {
        return Err(AppError::bad_request(
            "Invalid session_id: must be a valid UUID",
        ));
    }

    // Fetch all game states for the session user
    let session_games = puzzle_db
        .get_user_game_states(session_id)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    tracing::Span::current().record("games_found", session_games.len());

    let mut converted: usize = 0;
    let mut conflicts_resolved: usize = 0;
    let mut puzzle_dates_affected: Vec<String> = Vec::new();

    for session_game in &session_games {
        let puzzle_date = session_game.puzzle_date;

        // Check if the auth user already has a game for this puzzle date
        let auth_game = puzzle_db
            .get_game_state(auth_user_id, puzzle_date)
            .await
            .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

        match auth_game {
            None => {
                // No conflict: copy session game to auth user
                let auth_game_state = GameState {
                    user_id: auth_user_id.clone(),
                    puzzle_date: session_game.puzzle_date,
                    guesses: session_game.guesses.clone(),
                    won: session_game.won,
                    created_at: session_game.created_at,
                    updated_at: session_game.updated_at,
                };
                puzzle_db
                    .upsert_game_state(&auth_game_state)
                    .await
                    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

                tracing::info!(
                    puzzle_date = %puzzle_date,
                    "Converted session game to auth user (no conflict)"
                );
                converted += 1;
                puzzle_dates_affected.push(puzzle_date.to_string());
            }
            Some(existing_auth_game) => {
                // Conflict: keep whichever game was created first
                if session_game.created_at < existing_auth_game.created_at {
                    // Session game is older — replace auth game with session game data
                    let replacement = GameState {
                        user_id: auth_user_id.clone(),
                        puzzle_date: session_game.puzzle_date,
                        guesses: session_game.guesses.clone(),
                        won: session_game.won,
                        created_at: session_game.created_at,
                        updated_at: session_game.updated_at,
                    };
                    puzzle_db
                        .upsert_game_state(&replacement)
                        .await
                        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

                    tracing::info!(
                        puzzle_date = %puzzle_date,
                        session_created_at = %session_game.created_at,
                        auth_created_at = %existing_auth_game.created_at,
                        "Conflict resolved: session game older, replaced auth game"
                    );
                    converted += 1;
                    puzzle_dates_affected.push(puzzle_date.to_string());
                } else {
                    tracing::info!(
                        puzzle_date = %puzzle_date,
                        session_created_at = %session_game.created_at,
                        auth_created_at = %existing_auth_game.created_at,
                        "Conflict resolved: auth game older or same, keeping auth game"
                    );
                }
                conflicts_resolved += 1;
            }
        }

        // Always delete the session game
        puzzle_db
            .delete_game_state(session_id, puzzle_date)
            .await
            .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;
    }

    tracing::Span::current().record("games_converted", converted);
    tracing::Span::current().record("conflicts_resolved", conflicts_resolved);

    Ok(HttpResponse::Ok().json(ConvertSessionResponse {
        converted,
        conflicts_resolved,
        puzzle_dates_affected,
    }))
}

#[cfg(test)]
mod tests {
    use crate::db::InMemoryPuzzleDb;
    use crate::db::PuzzleDatabase;
    use crate::models::guess::GameState;
    use chrono::{DateTime, NaiveDate, Utc};
    use std::sync::Arc;

    async fn setup_db() -> Arc<InMemoryPuzzleDb> {
        Arc::new(InMemoryPuzzleDb::new())
    }

    #[tokio::test]
    async fn test_no_session_games() {
        let db = setup_db().await;
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";

        let session_games = db.get_user_game_states(session_id).await.unwrap();
        assert!(session_games.is_empty());
    }

    #[tokio::test]
    async fn test_simple_conversion() {
        let db = setup_db().await;
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        let auth_user_id = "auth0|abc123";
        let date = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();

        let mut session_game = GameState::new(session_id, date);
        session_game.add_guess("crane");
        session_game.add_guess("slate");
        db.upsert_game_state(&session_game).await.unwrap();

        assert!(db.get_game_state(session_id, date).await.unwrap().is_some());
        assert!(db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .is_none());

        // Simulate conversion
        let session_games = db.get_user_game_states(session_id).await.unwrap();
        assert_eq!(session_games.len(), 1);

        for sg in &session_games {
            let new_game = GameState {
                user_id: auth_user_id.to_string(),
                puzzle_date: sg.puzzle_date,
                guesses: sg.guesses.clone(),
                won: sg.won,
                created_at: sg.created_at,
                updated_at: sg.updated_at,
            };
            db.upsert_game_state(&new_game).await.unwrap();
            db.delete_game_state(session_id, sg.puzzle_date)
                .await
                .unwrap();
        }

        assert!(db.get_game_state(session_id, date).await.unwrap().is_none());
        let auth_game = db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth_game.guesses, vec!["crane", "slate"]);
    }

    #[tokio::test]
    async fn test_conflict_keep_auth_older() {
        let db = setup_db().await;
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        let auth_user_id = "auth0|abc123";
        let date = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();

        // Auth game created first (older)
        let mut auth_game = GameState::new(auth_user_id, date);
        auth_game.created_at = DateTime::parse_from_rfc3339("2026-02-07T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        auth_game.add_guess("crane");
        auth_game.add_guess("slate");
        db.upsert_game_state(&auth_game).await.unwrap();

        // Session game created later (newer)
        let mut session_game = GameState::new(session_id, date);
        session_game.created_at = DateTime::parse_from_rfc3339("2026-02-08T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        session_game.add_guess("moist");
        session_game.add_guess("brain");
        session_game.add_guess("plant");
        session_game.add_guess("world");
        db.upsert_game_state(&session_game).await.unwrap();

        let existing_auth = db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .unwrap();
        let existing_session = db.get_game_state(session_id, date).await.unwrap().unwrap();
        assert!(existing_auth.created_at < existing_session.created_at);

        // Auth game is older — keep it, delete session game
        db.delete_game_state(session_id, date).await.unwrap();

        let auth_after = db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth_after.guesses, vec!["crane", "slate"]);
    }

    #[tokio::test]
    async fn test_conflict_keep_session_older() {
        let db = setup_db().await;
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        let auth_user_id = "auth0|abc123";
        let date = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();

        // Session game created first (older)
        let mut session_game = GameState::new(session_id, date);
        session_game.created_at = DateTime::parse_from_rfc3339("2026-02-08T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        session_game.add_guess("crane");
        session_game.add_guess("slate");
        session_game.add_guess("moist");
        db.upsert_game_state(&session_game).await.unwrap();

        // Auth game created later (newer)
        let mut auth_game = GameState::new(auth_user_id, date);
        auth_game.created_at = DateTime::parse_from_rfc3339("2026-02-08T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        auth_game.add_guess("brain");
        db.upsert_game_state(&auth_game).await.unwrap();

        let existing_session = db.get_game_state(session_id, date).await.unwrap().unwrap();
        let existing_auth = db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .unwrap();
        assert!(existing_session.created_at < existing_auth.created_at);

        // Replace auth game with session game data
        let replacement = GameState {
            user_id: auth_user_id.to_string(),
            puzzle_date: existing_session.puzzle_date,
            guesses: existing_session.guesses.clone(),
            won: existing_session.won,
            created_at: existing_session.created_at,
            updated_at: existing_session.updated_at,
        };
        db.upsert_game_state(&replacement).await.unwrap();
        db.delete_game_state(session_id, date).await.unwrap();

        let auth_after = db
            .get_game_state(auth_user_id, date)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth_after.guesses, vec!["crane", "slate", "moist"]);
        assert!(db.get_game_state(session_id, date).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_multiple_puzzle_dates() {
        let db = setup_db().await;
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        let auth_user_id = "auth0|abc123";
        let date1 = NaiveDate::from_ymd_opt(2026, 2, 6).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2026, 2, 7).unwrap();
        let date3 = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();

        for date in [date1, date2, date3] {
            let mut game = GameState::new(session_id, date);
            game.add_guess("crane");
            db.upsert_game_state(&game).await.unwrap();
        }

        let session_games = db.get_user_game_states(session_id).await.unwrap();
        assert_eq!(session_games.len(), 3);

        for sg in &session_games {
            let new_game = GameState {
                user_id: auth_user_id.to_string(),
                puzzle_date: sg.puzzle_date,
                guesses: sg.guesses.clone(),
                won: sg.won,
                created_at: sg.created_at,
                updated_at: sg.updated_at,
            };
            db.upsert_game_state(&new_game).await.unwrap();
            db.delete_game_state(session_id, sg.puzzle_date)
                .await
                .unwrap();
        }

        let auth_games = db.get_user_game_states(auth_user_id).await.unwrap();
        assert_eq!(auth_games.len(), 3);
        assert!(db
            .get_user_game_states(session_id)
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_invalid_session_id_with_pipe() {
        let session_id = "auth0|abc123";
        assert!(session_id.contains('|'));
    }

    #[test]
    fn test_valid_session_id_uuid() {
        let session_id = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        assert!(uuid::Uuid::parse_str(session_id).is_ok());
        assert!(!session_id.contains('|'));
    }
}
