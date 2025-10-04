use async_trait::async_trait;
use bytes::Bytes;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::Error;
use pingora_core::ErrorType;
use pingora_core::Result;
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};
use sqlx::PgPool;
use std::sync::Arc;

use crate::auth::{login_user, logout_user, refresh_token, register_user, JwtManager};
use crate::cache::RedisClient;
use crate::config::Settings;
use crate::load_balancing::manager::LoadBalancerManager;
use crate::proxy::context::ProxyContext;
use pingora_core::upstreams::peer::Peer;

/// Proxy service with authentication
pub struct ProxyService {
    pub settings: Arc<Settings>,
    pub db_pool: Arc<PgPool>,
    pub redis_client: Arc<RedisClient>,
    pub jwt_manager: Arc<JwtManager>,
    pub load_balancer: Arc<LoadBalancerManager>,
}

impl ProxyService {
    /// Create a new proxy service
    pub fn new(
        settings: Settings,
        db_pool: PgPool,
        redis_client: RedisClient,
        jwt_manager: JwtManager,
        load_balancer: LoadBalancerManager,
    ) -> Self {
        Self {
            settings: Arc::new(settings),
            db_pool: Arc::new(db_pool),
            redis_client: Arc::new(redis_client),
            jwt_manager: Arc::new(jwt_manager),
            load_balancer: Arc::new(load_balancer),
        }
    }
}

#[async_trait]
impl ProxyHttp for ProxyService {
    type CTX = ProxyContext;

    fn new_ctx(&self) -> Self::CTX {
        ProxyContext::new()
    }

    /// Handle incoming requests - routing and authentication
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let req = session.req_header_mut();
        let path = req.uri.path().to_string(); // Clone to avoid borrowing issues
        let method = req.method.as_str().to_string(); // Clone to avoid borrowing issues

        log::info!(
            "[{}] {} {} from {:?}",
            ctx.request_id,
            method,
            path,
            session.client_addr()
        );

        // Store client IP
        if let Some(addr) = session.client_addr() {
            ctx.client_ip = Some(addr.to_string());
        }

        // ============================================================
        // Route Authentication Endpoints
        // ============================================================

        if path.starts_with("/auth/") {
            return self
                .handle_auth_endpoint(session, ctx, &path, &method)
                .await;
        }

        // ============================================================
        // Protected Routes - Require JWT Authentication
        // ============================================================

        if self.settings.middleware.auth.enabled {
            match self.authenticate_request(session.req_header(), ctx).await {
                Ok(()) => {
                    log::info!("[{}] Authenticated user: {:?}", ctx.request_id, ctx.user_id);
                }
                Err(e) => {
                    log::warn!("[{}] Authentication failed: {}", ctx.request_id, e);
                    self.send_unauthorized_response(session).await?;
                    return Ok(true); // Stop processing
                }
            }
        }

        // ============================================================
        // Rate Limiting
        // ============================================================

        if self.settings.middleware.rate_limit.enabled {
            if let Err(e) = self.check_rate_limit(ctx).await {
                log::warn!("[{}] Rate limit exceeded: {}", ctx.request_id, e);
                self.send_rate_limit_response(session).await?;
                return Ok(true); // Stop processing
            }
        }

        // Continue to upstream
        Ok(false)
    }

    /// Select upstream server for load balancing
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let peer = self
            .load_balancer
            .select_peer()
            .map_err(|e| Error::because(ErrorType::InternalError, "Load balancer error", e))?;

        log::info!("[{}] Selected upstream: {}", ctx.request_id, peer.address());

        Ok(peer)
    }

    /// Log response
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        log::info!(
            "[{}] Response: {} (took {:?})",
            ctx.request_id,
            upstream_response.status,
            ctx.elapsed()
        );

        Ok(())
    }
}

impl ProxyService {
    /// Handle authentication endpoints
    async fn handle_auth_endpoint(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
        path: &str,
        method: &str,
    ) -> Result<bool> {
        match (method, path) {
            ("POST", "/auth/register") => {
                self.handle_register(session, ctx).await?;
            }
            ("POST", "/auth/login") => {
                self.handle_login(session, ctx).await?;
            }
            ("POST", "/auth/refresh") => {
                self.handle_refresh(session, ctx).await?;
            }
            ("POST", "/auth/logout") => {
                self.handle_logout(session, ctx).await?;
            }
            _ => {
                self.send_not_found_response(session).await?;
            }
        }

        Ok(true) // Stop processing, we handled it
    }

