use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

use crate::auth::{JwtManager, PasswordManager};
use crate::db::{UserRepository, TokenRepository};
use crate::db::user::CreateUser;

/// Register request payload
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

/// Register response
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Registration error types
#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("Email already exists")]
    EmailExists,
    
    #[error("Invalid email format")]
    InvalidEmail,
    
    #[error("Password validation failed: {0}")]
    PasswordValidationFailed(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Token generation failed: {0}")]
    TokenError(String),
}

/// Register a new user
/// 
/// # Arguments
/// * `pool` - Database connection pool
/// * `jwt_manager` - JWT token manager
/// * `request` - Registration request data
/// * `refresh_token_expiration` - Refresh token expiration in seconds
/// 
/// # Returns
/// * `Result<RegisterResponse, RegisterError>` - Registration response or error
/// 
/// # Example
/// ```
/// let request = RegisterRequest {
///     email: "user@example.com".to_string(),
///     password: "SecurePass123!".to_string(),
/// };
/// 
/// let response = register_user(
///     &pool,
///     &jwt_manager,
///     request,
///     604800
/// ).await?;
/// ```
pub async fn register_user(
    pool: &PgPool,
    jwt_manager: &JwtManager,
    request: RegisterRequest,
    refresh_token_expiration: i64,
) -> Result<RegisterResponse, RegisterError> {
    // Validate email format (basic check)
    if !request.email.contains('@') || !request.email.contains('.') {
        return Err(RegisterError::InvalidEmail);
    }

    // Check if email already exists
    let user_repo = UserRepository::new(pool);
    if user_repo.email_exists(&request.email)
        .await
        .map_err(|e| RegisterError::DatabaseError(e.to_string()))? 
    {
        return Err(RegisterError::EmailExists);
    }

    // Hash password
    let password_hash = PasswordManager::hash(&request.password)
        .map_err(|e| RegisterError::PasswordValidationFailed(e.to_string()))?;

    // Create user
    let create_user = CreateUser {
        email: request.email.clone(),
        password_hash,
    };

    let user = user_repo.create(create_user)
        .await
        .map_err(|e| RegisterError::DatabaseError(e.to_string()))?;

    log::info!("New user registered: {} (ID: {})", user.email, user.id);

    // Generate tokens
    let access_token = jwt_manager.generate_access_token(&user.id)
        .map_err(|e| RegisterError::TokenError(e.to_string()))?;

    let (refresh_token, refresh_token_hash) = jwt_manager.generate_refresh_token(&user.id)
        .map_err(|e| RegisterError::TokenError(e.to_string()))?;

    // Save refresh token to database
    let token_repo = TokenRepository::new(pool);
    token_repo.save_refresh_token(
        &user.id,
        &refresh_token_hash,
        refresh_token_expiration,
    )
    .await
    .map_err(|e| RegisterError::DatabaseError(e.to_string()))?;

    log::info!("Tokens generated for user: {}", user.email);

    Ok(RegisterResponse {
        user_id: user.id.to_string(),
        email: user.email,
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: jwt_manager.access_token_expiration(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::JwtManager;

    #[tokio::test]
    #[ignore]
    async fn test_register_user() {
        let pool = PgPool::connect("postgresql://harrison@localhost:5432/pingora_proxy")
            .await
            .unwrap();

        let jwt_manager = JwtManager::new(
            "test_secret".to_string(),
            900,
            604800,
        );

        let request = RegisterRequest {
            email: format!("test_{}@example.com", uuid::Uuid::new_v4()),
            password: "SecurePass123!".to_string(),
        };

        let response = register_user(&pool, &jwt_manager, request, 604800)
            .await
            .unwrap();

        assert!(!response.access_token.is_empty());
        assert!(!response.refresh_token.is_empty());
    }
}