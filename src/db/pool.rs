use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// PostgreSQL connection pool wrapper
#[derive(Clone)]
pub struct DbPool {
    pool: PgPool,
}

impl DbPool {
    /// Create a new database connection pool
    pub async fn new(
        database_url: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<Self, sqlx::Error> {
        log::info!("Initializing database connection pool...");
        log::info!("Database URL: {}", Self::mask_password(database_url));
        log::info!(
            "Max connections: {}, Min connections: {}",
            max_connections,
            min_connections
        );

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await?;

        log::info!("Database connection pool initialized successfully");

        Ok(Self { pool })
    }

    /// Get the inner pool
    pub fn inner(&self) -> &PgPool {
        &self.pool
    }

    /// Test database connection
    pub async fn test_connection(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
    }

    /// Close the connection pool gracefully
    pub async fn close(&self) {
        log::info!("Closing database connection pool...");
        self.pool.close().await;
        log::info!("Database connection pool closed");
    }

    /// Mask password in database URL for logging
    fn mask_password(url: &str) -> String {
        if let Some(at_pos) = url.rfind('@') {
            if let Some(colon_pos) = url[..at_pos].rfind(':') {
                let mut masked = url.to_string();
                masked.replace_range(colon_pos + 1..at_pos, "****");
                return masked;
            }
        }
        url.to_string()
    }
}

// Implement Debug manually to avoid leaking credentials
impl std::fmt::Debug for DbPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbPool")
            .field("pool", &"PgPool { ... }")
            .finish()
    }
}