    /// Handle user registration
    async fn handle_register(&self, session: &mut Session, ctx: &ProxyContext) -> Result<()> {
        log::info!("[{}] Handling registration", ctx.request_id);

        // Read request body
        let body = self.read_request_body(session).await?;

        let request: crate::auth::RegisterRequest = serde_json::from_slice(&body)
            .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

        // Register user
        match register_user(
            &self.db_pool,
            &self.jwt_manager,
            request,
            self.settings.jwt.refresh_token_expiration,
        )
        .await
        {
            Ok(response) => {
                let json = serde_json::to_string(&response)
                    .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

                self.send_json_response(session, 201, json).await?;
            }
            Err(e) => {
                log::error!("[{}] Registration failed: {}", ctx.request_id, e);
                let error_msg = format!(r#"{{"error":"{}"}}"#, e);
                self.send_json_response(session, 400, error_msg).await?;
            }
        }

        Ok(())
    }

    /// Handle user login
    async fn handle_login(&self, session: &mut Session, ctx: &ProxyContext) -> Result<()> {
        log::info!("[{}] Handling login", ctx.request_id);

        let body = self.read_request_body(session).await?;

        let request: crate::auth::LoginRequest =
            serde_json::from_slice(&body).map_err(|_| Error::new_str("Invalid JSON"))?;

        match login_user(
            &self.db_pool,
            &self.jwt_manager,
            request,
            self.settings.jwt.refresh_token_expiration,
        )
        .await
        {
            Ok(response) => {
                let json = serde_json::to_string(&response)
                    .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

                self.send_json_response(session, 200, json).await?;
            }
            Err(e) => {
                log::error!("[{}] Login failed: {}", ctx.request_id, e);
                let error_msg = format!(r#"{{"error":"{}"}}"#, e);
                self.send_json_response(session, 401, error_msg).await?;
            }
        }

        Ok(())
    }

    /// Handle token refresh
    async fn handle_refresh(&self, session: &mut Session, ctx: &ProxyContext) -> Result<()> {
        log::info!("[{}] Handling token refresh", ctx.request_id);

        let body = self.read_request_body(session).await?;

        let request: crate::auth::RefreshRequest = serde_json::from_slice(&body)
            .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

        match refresh_token(
            &self.db_pool,
            &self.redis_client,
            &self.jwt_manager,
            request,
        )
        .await
        {
            Ok(response) => {
                let json = serde_json::to_string(&response)
                    .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

                self.send_json_response(session, 200, json).await?;
            }
            Err(e) => {
                log::error!("[{}] Token refresh failed: {}", ctx.request_id, e);
                let error_msg = format!(r#"{{"error":"{}"}}"#, e);
                self.send_json_response(session, 401, error_msg).await?;
            }
        }

        Ok(())
    }

    /// Handle user logout
    async fn handle_logout(&self, session: &mut Session, ctx: &ProxyContext) -> Result<()> {
        log::info!("[{}] Handling logout", ctx.request_id);

        // Extract access token from Authorization header
        let access_token = self.extract_token_from_header(session.req_header())?;

        let body = self.read_request_body(session).await?;

        let request: crate::auth::LogoutRequest = serde_json::from_slice(&body)
            .map_err(|e| Error::because(ErrorType::InternalError, "Invalid JSON", e))?;

        match logout_user(
            &self.db_pool,
            &self.redis_client,
            &self.jwt_manager,
            &access_token,
            request,
        )
        .await
        {
            Ok(()) => {
                let json = r#"{"message":"Logged out successfully"}"#.to_string();
                self.send_json_response(session, 200, json).await?;
            }
            Err(e) => {
                log::error!("[{}] Logout failed: {}", ctx.request_id, e);
                let error_msg = format!(r#"{{"error":"{}"}}"#, e);
                self.send_json_response(session, 400, error_msg).await?;
            }
        }

        Ok(())
    }

    /// Authenticate request using JWT
    async fn authenticate_request(
        &self,
        req: &RequestHeader,
        ctx: &mut ProxyContext,
    ) -> std::result::Result<(), String> {
        // Extract token from Authorization header
        let token = self
            .extract_token_from_header(req)
            .map_err(|e| format!("Token extraction failed: {}", e))?;

        // Check if token is blacklisted
        let is_blacklisted = self
            .redis_client
            .is_token_blacklisted(&token)
            .await
            .map_err(|e| format!("Redis error: {}", e))?;

        if is_blacklisted {
            return Err("Token has been revoked".to_string());
        }

        // Validate token
        let claims = self.jwt_manager.validate_token(&token)?;

        // Parse user ID
        let user_id = uuid::Uuid::parse_str(&claims.sub)
            .map_err(|_| "Invalid user ID in token".to_string())?;

        ctx.set_user_id(user_id);

        Ok(())
    }

    /// Extract JWT token from Authorization header
    fn extract_token_from_header(&self, req: &RequestHeader) -> Result<String> {
        let auth_header = req
            .headers
            .get("Authorization")
            .ok_or_else(|| pingora_core::Error::new_str("Missing Authorization header"))?
            .to_str()
            .map_err(|_| pingora_core::Error::new_str("Invalid Authorization header"))?;

        if !auth_header.starts_with("Bearer ") {
            return Err(pingora_core::Error::new_str("Invalid Authorization format"));
        }

        Ok(auth_header[7..].to_string())
    }

    /// Check rate limit
    async fn check_rate_limit(&self, ctx: &ProxyContext) -> std::result::Result<(), String> {
        let key = if let Some(user_id) = &ctx.user_id {
            format!("rate_limit:user:{}", user_id)
        } else if let Some(ip) = &ctx.client_ip {
            format!("rate_limit:ip:{}", ip)
        } else {
            format!("rate_limit:anonymous:{}", ctx.request_id)
        };

        let (allowed, count, _) = self
            .redis_client
            .check_rate_limit(
                &key,
                self.settings.middleware.rate_limit.requests_per_minute as i64,
                60,
            )
            .await
            .map_err(|e| format!("Rate limit check failed: {}", e))?;

        if !allowed {
            return Err(format!(
                "Rate limit exceeded: {}/{}",
                count, self.settings.middleware.rate_limit.requests_per_minute
            ));
        }

        Ok(())
    }

    /// Read request body
    async fn read_request_body(&self, session: &mut Session) -> Result<Vec<u8>> {
        use bytes::Buf;

        let mut body = Vec::new();

        while let Some(chunk) = session.read_request_body().await? {
            body.extend_from_slice(chunk.chunk());
        }

        Ok(body)
    }

    /// Send JSON response - FIXED: Accept owned String
    async fn send_json_response(
        &self,
        session: &mut Session,
        status: u16,
        json: String, // Changed from &str to String
    ) -> Result<()> {
        let mut resp = ResponseHeader::build(status, Some(4))?;
        resp.insert_header("Content-Type", "application/json")?;
        resp.insert_header("Content-Length", json.len().to_string())?;

        session.write_response_header(Box::new(resp), false).await?;

        // Convert String to Bytes
        let body = Bytes::from(json);
        session.write_response_body(Some(body), true).await?;

        Ok(())
    }

    /// Send 401 Unauthorized response
    async fn send_unauthorized_response(&self, session: &mut Session) -> Result<()> {
        let json = r#"{"error":"Unauthorized"}"#.to_string();
        self.send_json_response(session, 401, json).await
    }

    /// Send 429 Rate Limit response
    async fn send_rate_limit_response(&self, session: &mut Session) -> Result<()> {
        let json = r#"{"error":"Too many requests"}"#.to_string();
        self.send_json_response(session, 429, json).await
    }

    /// Send 404 Not Found response
    async fn send_not_found_response(&self, session: &mut Session) -> Result<()> {
        let json = r#"{"error":"Not found"}"#.to_string();
        self.send_json_response(session, 404, json).await
    }
}
