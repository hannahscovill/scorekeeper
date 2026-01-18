//! Authentication middleware.

use actix_web::HttpRequest;

use crate::models::AppError;

/// Validates an API key from the request headers.
///
/// This is a placeholder implementation that will be expanded later.
pub fn validate_api_key(req: &HttpRequest) -> Result<(), AppError> {
    // Placeholder: accept any request for now
    let _ = req;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn test_validate_api_key_placeholder() {
        let req = TestRequest::default().to_http_request();
        assert!(validate_api_key(&req).is_ok());
    }
}
