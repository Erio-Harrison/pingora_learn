// src/proxy/context.rs
use std::time::Instant;

#[derive(Debug)]
pub struct ProxyContext {
    pub request_id: String,
    pub start_time: Instant,
    pub upstream_address: Option<String>,
}

impl ProxyContext {
    pub fn new() -> Self {
        ProxyContext {
            request_id: String::new(),
            start_time: Instant::now(),
            upstream_address: None,
        }
    }
}