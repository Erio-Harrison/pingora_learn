use pingora_core::upstreams::peer::HttpPeer;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

use crate::config::settings::LoadBalancingConfig;

#[derive(Debug, Error)]
pub enum LoadBalancerError {
    #[error("No upstreams configured")]
    NoUpstreams,

    #[error("Invalid strategy: {0}")]
    InvalidStrategy(String),
}

/// Load balancer manager
pub struct LoadBalancerManager {
    config: LoadBalancingConfig,
    round_robin_counter: AtomicUsize,
}

impl LoadBalancerManager {
    /// Create a new load balancer manager
    pub fn new(config: LoadBalancingConfig) -> Result<Self, LoadBalancerError> {
        if config.upstreams.is_empty() {
            return Err(LoadBalancerError::NoUpstreams);
        }

        Ok(Self {
            config,
            round_robin_counter: AtomicUsize::new(0),
        })
    }

    /// Select next upstream peer
    pub fn select_peer(&self) -> Result<Box<HttpPeer>, LoadBalancerError> {
        match self.config.strategy.as_str() {
            "round_robin" => self.round_robin(),
            "random" => self.random(),
            _ => Err(LoadBalancerError::InvalidStrategy(
                self.config.strategy.clone(),
            )),
        }
    }

    /// Round-robin load balancing
    fn round_robin(&self) -> Result<Box<HttpPeer>, LoadBalancerError> {
        let index = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
        let upstream = &self.config.upstreams[index % self.config.upstreams.len()];

        let peer = Box::new(HttpPeer::new(
            (upstream.address.as_str(), upstream.port),
            false, // TLS
            upstream.name.clone(),
        ));

        Ok(peer)
    }

    /// Random load balancing
    fn random(&self) -> Result<Box<HttpPeer>, LoadBalancerError> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.config.upstreams.len());
        let upstream = &self.config.upstreams[index];

        let peer = Box::new(HttpPeer::new(
            (upstream.address.as_str(), upstream.port),
            false,
            upstream.name.clone(),
        ));

        Ok(peer)
    }
}
