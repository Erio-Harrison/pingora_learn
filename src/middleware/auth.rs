// src/middleware/auth.rs
use crate::config::settings::AuthConfig;
use pingora_http::RequestHeader;
use std::error::Error;
use std::fmt;

pub struct AuthMiddleware {
    config: AuthConfig,
}

#[derive(Debug)]
pub struct AuthError(String);

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Authentication error: {}", self.0)
    }
}

impl Error for AuthError {}

impl AuthMiddleware {
    pub fn new(config: &AuthConfig) -> Self {
        AuthMiddleware {
            config: config.clone(),
        }
    }
    
    pub fn check_auth(&self, req: &RequestHeader) -> Result<(), AuthError> {
        if !self.config.enabled {
            return Ok(());
        }
        
        match self.config.auth_type.as_str() {
            "bearer" => self.check_bearer_token(req),
            "basic" => self.check_basic_auth(req),
            "api_key" => self.check_api_key(req),
            _ => Err(AuthError("Unsupported authentication type".to_string())),
        }
    }
    
    fn check_bearer_token(&self, req: &RequestHeader) -> Result<(), AuthError> {
        let auth_header = req.headers.get("Authorization")
            .ok_or_else(|| AuthError("Missing Authorization header".to_string()))?;
        
        let auth_str = auth_header.to_str()
            .map_err(|_| AuthError("Invalid Authorization header format".to_string()))?;
        
        if !auth_str.starts_with("Bearer ") {
            return Err(AuthError("Invalid Bearer token format".to_string()));
        }
        
        let token = &auth_str[7..];
        
        if self.config.valid_tokens.contains(&token.to_string()) {
            Ok(())
        } else {
            Err(AuthError("Invalid token".to_string()))
        }
    }
    
    fn check_basic_auth(&self, req: &RequestHeader) -> Result<(), AuthError> {
        let auth_header = req.headers.get("Authorization")
            .ok_or_else(|| AuthError("Missing Authorization header".to_string()))?;
        
        let auth_str = auth_header.to_str()
            .map_err(|_| AuthError("Invalid Authorization header format".to_string()))?;
        
        if !auth_str.starts_with("Basic ") {
            return Err(AuthError("Invalid Basic authentication format".to_string()));
        }
        
        let encoded = &auth_str[6..];
        if self.config.valid_tokens.contains(&encoded.to_string()) {
            Ok(())
        } else {
            Err(AuthError("Invalid authentication information".to_string()))
        }
    }
    
    fn check_api_key(&self, req: &RequestHeader) -> Result<(), AuthError> {
        let api_key = req.headers.get("X-API-Key")
            .ok_or_else(|| AuthError("Missing X-API-Key header".to_string()))?;
        
        let key_str = api_key.to_str()
            .map_err(|_| AuthError("Invalid API Key format".to_string()))?;
        
        if self.config.valid_tokens.contains(&key_str.to_string()) {
            Ok(())
        } else {
            Err(AuthError("Invalid API Key".to_string()))
        }
    }
}