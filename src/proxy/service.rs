use async_trait::async_trait;
use bytes::Bytes;
use log::{error, info, warn};
use std::sync::Arc;

use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::Result;
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};

use crate::config::Settings;
use crate::load_balancing::LoadBalancingManager;
use crate::middleware::{AuthMiddleware, RateLimitMiddleware};
use crate::proxy::context::ProxyContext;

pub struct ProxyService {
    settings: Arc<Settings>,
    load_balancer: LoadBalancingManager,
    auth_middleware: AuthMiddleware,
    rate_limit_middleware: RateLimitMiddleware,
}

impl ProxyService {
    pub fn new(settings: Arc<Settings>) -> Result<Self> {
        info!("Initialize the proxy service...");
        info!("Auth config: enabled={}, type={}, tokens={:?}", 
            settings.middleware.auth.enabled,
            settings.middleware.auth.auth_type,
            settings.middleware.auth.valid_tokens
        );    
        
        let load_balancer = LoadBalancingManager::new(settings.clone())?;
        let auth_middleware = AuthMiddleware::new(&settings.middleware.auth);
        let rate_limit_middleware = RateLimitMiddleware::new(&settings.middleware.rate_limit);
        
        Ok(ProxyService {
            settings,
            load_balancer,
            auth_middleware,
            rate_limit_middleware,
        })
    }
}

#[async_trait]
impl ProxyHttp for ProxyService {
    type CTX = ProxyContext;
    
    fn new_ctx(&self) -> Self::CTX {
        ProxyContext::new()
    }
    
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let request_id = uuid::Uuid::new_v4().to_string();
        ctx.request_id = request_id.clone();
        ctx.start_time = std::time::Instant::now();
        
        let uri = session.req_header().uri.to_string();
        info!("Request Start: {} {}", request_id, uri);
        
        info!("Auth check: enabled={}", self.settings.middleware.auth.enabled);
        
        if self.settings.middleware.auth.enabled {
            info!("Running authentication check...");
            
            if let Some(auth_header) = session.req_header().headers.get("Authorization") {
                info!("Authorization header present: {:?}", auth_header);
            } else {
                info!("No Authorization header found");
            }
            
            match self.auth_middleware.check_auth(session.req_header()) {
                Ok(_) => {
                    info!("Authentication successful for request {}", request_id);
                }
                Err(e) => {
                    error!("Authentication failed for request {}: {}", request_id, e);
                    let _ = session.respond_error_with_body(
                        401,
                        Bytes::from_static(b"Unauthorized")
                    ).await;
                    return Ok(true);
                }
            }
        } else {
            info!("Authentication disabled, skipping check");
        }
        
        if self.settings.middleware.rate_limit.enabled {
            if let Err(e) = self.rate_limit_middleware.check_rate_limit(session) {
                warn!("Rate limit triggered: {}", e);
                let _ = session.respond_error_with_body(
                    429, 
                    Bytes::from_static(b"Too Many Requests")
                ).await;
                return Ok(true);
            }
        }
        
        if session.req_header().uri.path() == "/health" {
            let health_response = r#"{"status": "healthy"}"#;
            let _ = session.respond_error_with_body(
                200, 
                Bytes::from(health_response)
            ).await;
            return Ok(true);
        }
        
        Ok(false)
    }
    
    async fn upstream_peer(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<Box<HttpPeer>> {
        let path = session.req_header().uri.path();
        let upstream = self.load_balancer.select_upstream(path).await?;
        
        ctx.upstream_address = Some(format!("{}:{}", upstream.address, upstream.port));
        info!("Select Upstream: {} -> {}:{}", ctx.request_id, upstream.address, upstream.port);
        
        let peer = HttpPeer::new(
            (upstream.address.as_str(), upstream.port),
            false,
            "".to_string(),
        );
        
        Ok(Box::new(peer))
    }
    
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.insert_header("X-Proxy-By", "Pingora-Custom-Proxy")?;
        upstream_request.insert_header("X-Request-ID", &ctx.request_id)?;
        Ok(())
    }
    
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_response.insert_header("X-Proxy-By", "Pingora-Custom-Proxy")?;
        upstream_response.insert_header("X-Request-ID", &ctx.request_id)?;
        upstream_response.insert_header("X-Response-Time", 
            &format!("{}ms", ctx.start_time.elapsed().as_millis()))?;
        
        upstream_response.remove_header("Server");
        Ok(())
    }
    
    async fn logging(
        &self,
        session: &mut Session,
        error: Option<&pingora_core::Error>,
        ctx: &mut Self::CTX,
    ) {
        let status_code = session
            .response_written()
            .map_or(0, |resp| resp.status.as_u16());
        
        let duration = ctx.start_time.elapsed();
        let upstream = ctx.upstream_address.as_deref().unwrap_or("unknown");
        
        if let Some(e) = error {
            error!(
                "Request failed: {} {} -> {} ({}ms) - {}",
                ctx.request_id,
                session.req_header().uri,
                upstream,
                duration.as_millis(),
                e
            );
        } else {
            info!(
                "Request Completed: {} {} -> {} ({}) ({}ms)",
                ctx.request_id,
                session.req_header().uri,
                upstream,
                status_code,
                duration.as_millis()
            );
        }
    }
}