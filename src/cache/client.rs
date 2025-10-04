use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use std::time::Duration;

/// Redis client wrapper with connection pooling
#[derive(Clone)]
pub struct RedisClient {
    manager: ConnectionManager,
}

impl RedisClient {
    /// Create a new Redis client with connection manager
    pub async fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        log::info!("Initializing Redis connection...");
        log::info!("Redis URL: {}", Self::mask_password(redis_url));

        let client = Client::open(redis_url)?;
        let manager = ConnectionManager::new(client).await?;

        log::info!("Redis connection initialized successfully");

        Ok(Self { manager })
    }

    /// Test Redis connection
    pub async fn test_connection(&self) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let _: String = redis::cmd("PING").query_async::<String>(&mut conn).await?;
        log::info!("Redis connection test successful");
        Ok(())
    }

    /// Set a key-value pair with expiration (in seconds)
    pub async fn set_ex(
        &self,
        key: &str,
        value: &str,
        expiration_seconds: u64,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.set_ex(key, value, expiration_seconds).await
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.get(key).await
    }

    /// Delete a key
    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.del(key).await
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool, redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.exists(key).await
    }

    /// Set a key with TTL (Time To Live) in seconds
    pub async fn expire(&self, key: &str, seconds: u64) -> Result<bool, redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.expire(key, seconds as i64).await
    }

    /// Increment a counter (used for rate limiting)
    pub async fn incr(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.manager.clone();
        conn.incr(key, 1).await
    }

    /// Increment a counter with expiration
    pub async fn incr_with_expiry(
        &self,
        key: &str,
        expiration_seconds: u64,
    ) -> Result<i64, redis::RedisError> {
        let mut conn = self.manager.clone();

        // Use Redis transaction to atomically increment and set expiration
        let count: i64 = conn.incr(key, 1).await?;

        // Only set expiration if this is the first increment
        if count == 1 {
            conn.expire::<_, ()>(key, expiration_seconds as i64).await?;
        }

        Ok(count)
    }

    /// Add token to blacklist (for JWT logout)
    pub async fn blacklist_token(
        &self,
        token: &str,
        expiration_seconds: u64,
    ) -> Result<(), redis::RedisError> {
        let key = format!("blacklist:{}", token);
        self.set_ex(&key, "1", expiration_seconds).await
    }

    /// Check if token is blacklisted
    pub async fn is_token_blacklisted(&self, token: &str) -> Result<bool, redis::RedisError> {
        let key = format!("blacklist:{}", token);
        self.exists(&key).await
    }

    /// Rate limiting: check if request is allowed
    /// Returns (allowed, current_count, ttl_seconds)
    pub async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: i64,
        window_seconds: u64,
    ) -> Result<(bool, i64, Option<Duration>), redis::RedisError> {
        let count = self.incr_with_expiry(key, window_seconds).await?;
        let allowed = count <= max_requests;

        // Get remaining TTL
        let mut conn = self.manager.clone();
        let ttl: i64 = conn.ttl(key).await?;
        let ttl_duration = if ttl > 0 {
            Some(Duration::from_secs(ttl as u64))
        } else {
            None
        };

        Ok((allowed, count, ttl_duration))
    }

    /// Mask password in Redis URL for logging
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
impl std::fmt::Debug for RedisClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisClient")
            .field("manager", &"ConnectionManager { ... }")
            .finish()
    }
}
