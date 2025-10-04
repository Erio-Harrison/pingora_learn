use crate::cache::RedisClient;
use pingora_http::ResponseHeader;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct RateLimitMiddleware {
    redis_client: RedisClient,
    requests_per_minute: u32,
    burst_size: u32,
}

impl RateLimitMiddleware {
    pub fn new(redis_client: RedisClient, requests_per_minute: u32, burst_size: u32) -> Self {
        Self {
            redis_client,
            requests_per_minute,
            burst_size,
        }
    }

    /// Check if request is allowed (Token Bucket Algorithm)
    /// Returns true if allowed, false if rate limit exceeded
    pub async fn check_rate_limit(&self, client_id: &str) -> bool {
        let key = format!("rate_limit:{}", client_id);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Try to get current token bucket state from Redis
        match self.get_token_bucket(&key).await {
            Ok(Some((tokens, last_refill))) => {
                // Calculate tokens to add since last refill
                let elapsed = now.saturating_sub(last_refill);
                let refill_rate = self.requests_per_minute as f64 / 60.0; // tokens per second
                let tokens_to_add = (elapsed as f64 * refill_rate) as u32;
                
                // Current tokens = previous remaining + newly added, capped at bucket capacity
                let current_tokens = (tokens + tokens_to_add).min(self.burst_size);

                if current_tokens > 0 {
                    // Token available, consume one
                    let new_tokens = current_tokens - 1;
                    if let Err(e) = self.set_token_bucket(&key, new_tokens, now).await {
                        log::error!("Failed to update token bucket for {}: {}", client_id, e);
                    }
                    log::debug!(
                        "Rate limit check passed for {}: {} tokens remaining", 
                        client_id, 
                        new_tokens
                    );
                    true
                } else {
                    // No tokens available, rate limited
                    log::warn!("Rate limit exceeded for {}: 0 tokens remaining", client_id);
                    false
                }
            }
            Ok(None) => {
                // First request, initialize token bucket
                // Bucket starts full, consume one token
                let initial_tokens = self.burst_size - 1;
                if let Err(e) = self.set_token_bucket(&key, initial_tokens, now).await {
                    log::error!("Failed to initialize token bucket for {}: {}", client_id, e);
                    // Fallback: allow request on Redis failure
                    return true;
                }
                log::debug!("Initialized token bucket for {} with {} tokens", client_id, initial_tokens);
                true
            }
            Err(e) => {
                // Redis error, fallback strategy: allow request
                log::error!("Redis error during rate limit check for {}: {}", client_id, e);
                true
            }
        }
    }

    /// Get token bucket state from Redis
    /// Returns (remaining_tokens, last_refill_timestamp)
    async fn get_token_bucket(&self, key: &str) -> anyhow::Result<Option<(u32, u64)>> {
        if let Some(value) = self.redis_client.get(key).await? {
            // Format: "tokens:timestamp"
            let parts: Vec<&str> = value.split(':').collect();
            if parts.len() == 2 {
                let tokens = parts[0].parse::<u32>()?;
                let timestamp = parts[1].parse::<u64>()?;
                return Ok(Some((tokens, timestamp)));
            } else {
                log::warn!("Invalid token bucket format in Redis for key {}: {}", key, value);
            }
        }
        Ok(None)
    }

    /// Set token bucket state to Redis
    async fn set_token_bucket(&self, key: &str, tokens: u32, timestamp: u64) -> anyhow::Result<()> {
        let value = format!("{}:{}", tokens, timestamp);
        let ttl = 120; // 2 minutes TTL to prevent Redis data accumulation
        self.redis_client.set_ex(key, &value, ttl).await
            .map_err(|e| anyhow::anyhow!("Redis set_ex failed: {}", e))
    }

    /// Create 429 Too Many Requests response
    pub fn too_many_requests_response() -> ResponseHeader {
        let mut resp = ResponseHeader::build(429, None).unwrap();
        resp.insert_header("Content-Type", "application/json")
            .unwrap();
        resp.insert_header("Retry-After", "60").unwrap();
        resp
    }

    /// Get configured request limit
    pub fn get_limit(&self) -> u32 {
        self.requests_per_minute
    }

    /// Get configured burst size
    pub fn get_burst_size(&self) -> u32 {
        self.burst_size
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_token_bucket_format() {
        let value = "10:1234567890";
        let parts: Vec<&str> = value.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].parse::<u32>().unwrap(), 10);
        assert_eq!(parts[1].parse::<u64>().unwrap(), 1234567890);
    }

    #[test]
    fn test_refill_calculation() {
        let requests_per_minute = 60u32;
        let refill_rate = requests_per_minute as f64 / 60.0;
        assert_eq!(refill_rate, 1.0); // 1 token per second

        let elapsed = 10u64; // 10 seconds
        let tokens_to_add = (elapsed as f64 * refill_rate) as u32;
        assert_eq!(tokens_to_add, 10); // Should refill 10 tokens in 10 seconds
    }
}