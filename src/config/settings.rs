use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub jwt: JwtConfig,
    pub load_balancing: LoadBalancingConfig,
    pub middleware: MiddlewareConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub listen_port: u16,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expiration: i64,
    pub refresh_token_expiration: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadBalancingConfig {
    pub strategy: String,
    pub upstreams: Vec<UpstreamConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpstreamConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub weight: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MiddlewareConfig {
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

impl Settings {
    /// Load settings from YAML file and expand environment variables
    /// Returns Box<dyn Error> (not Send + Sync)
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Load .env file if exists
        dotenv::dotenv().ok();

        let content = fs::read_to_string(path)?;
        
        // Replace environment variables in the format ${VAR_NAME}
        let expanded_content = Self::expand_env_vars(&content);
        
        let settings: Settings = serde_yaml::from_str(&expanded_content)?;
        Ok(settings)
    }

    /// Expand environment variables in the format ${VAR_NAME}
    fn expand_env_vars(content: &str) -> String {
        let mut result = content.to_string();
        
        // Find all ${...} patterns
        while let Some(start) = result.find("${") {
            if let Some(end) = result[start..].find('}') {
                let var_name = &result[start + 2..start + end];
                let var_value = std::env::var(var_name).unwrap_or_else(|_| {
                    log::warn!("Environment variable {} not found, using empty string", var_name);
                    String::new()
                });
                
                result.replace_range(start..start + end + 1, &var_value);
            } else {
                break;
            }
        }
        
        result
    }

    /// Validate configuration
    /// Returns Result with String error (not implementing std::error::Error)
    pub fn validate(&self) -> Result<(), String> {
        // Validate server config
        if self.server.listen_port == 0 {
            return Err("Server listen_port cannot be 0".to_string());
        }

        // Validate database config
        if self.database.url.is_empty() {
            return Err("Database URL cannot be empty".to_string());
        }
        if self.database.max_connections < self.database.min_connections {
            return Err("Database max_connections must be >= min_connections".to_string());
        }

        // Validate Redis config
        if self.redis.url.is_empty() {
            return Err("Redis URL cannot be empty".to_string());
        }

        // Validate JWT config
        if self.jwt.secret.is_empty() {
            return Err("JWT secret cannot be empty".to_string());
        }
        if self.jwt.access_token_expiration <= 0 {
            return Err("JWT access_token_expiration must be positive".to_string());
        }
        if self.jwt.refresh_token_expiration <= 0 {
            return Err("JWT refresh_token_expiration must be positive".to_string());
        }

        // Validate upstreams
        if self.load_balancing.upstreams.is_empty() {
            return Err("At least one upstream must be configured".to_string());
        }

        for upstream in &self.load_balancing.upstreams {
            if upstream.name.is_empty() {
                return Err("Upstream name cannot be empty".to_string());
            }
            if upstream.address.is_empty() {
                return Err(format!("Upstream {} address cannot be empty", upstream.name));
            }
            if upstream.port == 0 {
                return Err(format!("Upstream {} port cannot be 0", upstream.name));
            }
        }

        Ok(())
    }
}