use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

use crate::auth::JwtManager;
use crate::cache::RedisClient;
use crate::db::TokenRepository;

/// Refresh token request payload
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// Refresh token response
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Refresh token error types
#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("Invalid refresh token")]
    InvalidToken,
    
    #[error("Refresh token has expired")]
    TokenExpired,
    
    #[error("Refresh token has been revoked")]
    TokenRevoked,
    
    #[error("Token is blacklisted")]
    TokenBlacklisted,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Token generation failed: {0}")]
    TokenError(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Refresh access token using refresh token
/// 
/// # Arguments
/// * `pool` - Database connection pool
/// * `redis_client` - Redis client for blacklist checking
/// * `jwt_manager` - JWT token manager
/// * `request` - Refresh request data
/// 
/// # Returns
/// * `Result<RefreshResponse, RefreshError>` - New access token or error
/// 
/// # Example
/// ```
/// let request = RefreshRequest {
///     refresh_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...".to_string(),
/// };
/// 
/// let response = refresh_token(
///     &pool,
///     &redis_client,
///     &jwt_manager,
///     request
/// ).await?;
/// ```
pub async fn refresh_token(
    pool: &PgPool,
    redis_client: &RedisClient,
    jwt_manager: &JwtManager,
    request: RefreshRequest,
) -> Result<RefreshResponse, RefreshError> {
    // Decode and validate refresh token
    let claims = jwt_manager.validate_token(&request.refresh_token)
        .map_err(|_| RefreshError::InvalidToken)?;

    // Check token type
    if claims.token_type != "refresh" {
        log::warn!("Attempted to refresh using non-refresh token");
        return Err(RefreshError::InvalidToken);
    }

    // Check if token is blacklisted in Redis
    let is_blacklisted = redis_client.is_token_blacklisted(&request.refresh_token)
        .await
        .map_err(|e| RefreshError::CacheError(e.to_string()))?;

    if is_blacklisted {
        log::warn!("Attempted to use blacklisted refresh token");
        return Err(RefreshError::TokenBlacklisted);
    }

    // Hash the token to check database
    let token_hash = hash_token(&request.refresh_token);

    // Verify refresh token exists in database and is not expired
    let token_repo = TokenRepository::new(pool);
    let stored_token = token_repo.verify_refresh_token(&token_hash)
        .await
        .map_err(|e| match e {
            crate::db::token::TokenError::NotFound => RefreshError::TokenRevoked,
            crate::db::token::TokenError::Expired => RefreshError::TokenExpired,
            crate::db::token::TokenError::Revoked => RefreshError::TokenRevoked,
            _ => RefreshError::DatabaseError(e.to_string()),
        })?;

    log::info!("Refresh token validated for user: {}", stored_token.user_id);

    // Parse user_id from claims
    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| RefreshError::InvalidToken)?;

    // Generate new access token
    let new_access_token = jwt_manager.generate_access_token(&user_id)
        .map_err(|e| RefreshError::TokenError(e.to_string()))?;

    log::info!("New access token generated for user: {}", user_id);

    Ok(RefreshResponse {
        access_token: new_access_token,
        token_type: "Bearer".to_string(),
        expires_in: jwt_manager.access_token_expiration(),
    })
}

/// Hash token for database storage (simple hash function)
fn hash_token(token: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::JwtManager;

    #[tokio::test]
    #[ignore]
    async fn test_refresh_token() {
        let pool = PgPool::connect("postgresql://harrison@localhost:5432/pingora_proxy")
            .await
            .unwrap();

        let redis_client = RedisClient::new("redis://localhost:6379")
            .await
            .unwrap();

        let jwt_manager = JwtManager::new(
            "test_secret".to_string(),
            900,
            604800,
        );

        let user_id = uuid::Uuid::new_v4();
        let (refresh_token_str, token_hash) = jwt_manager.generate_refresh_token(&user_id).unwrap();
        //   ^^^^^^^^^^^^^^^^^^ 重命名变量，避免与函数名冲突

        // Save to database
        let token_repo = TokenRepository::new(&pool);
        token_repo.save_refresh_token(&user_id, &token_hash, 604800)
            .await
            .unwrap();

        // Test refresh
        let request = RefreshRequest {
            refresh_token: refresh_token_str,
        };

        let response = refresh_token(&pool, &redis_client, &jwt_manager, request)
            .await
            .unwrap();

        assert!(!response.access_token.is_empty());
    }
}