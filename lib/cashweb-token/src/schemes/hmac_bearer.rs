//! This module contains [`HmacScheme`] which provides a rudimentary HMAC validation scheme.

use ring::hmac;
use thiserror::Error;

/// Error associated with basic HMAC token validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    /// Failed to decode token.
    #[error("failed to decode token: {0}")]
    Base64(base64::DecodeError),
    /// Token was invalid.
    #[error("invalid token")]
    Invalid,
}

/// Basic HMAC token scheme.
#[derive(Debug)]
pub struct HmacScheme {
    key: hmac::Key,
}

impl HmacScheme {
    /// Create a new HMAC scheme using a speficied secret key.
    pub fn new(key: &[u8]) -> Self {
        let key = hmac::Key::new(hmac::HMAC_SHA256, key);
        Self { key }
    }

    /// Construct a token.
    pub fn construct_token(&self, data: &[u8]) -> String {
        let url_safe_config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
        let tag = hmac::sign(&self.key, data);
        base64::encode_config(tag.as_ref(), url_safe_config)
    }

    /// Validate a token.
    pub fn validate_token(&self, data: &[u8], token: &str) -> Result<(), ValidationError> {
        let url_safe_config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
        let tag = base64::decode_config(token, url_safe_config).map_err(ValidationError::Base64)?;
        hmac::verify(&self.key, data, &tag).map_err(|_| ValidationError::Invalid)
    }
}
