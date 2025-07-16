// src/load_balancing/manager.rs
use crate::config::{Settings, settings::{LoadBalancingConfig, UpstreamConfig}};
use pingora_core::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use log::info;

pub struct LoadBalancingManager {
    config: LoadBalancingConfig,
    upstreams: Vec<UpstreamConfig>,
    round_robin_index: AtomicUsize,
}

impl LoadBalancingManager {
    pub async fn new(settings: Arc<Settings>) -> Result<Self> {
        let config = settings.load_balancing.clone();
        let upstreams = config.upstreams.clone();
        
        info!("Initialize load balancer: {}, upstream number: {}",
              config.strategy, upstreams.len());
        
        Ok(LoadBalancingManager {
            config,
            upstreams,
            round_robin_index: AtomicUsize::new(0),
        })
    }
    
    pub async fn select_upstream(&self, _path: &str) -> Result<UpstreamConfig> {
        if self.upstreams.is_empty() {
            return Err(pingora_core::Error::new_str("No upstream server available"));
        }
        
        match self.config.strategy.as_str() {
            "round_robin" => self.round_robin_select(),
            "random" => self.random_select(),
            _ => {
                log::warn!("Unknown strategy: {}, using polling", self.config.strategy);
                self.round_robin_select()
            }
        }
    }
    
    fn round_robin_select(&self) -> Result<UpstreamConfig> {
        let index = self.round_robin_index.fetch_add(1, Ordering::SeqCst);
        let upstream = &self.upstreams[index % self.upstreams.len()];
        Ok(upstream.clone())
    }
    
    fn random_select(&self) -> Result<UpstreamConfig> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;
        
        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        let hash = hasher.finish();
        
        let index = (hash as usize) % self.upstreams.len();
        Ok(self.upstreams[index].clone())
    }
}