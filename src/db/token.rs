use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use thiserror::Error;

/// Refresh token database model
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Custom error type for token operations
#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Token not found")]
    NotFound,
    
    #[error("Token has expired")]
    Expired,
    
    #[error("Token has been revoked")]
    Revoked,
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

/// Token repository for database operations
pub struct TokenRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> TokenRepository<'a> {
    /// Create a new token repository
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Save a refresh token to database
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// * `token_hash` - Hashed refresh token
    /// * `expires_in_seconds` - Token expiration time in seconds
    /// 
    /// # Returns
    /// * `Result<RefreshToken, TokenError>` - Saved token or error
    /// 
    /// # Example
    /// ```
    /// let token = token_repo.save_refresh_token(
    ///     &user_id,
    ///     &token_hash,
    ///     604800  // 7 days
    /// ).await?;
    /// ```
    pub async fn save_refresh_token(
        &self,
        user_id: &Uuid,
        token_hash: &str,
        expires_in_seconds: i64,
    ) -> Result<RefreshToken, TokenError> {
        let expires_at = Utc::now() + Duration::seconds(expires_in_seconds);

        let token = sqlx::query_as::<_, RefreshToken>(
            r#"
            INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
            VALUES ($1, $2, $3)
            RETURNING id, user_id, token_hash, expires_at
            "#
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .fetch_one(self.pool)
        .await?;

        log::info!("Refresh token saved for user: {} (expires: {})", user_id, expires_at);

        Ok(token)
    }

    /// Find refresh token by hash
    /// 
    /// # Arguments
    /// * `token_hash` - Hashed refresh token
    /// 
    /// # Returns
    /// * `Result<RefreshToken, TokenError>` - Token or error
    pub async fn find_by_hash(&self, token_hash: &str) -> Result<RefreshToken, TokenError> {
        let token = sqlx::query_as::<_, RefreshToken>(
            r#"
            SELECT id, user_id, token_hash, expires_at
            FROM refresh_tokens
            WHERE token_hash = $1
            "#
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TokenError::NotFound)?;

        Ok(token)
    }

    /// Verify refresh token is valid (exists and not expired)
    /// 
    /// # Arguments
    /// * `token_hash` - Hashed refresh token
    /// 
    /// # Returns
    /// * `Result<RefreshToken, TokenError>` - Valid token or error
    /// 
    /// # Example
    /// ```
    /// match token_repo.verify_refresh_token(&token_hash).await {
    ///     Ok(token) => println!("Token is valid for user: {}", token.user_id),
    ///     Err(TokenError::Expired) => println!("Token expired"),
    ///     Err(TokenError::NotFound) => println!("Invalid token"),
    ///     Err(e) => println!("Error: {}", e),
    /// }
    /// ```
    pub async fn verify_refresh_token(&self, token_hash: &str) -> Result<RefreshToken, TokenError> {
        let token = self.find_by_hash(token_hash).await?;

        // Check if expired
        if token.expires_at < Utc::now() {
            // Optionally delete expired token
            self.revoke_token(&token.id).await.ok();
            return Err(TokenError::Expired);
        }

        Ok(token)
    }

    /// Revoke a specific refresh token
    /// 
    /// # Arguments
    /// * `token_id` - Token's UUID
    /// 
    /// # Returns
    /// * `Result<(), TokenError>` - Success or error
    pub async fn revoke_token(&self, token_id: &Uuid) -> Result<(), TokenError> {
        let result = sqlx::query(
            r#"
            DELETE FROM refresh_tokens
            WHERE id = $1
            "#
        )
        .bind(token_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(TokenError::NotFound);
        }

        log::info!("Refresh token revoked: {}", token_id);

        Ok(())
    }

    /// Revoke refresh token by hash
    /// 
    /// # Arguments
    /// * `token_hash` - Hashed refresh token
    /// 
    /// # Returns
    /// * `Result<(), TokenError>` - Success or error
    pub async fn revoke_token_by_hash(&self, token_hash: &str) -> Result<(), TokenError> {
        let result = sqlx::query(
            r#"
            DELETE FROM refresh_tokens
            WHERE token_hash = $1
            "#
        )
        .bind(token_hash)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(TokenError::NotFound);
        }

        log::info!("Refresh token revoked by hash");

        Ok(())
    }

    /// Revoke all refresh tokens for a user (useful for logout from all devices)
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// 
    /// # Returns
    /// * `Result<u64, TokenError>` - Number of tokens revoked or error
    pub async fn revoke_all_user_tokens(&self, user_id: &Uuid) -> Result<u64, TokenError> {
        let result = sqlx::query(
            r#"
            DELETE FROM refresh_tokens
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .execute(self.pool)
        .await?;

        let count = result.rows_affected();
        log::info!("Revoked {} refresh tokens for user: {}", count, user_id);

        Ok(count)
    }

    /// Get all active refresh tokens for a user
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// 
    /// # Returns
    /// * `Result<Vec<RefreshToken>, TokenError>` - List of active tokens or error
    pub async fn get_user_tokens(&self, user_id: &Uuid) -> Result<Vec<RefreshToken>, TokenError> {
        let tokens = sqlx::query_as::<_, RefreshToken>(
            r#"
            SELECT id, user_id, token_hash, expires_at
            FROM refresh_tokens
            WHERE user_id = $1
            AND expires_at > NOW()
            ORDER BY expires_at DESC
            "#
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;

        Ok(tokens)
    }

    /// Clean up expired tokens (should be run periodically)
    /// 
    /// # Returns
    /// * `Result<u64, TokenError>` - Number of tokens deleted or error
    pub async fn cleanup_expired_tokens(&self) -> Result<u64, TokenError> {
        let result = sqlx::query(
            r#"
            DELETE FROM refresh_tokens
            WHERE expires_at < NOW()
            "#
        )
        .execute(self.pool)
        .await?;

        let count = result.rows_affected();
        if count > 0 {
            log::info!("Cleaned up {} expired refresh tokens", count);
        }

        Ok(count)
    }

    /// Count active tokens for a user
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// 
    /// # Returns
    /// * `Result<i64, TokenError>` - Count of active tokens or error
    pub async fn count_user_active_tokens(&self, user_id: &Uuid) -> Result<i64, TokenError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM refresh_tokens
            WHERE user_id = $1
            AND expires_at > NOW()
            "#
        )
        .bind(user_id)
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Get token expiration time
    /// 
    /// # Arguments
    /// * `token_hash` - Hashed refresh token
    /// 
    /// # Returns
    /// * `Result<DateTime<Utc>, TokenError>` - Expiration time or error
    pub async fn get_token_expiration(
        &self,
        token_hash: &str,
    ) -> Result<DateTime<Utc>, TokenError> {
        let token = self.find_by_hash(token_hash).await?;
        Ok(token.expires_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Remove this to run integration tests
    async fn test_token_crud() {
        // Setup test database connection
        let pool = PgPool::connect("postgresql://harrison@localhost:5432/pingora_proxy")
            .await
            .unwrap();

        let repo = TokenRepository::new(&pool);
        let user_id = Uuid::new_v4();
        let token_hash = "test_token_hash_123";

        // Save token
        let token = repo.save_refresh_token(&user_id, token_hash, 604800)
            .await
            .unwrap();
        assert_eq!(token.user_id, user_id);

        // Find by hash
        let found_token = repo.find_by_hash(token_hash).await.unwrap();
        assert_eq!(found_token.id, token.id);

        // Verify token
        let verified_token = repo.verify_refresh_token(token_hash).await.unwrap();
        assert_eq!(verified_token.id, token.id);

        // Revoke token
        repo.revoke_token(&token.id).await.unwrap();

        // Verify revocation
        assert!(repo.find_by_hash(token_hash).await.is_err());
    }
}