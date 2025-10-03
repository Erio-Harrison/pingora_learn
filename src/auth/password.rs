use bcrypt::{hash, verify, DEFAULT_COST};
use thiserror::Error;

/// Custom password error type
#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Password must be at least 8 characters long")]
    TooShort,
    
    #[error("Password must contain at least one uppercase letter")]
    NoUppercase,
    
    #[error("Password must contain at least one lowercase letter")]
    NoLowercase,
    
    #[error("Password must contain at least one digit")]
    NoDigit,
    
    #[error("Bcrypt error: {0}")]
    BcryptError(#[from] bcrypt::BcryptError),
}

/// Password hashing and verification manager
pub struct PasswordManager;

impl PasswordManager {
    /// Hash a plain text password
    pub fn hash(password: &str) -> Result<String, PasswordError> {
        // Validate password strength
        Self::validate_password_strength(password)?;
        
        // Hash with default cost (12 rounds)
        Ok(hash(password, DEFAULT_COST)?)
    }

    /// Hash a password with custom cost
    pub fn hash_with_cost(password: &str, cost: u32) -> Result<String, PasswordError> {
        // Validate password strength
        Self::validate_password_strength(password)?;
        
        // Hash with custom cost
        Ok(hash(password, cost)?)
    }

    /// Verify a password against a hash
    pub fn verify(password: &str, hash: &str) -> Result<bool, PasswordError> {
        Ok(verify(password, hash)?)
    }

    /// Validate password strength
    fn validate_password_strength(password: &str) -> Result<(), PasswordError> {
        if password.len() < 8 {
            return Err(PasswordError::TooShort);
        }

        if !password.chars().any(|c| c.is_uppercase()) {
            return Err(PasswordError::NoUppercase);
        }

        if !password.chars().any(|c| c.is_lowercase()) {
            return Err(PasswordError::NoLowercase);
        }

        if !password.chars().any(|c| c.is_numeric()) {
            return Err(PasswordError::NoDigit);
        }

        Ok(())
    }

    /// Check if password needs rehashing
    pub fn needs_rehash(hash: &str, target_cost: u32) -> bool {
        if let Some(cost_str) = hash.split('$').nth(2) {
            if let Ok(current_cost) = cost_str.parse::<u32>() {
                return current_cost != target_cost;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "TestPassword123";
        let hashed = PasswordManager::hash(password).unwrap();
        
        assert!(PasswordManager::verify(password, &hashed).unwrap());
        assert!(!PasswordManager::verify("WrongPassword", &hashed).unwrap());
    }

    #[test]
    fn test_password_validation() {
        // Valid password
        assert!(PasswordManager::hash("ValidPass123").is_ok());
        
        // Too short
        assert!(matches!(
            PasswordManager::hash("Short1"),
            Err(PasswordError::TooShort)
        ));
        
        // No uppercase
        assert!(matches!(
            PasswordManager::hash("nouppercase123"),
            Err(PasswordError::NoUppercase)
        ));
        
        // No lowercase
        assert!(matches!(
            PasswordManager::hash("NOLOWERCASE123"),
            Err(PasswordError::NoLowercase)
        ));
        
        // No digit
        assert!(matches!(
            PasswordManager::hash("NoDigitPassword"),
            Err(PasswordError::NoDigit)
        ));
    }

    #[test]
    fn test_needs_rehash() {
        let password = "TestPassword123";
        let hashed = PasswordManager::hash_with_cost(password, 10).unwrap();
        
        assert!(!PasswordManager::needs_rehash(&hashed, 10));
        assert!(PasswordManager::needs_rehash(&hashed, 12));
    }
}