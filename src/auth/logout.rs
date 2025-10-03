use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;

use crate::auth::JwtManager;
use crate::cache::RedisClient;
use crate::db::TokenRepository;

/// Logout request payload
#[derive(Debug, Clone, Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: String,
}

/// Logout error types
#[derive(Debug, Error)]
pub enum LogoutError {
    #[error("Invalid token")]
    InvalidToken,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Logout user by revoking tokens
/// 
/// # Arguments
/// * `pool` - Database connection pool
/// * `redis_client` - Redis client for blacklisting
/// * `jwt_manager` - JWT token manager
/// * `access_token` - Access token to blacklist
/// * `request` - Logout request data
/// 
/// # Returns
/// * `Result<(), LogoutError>` - Success or error
/// 
/// # Example
/// ```
/// let request = LogoutRequest {
///     refresh_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...".to_string(),
/// };
/// 
/// logout_user(
///     &pool,
///     &redis_client,
///     &jwt_manager,
///     &access_token,
///     request
/// ).await?;
/// ```
pub async fn logout_user(
    pool: &PgPool,
    redis_client: &RedisClient,
    jwt_manager: &JwtManager,
    access_token: &str,
    request: LogoutRequest,
) -> Result<(), LogoutError> {
    // Decode access token to get user_id
    let access_claims = jwt_manager.validate_token(access_token)
        .map_err(|_| LogoutError::InvalidToken)?;

    let user_id = uuid::Uuid::parse_str(&access_claims.sub)
        .map_err(|_| LogoutError::InvalidToken)?;

    log::info!("Logout initiated for user: {}", user_id);

    // Add access token to blacklist (with remaining TTL)
    let remaining_ttl = access_claims.exp - chrono::Utc::now().timestamp();
    if remaining_ttl > 0 {
        redis_client.blacklist_token(access_token, remaining_ttl as u64)
            .await
            .map_err(|e| LogoutError::CacheError(e.to_string()))?;
        
        log::info!("Access token blacklisted for {} seconds", remaining_ttl);
    }

    // Revoke refresh token from database
    let token_hash = hash_token(&request.refresh_token);
    let token_repo = TokenRepository::new(pool);
    
    token_repo.revoke_token_by_hash(&token_hash)
        .await
        .map_err(|e| LogoutError::DatabaseError(e.to_string()))?;

    log::info!("Refresh token revoked for user: {}", user_id);

    Ok(())
}

/// Logout user from all devices
/// 
/// # Arguments
/// * `pool` - Database connection pool
/// * `redis_client` - Redis client for blacklisting
/// * `jwt_manager` - JWT token manager
/// * `access_token` - Current access token
/// 
/// # Returns
/// * `Result<u64, LogoutError>` - Number of tokens revoked or error
/// 
/// # Example
/// ```
/// let revoked_count = logout_all_devices(
///     &pool,
///     &redis_client,
///     &jwt_manager,
///     &access_token
/// ).await?;
/// 
/// println!("Revoked {} refresh tokens", revoked_count);
/// ```
pub async fn logout_all_devices(
    pool: &PgPool,
    redis_client: &RedisClient,
    jwt_manager: &JwtManager,
    access_token: &str,
) -> Result<u64, LogoutError> {
    // Decode access token to get user_id
    let access_claims = jwt_manager.validate_token(access_token)
        .map_err(|_| LogoutError::InvalidToken)?;

    let user_id = uuid::Uuid::parse_str(&access_claims.sub)
        .map_err(|_| LogoutError::InvalidToken)?;

    log::info!("Logout from all devices initiated for user: {}", user_id);

    // Add current access token to blacklist
    let remaining_ttl = access_claims.exp - chrono::Utc::now().timestamp();
    if remaining_ttl > 0 {
        redis_client.blacklist_token(access_token, remaining_ttl as u64)
            .await
            .map_err(|e| LogoutError::CacheError(e.to_string()))?;
    }

    // Revoke all refresh tokens for user
    let token_repo = TokenRepository::new(pool);
    let revoked_count = token_repo.revoke_all_user_tokens(&user_id)
        .await
        .map_err(|e| LogoutError::DatabaseError(e.to_string()))?;

    log::info!("Revoked {} refresh tokens for user: {}", revoked_count, user_id);

    Ok(revoked_count)
}

/// Hash token for database lookup
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
    use crate::db::TokenRepository;

    #[tokio::test]
    #[ignore]
    async fn test_logout_user() {
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
        
        // Generate tokens
        let access_token_str = jwt_manager.generate_access_token(&user_id).unwrap();
        let (refresh_token_str, token_hash) = jwt_manager.generate_refresh_token(&user_id).unwrap();

        // Save refresh token
        let token_repo = TokenRepository::new(&pool);
        token_repo.save_refresh_token(&user_id, &token_hash, 604800)
            .await
            .unwrap();

        // Test logout
        let request = LogoutRequest {
            refresh_token: refresh_token_str,
        };

        logout_user(&pool, &redis_client, &jwt_manager, &access_token_str, request)
            .await
            .unwrap();

        // Verify access token is blacklisted
        let is_blacklisted = redis_client.is_token_blacklisted(&access_token_str)
            .await
            .unwrap();
        assert!(is_blacklisted);
    }
}