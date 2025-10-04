// src/middleware/rate_limit.rs
use crate::config::settings::RateLimitConfig;
use pingora_proxy::Session;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct RateLimitMiddleware {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
}

#[derive(Debug)]
pub struct RateLimitError(String);

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Current limiting error: {}", self.0)
    }
}

impl Error for RateLimitError {}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
    max_tokens: u32,
    refill_rate: u32,
}

impl TokenBucket {
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        TokenBucket {
            tokens: max_tokens,
            last_refill: Instant::now(),
            max_tokens,
            refill_rate,
        }
    }

    fn try_consume(&mut self, tokens: u32) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        if elapsed >= Duration::from_secs(60) {
            let minutes = elapsed.as_secs() / 60;
            let new_tokens = (minutes as u32) * self.refill_rate;
            self.tokens = std::cmp::min(self.max_tokens, self.tokens + new_tokens);
            self.last_refill = now;
        }
    }
}

impl RateLimitMiddleware {
    pub fn new(config: &RateLimitConfig) -> Self {
        RateLimitMiddleware {
            config: config.clone(),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn check_rate_limit(&self, session: &Session) -> Result<(), RateLimitError> {
        if !self.config.enabled {
            return Ok(());
        }

        let key = self.extract_key(session)?;

        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(key).or_insert_with(|| {
            TokenBucket::new(self.config.burst_size, self.config.requests_per_minute)
        });

        if bucket.try_consume(1) {
            Ok(())
        } else {
            Err(RateLimitError("Request frequency is too high".to_string()))
        }
    }

    fn extract_key(&self, session: &Session) -> Result<String, RateLimitError> {
        let ip = session
            .req_header()
            .headers
            .get("X-Forwarded-For")
            .or_else(|| session.req_header().headers.get("X-Real-IP"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");
        Ok(ip.to_string())
    }
}
