// src/main.rs
use log::info;
use std::sync::Arc;

mod config;
mod proxy;
mod middleware;
mod load_balancing;

use config::Settings;
use proxy::ProxyService;
use pingora_core::server::{configuration::Opt, Server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Start Pingora proxy service...");

    let opt = Opt::default();
    let settings = Settings::load_from_file("config/proxy.yaml")?;
    info!("Configuration loading completed");

    let mut server = Server::new(Some(opt))?;
    server.bootstrap();

    let proxy_service = ProxyService::new(Arc::new(settings))?;
    
    let mut http_proxy = pingora_proxy::http_proxy_service(
        &server.configuration,
        proxy_service,
    );
    
    http_proxy.add_tcp("0.0.0.0:8080");
    server.add_service(http_proxy);
    
    info!("Proxy service started successfully, listening on port 8080");
    server.run_forever();
}
