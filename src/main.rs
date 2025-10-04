use anyhow::{Context, Result};
use pingora_core::server::Server;
use pingora_proxy::http_proxy_service;

mod auth;
mod cache;
mod config;
mod db;
mod load_balancing;
mod middleware;
mod proxy;

use tokio::runtime::Runtime;

fn main() -> Result<()> {
    env_logger::init();

    log::info!("========================================");
    log::info!("  Pingora Proxy with Authentication");
    log::info!("========================================\n");

    // Load configuration
    log::info!("Loading configuration...");
    let settings = config::Settings::load_from_file("config/proxy.yaml")
        .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;

    settings
        .validate()
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {}", e))?;

    log::info!("✓ Configuration loaded");
    log::info!("  Listen port: {}", settings.server.listen_port);
    log::info!("  Auth enabled: {}", settings.middleware.auth.enabled);
    log::info!(
        "  Rate limit enabled: {}",
        settings.middleware.rate_limit.enabled
    );

    // Create runtime for async initializations
    let rt = Runtime::new().context("Failed to create Tokio runtime")?;

    // Initialize database pool
    log::info!("\nInitializing database...");
    let db_pool = rt.block_on(async {
        db::DbPool::new(
            &settings.database.url,
            settings.database.max_connections,
            settings.database.min_connections,
        )
        .await
        .context("Failed to initialize database pool")
    })?;

    rt.block_on(async {
        db_pool
            .test_connection()
            .await
            .context("Database connection test failed")
    })?;

    log::info!("✓ Database connected");

    // Initialize Redis within async context
    log::info!("Initializing Redis...");
    let redis_client = rt.block_on(async {
        cache::RedisClient::new(&settings.redis.url)
            .await
            .context("Failed to initialize Redis client")
    })?;

    rt.block_on(async {
        redis_client
            .test_connection()
            .await
            .context("Redis connection test failed")
    })?;

    log::info!("✓ Redis connected");

    // Initialize JWT manager
    log::info!("Initializing JWT manager...");
    let jwt_manager = auth::JwtManager::new(
        settings.jwt.secret.clone(),
        settings.jwt.access_token_expiration,
        settings.jwt.refresh_token_expiration,
    );
    log::info!("✓ JWT manager initialized");

    // Initialize load balancer
    log::info!("Initializing load balancer...");
    let load_balancer =
        load_balancing::manager::LoadBalancerManager::new(settings.load_balancing.clone())?;
    log::info!(
        "✓ Load balancer initialized with {} upstream(s)",
        settings.load_balancing.upstreams.len()
    );

    // Create proxy service
    let proxy_service = proxy::service::ProxyService::new(
        settings.clone(),
        db_pool.inner().clone(),
        redis_client,
        jwt_manager,
        load_balancer,
    );

    // Create Pingora server
    let mut server = Server::new(None).context("Failed to create server")?;
    server.bootstrap();

    // Create HTTP proxy service
    let mut proxy = http_proxy_service(&server.configuration, proxy_service);
    proxy.add_tcp(&format!("0.0.0.0:{}", settings.server.listen_port));

    // Add service to server
    server.add_service(proxy);

    log::info!("\n========================================");
    log::info!(
        "✓ Server starting on 0.0.0.0:{}",
        settings.server.listen_port
    );
    log::info!("========================================\n");

    log::info!("Available endpoints:");
    log::info!("  POST /auth/register  - Register new user");
    log::info!("  POST /auth/login     - User login");
    log::info!("  POST /auth/refresh   - Refresh access token");
    log::info!("  POST /auth/logout    - User logout");
    log::info!("  *                    - Proxied to backend (requires auth)\n");

    // Run server
    server.run_forever();
}
