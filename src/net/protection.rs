use std::sync::Arc;

use bitcoincash_addr::Address;
use cashweb::bitcoin_client::{BitcoinClient, HttpClient};
use cashweb::token::{extract_pop, schemes::hmac_bearer::*, split_pop_token};
use http::header::HeaderMap;
use thiserror::Error;
use warp::{http::Response, hyper::Body, reject::Reject};

use crate::net::payments::{generate_payment_request, Wallet};

#[derive(Debug, Error)]
pub enum ProtectionError {
    #[error("missing token: {0:?}")] // TODO: Make this prettier
    MissingToken(Address, Wallet, BitcoinClient<HttpClient>),
    #[error("validation failed: {0}")]
    Validation(ValidationError),
}

pub async fn protection_error_recovery(err: &ProtectionError) -> Response<Body> {
    match err {
        ProtectionError::Validation(_) => Response::builder()
            .status(400)
            .body(Body::from(err.to_string()))
            .unwrap(),
        ProtectionError::MissingToken(addr, wallet, bitcoin_client) => {
            // TODO: Remove clones here
            match generate_payment_request(addr.clone(), wallet.clone(), bitcoin_client.clone())
                .await
            {
                Ok(ok) => ok,
                Err(err) => Response::builder()
                    .status(400)
                    .body(Body::from(err.to_string()))
                    .unwrap(),
            }
        }
    }
}

impl Reject for ProtectionError {}

pub async fn pop_protection(
    addr: Address,
    header_map: HeaderMap,
    access_token: Option<String>,
    token_scheme: Arc<HmacScheme>,
    wallet: Wallet,
    bitcoin_client: BitcoinClient<HttpClient>,
) -> Result<Address, ProtectionError> {
    match extract_pop(&header_map).or(access_token
        .as_ref()
        .and_then(|access_token| split_pop_token(access_token)))
    {
        Some(pop_token) => {
            token_scheme
                .validate_token(&addr.as_body().to_vec(), pop_token)
                .map_err(ProtectionError::Validation)?;
            Ok(addr)
        }
        None => Err(ProtectionError::MissingToken(addr, wallet, bitcoin_client)),
    }
}
