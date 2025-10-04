use uuid::Uuid;

/// Request context that persists throughout the request lifecycle
#[derive(Debug, Clone)]
pub struct ProxyContext {
    /// Authenticated user ID (if authenticated)
    pub user_id: Option<Uuid>,

    /// Request ID for tracking
    pub request_id: String,

    /// Client IP address
    pub client_ip: Option<String>,

    /// Request start time (for metrics)
    pub start_time: std::time::Instant,
}

impl ProxyContext {
    /// Create a new context
    pub fn new() -> Self {
        Self {
            user_id: None,
            request_id: uuid::Uuid::new_v4().to_string(),
            client_ip: None,
            start_time: std::time::Instant::now(),
        }
    }

    /// Set authenticated user ID
    pub fn set_user_id(&mut self, user_id: Uuid) {
        self.user_id = Some(user_id);
    }

    /// Get elapsed time since request started
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

impl Default for ProxyContext {
    fn default() -> Self {
        Self::new()
    }
}
