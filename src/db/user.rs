use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

/// User database model
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
}

/// User creation data
#[derive(Debug, Clone)]
pub struct CreateUser {
    pub email: String,
    pub password_hash: String,
}

/// Custom error type for user operations
#[derive(Debug, Error)]
pub enum UserError {
    #[error("User not found")]
    NotFound,

    #[error("Email already exists")]
    EmailExists,

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

/// User repository for database operations
pub struct UserRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> UserRepository<'a> {
    /// Create a new user repository
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new user
    ///
    /// # Arguments
    /// * `user_data` - User creation data
    ///
    /// # Returns
    /// * `Result<User, UserError>` - Created user or error
    ///
    /// # Example
    /// ```
    /// let user_data = CreateUser {
    ///     email: "user@example.com".to_string(),
    ///     password_hash: hashed_password,
    /// };
    /// let user = user_repo.create(user_data).await?;
    /// ```
    pub async fn create(&self, user_data: CreateUser) -> Result<User, UserError> {
        // Check if email already exists
        if self.email_exists(&user_data.email).await? {
            return Err(UserError::EmailExists);
        }

        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (email, password_hash)
            VALUES ($1, $2)
            RETURNING id, email, password_hash
            "#,
        )
        .bind(&user_data.email)
        .bind(&user_data.password_hash)
        .fetch_one(self.pool)
        .await?;

        log::info!("User created: {} (ID: {})", user.email, user.id);

        Ok(user)
    }

    /// Find user by ID
    ///
    /// # Arguments
    /// * `user_id` - User's UUID
    ///
    /// # Returns
    /// * `Result<User, UserError>` - User or error
    pub async fn find_by_id(&self, user_id: &Uuid) -> Result<User, UserError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(UserError::NotFound)?;

        Ok(user)
    }

    /// Find user by email
    ///
    /// # Arguments
    /// * `email` - User's email address
    ///
    /// # Returns
    /// * `Result<User, UserError>` - User or error
    ///
    /// # Example
    /// ```
    /// let user = user_repo.find_by_email("user@example.com").await?;
    /// ```
    pub async fn find_by_email(&self, email: &str) -> Result<User, UserError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(self.pool)
        .await?
        .ok_or(UserError::NotFound)?;

        Ok(user)
    }

    /// Check if email already exists
    ///
    /// # Arguments
    /// * `email` - Email to check
    ///
    /// # Returns
    /// * `Result<bool, UserError>` - true if exists, false otherwise
    pub async fn email_exists(&self, email: &str) -> Result<bool, UserError> {
        let result = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)
            "#,
        )
        .bind(email)
        .fetch_one(self.pool)
        .await?;

        Ok(result)
    }

    /// Update user's password
    ///
    /// # Arguments
    /// * `user_id` - User's UUID
    /// * `new_password_hash` - New hashed password
    ///
    /// # Returns
    /// * `Result<User, UserError>` - Updated user or error
    pub async fn update_password(
        &self,
        user_id: &Uuid,
        new_password_hash: &str,
    ) -> Result<User, UserError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET password_hash = $1
            WHERE id = $2
            RETURNING id, email, password_hash
            "#,
        )
        .bind(new_password_hash)
        .bind(user_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(UserError::NotFound)?;

        log::info!("Password updated for user: {}", user.email);

        Ok(user)
    }

    /// Delete user by ID
    ///
    /// # Arguments
    /// * `user_id` - User's UUID
    ///
    /// # Returns
    /// * `Result<(), UserError>` - Success or error
    pub async fn delete(&self, user_id: &Uuid) -> Result<(), UserError> {
        let result = sqlx::query(
            r#"
            DELETE FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(UserError::NotFound);
        }

        log::info!("User deleted: {}", user_id);

        Ok(())
    }

    /// Get all users (for admin purposes, use with caution)
    ///
    /// # Arguments
    /// * `limit` - Maximum number of users to return
    /// * `offset` - Number of users to skip
    ///
    /// # Returns
    /// * `Result<Vec<User>, UserError>` - List of users or error
    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<User>, UserError> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash
            FROM users
            ORDER BY id
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;

        Ok(users)
    }

    /// Count total users
    ///
    /// # Returns
    /// * `Result<i64, UserError>` - Total user count or error
    pub async fn count(&self) -> Result<i64, UserError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM users
            "#,
        )
        .fetch_one(self.pool)
        .await?;

        Ok(count)
    }

    /// Verify user's password
    ///
    /// # Arguments
    /// * `email` - User's email
    /// * `password` - Plain text password to verify
    ///
    /// # Returns
    /// * `Result<Option<User>, UserError>` - Some(User) if valid, None if invalid password
    ///
    /// # Example
    /// ```
    /// match user_repo.verify_credentials("user@example.com", "password").await? {
    ///     Some(user) => println!("Login successful: {}", user.email),
    ///     None => println!("Invalid credentials"),
    /// }
    /// ```
    pub async fn verify_credentials(
        &self,
        email: &str,
        password: &str,
    ) -> Result<Option<User>, UserError> {
        use crate::auth::PasswordManager;

        // Find user by email
        let user = match self.find_by_email(email).await {
            Ok(user) => user,
            Err(UserError::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };

        // Verify password
        let is_valid = PasswordManager::verify(password, &user.password_hash)
            .map_err(|_| UserError::DatabaseError(sqlx::Error::RowNotFound))?;

        if is_valid {
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::PasswordManager;

    // Note: These are integration tests that require a running database
    // Run with: cargo test --features integration-tests

    #[tokio::test]
    #[ignore] // Remove this to run integration tests
    async fn test_user_crud() {
        // Setup test database connection
        let pool =
            PgPool::connect("postgresql://proxy_user:proxy_pass@localhost:5432/pingora_proxy")
                .await
                .unwrap();

        let repo = UserRepository::new(&pool);

        // Create user
        let password_hash = PasswordManager::hash("TestPassword123").unwrap();
        let user_data = CreateUser {
            email: "test@example.com".to_string(),
            password_hash,
        };

        let user = repo.create(user_data).await.unwrap();
        assert_eq!(user.email, "test@example.com");

        // Find by ID
        let found_user = repo.find_by_id(&user.id).await.unwrap();
        assert_eq!(found_user.email, user.email);

        // Find by email
        let found_user = repo.find_by_email("test@example.com").await.unwrap();
        assert_eq!(found_user.id, user.id);

        // Delete user
        repo.delete(&user.id).await.unwrap();

        // Verify deletion
        assert!(repo.find_by_id(&user.id).await.is_err());
    }
}
