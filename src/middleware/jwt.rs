use crate::auth::JwtManager;
use pingora_http::{RequestHeader, ResponseHeader};

pub struct JwtMiddleware {
    jwt_manager: JwtManager,
}

impl JwtMiddleware {
    pub fn new(jwt_manager: JwtManager) -> Self {
        Self { jwt_manager }
    }
    pub fn verify_request(&self, req: &RequestHeader) -> Option<String> {
        let auth_header = req.headers.get("Authorization")?;
        let auth_str = auth_header.to_str().ok()?;

        if !auth_str.starts_with("Bearer ") {
            log::warn!("Invalid authorization header format");
            return None;
        }

        let token = &auth_str[7..];

        match self.jwt_manager.validate_token(token) {
            Ok(claims) => {
                if claims.token_type != "access" {
                    log::warn!("Wrong token type: expected 'access', got '{}'", claims.token_type);
                    return None;
                }
                
                log::debug!("Token verified for user: {}", claims.sub);
                Some(claims.sub)
            }
            Err(e) => {
                log::warn!("Token verification failed: {}", e);
                None
            }
        }
    }

    pub fn unauthorized_response() -> ResponseHeader {
        let mut resp = ResponseHeader::build(401, None).unwrap();
        resp.insert_header("Content-Type", "application/json")
            .unwrap();
        resp.insert_header("WWW-Authenticate", "Bearer")
            .unwrap();
        resp
    }

    pub fn requires_auth(path: &str) -> bool {
        let public_paths = [
            "/auth/register",
            "/auth/login", 
            "/health",
        ];

        !public_paths.iter().any(|&p| path.starts_with(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_requires_auth() {
        assert!(!JwtMiddleware::requires_auth("/health"));
        assert!(!JwtMiddleware::requires_auth("/auth/register"));
        assert!(!JwtMiddleware::requires_auth("/auth/login"));
        
        assert!(JwtMiddleware::requires_auth("/"));
        assert!(JwtMiddleware::requires_auth("/api/users"));
        assert!(JwtMiddleware::requires_auth("/auth/refresh"));
        assert!(JwtMiddleware::requires_auth("/auth/logout"));
    }
}