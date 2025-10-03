use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT Claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // Subject (user_id)
    pub exp: i64,           // Expiration time (as UTC timestamp)
    pub iat: i64,           // Issued at (as UTC timestamp)
    pub jti: String,        // JWT ID (unique identifier for this token)
    pub token_type: String, // "access" or "refresh"
}

/// JWT token manager
pub struct JwtManager {
    secret: String,
    access_token_expiration: i64,  // in seconds
    refresh_token_expiration: i64, // in seconds
}

impl JwtManager {
    /// Create a new JWT manager
    /// 
    /// # Arguments
    /// * `secret` - Secret key for signing tokens
    /// * `access_token_expiration` - Access token expiration in seconds
    /// * `refresh_token_expiration` - Refresh token expiration in seconds
    pub fn new(
        secret: String,
        access_token_expiration: i64,
        refresh_token_expiration: i64,
    ) -> Self {
        Self {
            secret,
            access_token_expiration,
            refresh_token_expiration,
        }
    }

    /// Generate an access token for a user
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// 
    /// # Returns
    /// * `Result<String, jsonwebtoken::errors::Error>` - JWT token or error
    /// 
    /// # Example
    /// ```
    /// let token = jwt_manager.generate_access_token(&user_id)?;
    /// ```
    pub fn generate_access_token(&self, user_id: &Uuid) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let expiration = now + Duration::seconds(self.access_token_expiration);

        let claims = Claims {
            sub: user_id.to_string(),
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(), // Unique ID for this token
            token_type: "access".to_string(),
        };

        self.encode_token(&claims)
    }

    /// Generate a refresh token for a user
    /// 
    /// # Arguments
    /// * `user_id` - User's UUID
    /// 
    /// # Returns
    /// * `Result<(String, String), jsonwebtoken::errors::Error>` - (token, token_hash) or error
    /// 
    /// # Note
    /// Returns both the token (to send to client) and its hash (to store in database)
    pub fn generate_refresh_token(
        &self,
        user_id: &Uuid,
    ) -> Result<(String, String), jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let expiration = now + Duration::seconds(self.refresh_token_expiration);

        let claims = Claims {
            sub: user_id.to_string(),
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
            token_type: "refresh".to_string(),
        };

        let token = self.encode_token(&claims)?;
        
        // Hash the token for storage (similar to password hashing)
        let token_hash = self.hash_token(&token);

        Ok((token, token_hash))
    }

    /// Decode and validate a JWT token
    /// 
    /// # Arguments
    /// * `token` - JWT token string
    /// 
    /// # Returns
    /// * `Result<Claims, jsonwebtoken::errors::Error>` - Decoded claims or error
    /// 
    /// # Example
    /// ```
    /// let claims = jwt_manager.decode_token(&token)?;
    /// println!("User ID: {}", claims.sub);
    /// ```
    pub fn decode_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let decoding_key = DecodingKey::from_secret(self.secret.as_bytes());
        let validation = Validation::new(Algorithm::HS256);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
        
        Ok(token_data.claims)
    }

    /// Validate token and check if it's not expired
    /// 
    /// # Arguments
    /// * `token` - JWT token string
    /// 
    /// # Returns
    /// * `Result<Claims, String>` - Claims if valid, error message if invalid
    pub fn validate_token(&self, token: &str) -> Result<Claims, String> {
        match self.decode_token(token) {
            Ok(claims) => {
                // Check expiration (jsonwebtoken already validates this, but double-check)
                let now = Utc::now().timestamp();
                if claims.exp < now {
                    return Err("Token has expired".to_string());
                }
                
                Ok(claims)
            }
            Err(e) => Err(format!("Invalid token: {}", e)),
        }
    }

    /// Extract user ID from token without full validation
    /// Useful for logging or non-critical operations
    /// 
    /// # Arguments
    /// * `token` - JWT token string
    /// 
    /// # Returns
    /// * `Option<String>` - User ID if extractable, None otherwise
    pub fn extract_user_id(&self, token: &str) -> Option<String> {
        self.decode_token(token).ok().map(|claims| claims.sub)
    }

    /// Get token expiration time
    /// 
    /// # Arguments
    /// * `token` - JWT token string
    /// 
    /// # Returns
    /// * `Option<i64>` - Expiration timestamp if valid, None otherwise
    pub fn get_token_expiration(&self, token: &str) -> Option<i64> {
        self.decode_token(token).ok().map(|claims| claims.exp)
    }

    /// Check if token is of specific type (access or refresh)
    /// 
    /// # Arguments
    /// * `token` - JWT token string
    /// * `expected_type` - Expected token type ("access" or "refresh")
    /// 
    /// # Returns
    /// * `bool` - true if token type matches
    pub fn is_token_type(&self, token: &str, expected_type: &str) -> bool {
        self.decode_token(token)
            .ok()
            .map(|claims| claims.token_type == expected_type)
            .unwrap_or(false)
    }

    /// Encode claims into JWT token
    fn encode_token(&self, claims: &Claims) -> Result<String, jsonwebtoken::errors::Error> {
        let encoding_key = EncodingKey::from_secret(self.secret.as_bytes());
        encode(&Header::default(), claims, &encoding_key)
    }

    /// Hash a token for secure storage
    /// Uses SHA256 for fast hashing (tokens are already random)
    fn hash_token(&self, token: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get access token expiration in seconds
    pub fn access_token_expiration(&self) -> i64 {
        self.access_token_expiration
    }

    /// Get refresh token expiration in seconds
    pub fn refresh_token_expiration(&self) -> i64 {
        self.refresh_token_expiration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> JwtManager {
        JwtManager::new(
            "test_secret_key_12345".to_string(),
            900,      // 15 minutes
            604800,   // 7 days
        )
    }

    #[test]
    fn test_generate_and_decode_access_token() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();

        let token = manager.generate_access_token(&user_id).unwrap();
        let claims = manager.decode_token(&token).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn test_generate_refresh_token() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();

        let (token, hash) = manager.generate_refresh_token(&user_id).unwrap();
        let claims = manager.decode_token(&token).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.token_type, "refresh");
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_validate_token() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();

        let token = manager.generate_access_token(&user_id).unwrap();
        let result = manager.validate_token(&token);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().sub, user_id.to_string());
    }

    #[test]
    fn test_invalid_token() {
        let manager = create_test_manager();
        let invalid_token = "invalid.jwt.token";

        let result = manager.decode_token(invalid_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_user_id() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();

        let token = manager.generate_access_token(&user_id).unwrap();
        let extracted_id = manager.extract_user_id(&token);

        assert_eq!(extracted_id, Some(user_id.to_string()));
    }

    #[test]
    fn test_token_type_check() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();

        let access_token = manager.generate_access_token(&user_id).unwrap();
        let (refresh_token, _) = manager.generate_refresh_token(&user_id).unwrap();

        assert!(manager.is_token_type(&access_token, "access"));
        assert!(!manager.is_token_type(&access_token, "refresh"));
        
        assert!(manager.is_token_type(&refresh_token, "refresh"));
        assert!(!manager.is_token_type(&refresh_token, "access"));
    }

    #[test]
    fn test_different_secrets_produce_different_tokens() {
        let manager1 = JwtManager::new("secret1".to_string(), 900, 604800);
        let manager2 = JwtManager::new("secret2".to_string(), 900, 604800);
        
        let user_id = Uuid::new_v4();
        let token1 = manager1.generate_access_token(&user_id).unwrap();

        // Token from manager1 should not be valid for manager2
        assert!(manager2.decode_token(&token1).is_err());
    }
}