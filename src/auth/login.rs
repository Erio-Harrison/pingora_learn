use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

use crate::auth::{JwtManager, PasswordManager};
use crate::db::{TokenRepository, UserRepository};

/// Login request payload
#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Login error types
#[derive(Debug, Error)]
pub enum LoginError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User not found")]
    UserNotFound,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Token generation failed: {0}")]
    TokenError(String),
}

/// Authenticate user and generate tokens
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `jwt_manager` - JWT token manager
/// * `request` - Login request data
/// * `refresh_token_expiration` - Refresh token expiration in seconds
///
/// # Returns
/// * `Result<LoginResponse, LoginError>` - Login response or error
///
/// # Example
/// ```
/// let request = LoginRequest {
///     email: "user@example.com".to_string(),
///     password: "SecurePass123!".to_string(),
/// };
///
/// let response = login_user(
///     &pool,
///     &jwt_manager,
///     request,
///     604800
/// ).await?;
/// ```
pub async fn login_user(
    pool: &PgPool,
    jwt_manager: &JwtManager,
    request: LoginRequest,
    refresh_token_expiration: i64,
) -> Result<LoginResponse, LoginError> {
    let user_repo = UserRepository::new(pool);

    // Find user by email
    let user = user_repo
        .find_by_email(&request.email)
        .await
        .map_err(|e| match e {
            crate::db::user::UserError::NotFound => LoginError::UserNotFound,
            _ => LoginError::DatabaseError(e.to_string()),
        })?;

    // Verify password
    let is_valid = PasswordManager::verify(&request.password, &user.password_hash)
        .map_err(|e| LoginError::DatabaseError(e.to_string()))?;

    if !is_valid {
        log::warn!("Failed login attempt for user: {}", request.email);
        return Err(LoginError::InvalidCredentials);
    }

    log::info!("User logged in: {} (ID: {})", user.email, user.id);

    // Generate tokens
    let access_token = jwt_manager
        .generate_access_token(&user.id)
        .map_err(|e| LoginError::TokenError(e.to_string()))?;

    let (refresh_token, refresh_token_hash) = jwt_manager
        .generate_refresh_token(&user.id)
        .map_err(|e| LoginError::TokenError(e.to_string()))?;

    // Save refresh token to database
    let token_repo = TokenRepository::new(pool);
    token_repo
        .save_refresh_token(&user.id, &refresh_token_hash, refresh_token_expiration)
        .await
        .map_err(|e| LoginError::DatabaseError(e.to_string()))?;

    log::info!("Tokens generated for user: {}", user.email);

    Ok(LoginResponse {
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
    use crate::auth::{JwtManager, PasswordManager};
    use crate::db::user::CreateUser;
    use crate::db::UserRepository;

    #[tokio::test]
    #[ignore]
    async fn test_login_user() {
        let pool = PgPool::connect("postgresql://harrison@localhost:5432/pingora_proxy")
            .await
            .unwrap();

        let jwt_manager = JwtManager::new("test_secret".to_string(), 900, 604800);

        // Create test user
        let user_repo = UserRepository::new(&pool);
        let email = format!("test_{}@example.com", uuid::Uuid::new_v4());
        let password = "SecurePass123!";
        let password_hash = PasswordManager::hash(password).unwrap();

        let create_user = CreateUser {
            email: email.clone(),
            password_hash,
        };

        user_repo.create(create_user).await.unwrap();

        // Test login
        let request = LoginRequest {
            email,
            password: password.to_string(),
        };

        let response = login_user(&pool, &jwt_manager, request, 604800)
            .await
            .unwrap();

        assert!(!response.access_token.is_empty());
        assert!(!response.refresh_token.is_empty());
    }
}
