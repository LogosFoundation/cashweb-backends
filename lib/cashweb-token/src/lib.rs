#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb-token` is a library providing utility methods for the [`POP Token Protocol`].
//!
//! [`POP Token Protocol`]: https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki

pub mod schemes;

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};

/// Extract a POP token from `Authorization` header.
pub fn extract_pop_header(value: &HeaderValue) -> Option<&str> {
    value.to_str().ok().and_then(split_pop_token)
}

/// Split the POP token, removing the prefix "POP".
pub fn split_pop_token(full_token: &str) -> Option<&str> {
    if full_token.len() > 4 && &full_token[..4] == "POP " {
        return Some(&full_token[4..]);
    }
    None
}

/// Extract the first POP token from [`HeaderMap`].
pub fn extract_pop(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(AUTHORIZATION)
        .iter()
        .find_map(extract_pop_header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_ok() {
        split_pop_token("POP abc").unwrap();
    }

    #[test]
    fn test_split_short() {
        assert_eq!(split_pop_token("A"), None);
    }

    #[test]
    fn test_split_err() {
        assert_eq!(split_pop_token("ABC d"), None);
    }
}
