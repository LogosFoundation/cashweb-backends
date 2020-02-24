use std::sync::Arc;

use bitcoincash_addr::Address;
use cashweb::token::{extract_pop, schemes::hmac_bearer::HmacTokenScheme, TokenValidator};
use http::header::HeaderMap;
use warp::reject::Reject;

#[derive(Debug)]
pub enum ProtectionError {
    MissingToken,
}

impl Reject for ProtectionError {}

pub async fn pop_protection(
    addr: Address,
    header_map: HeaderMap,
    token_scheme: Arc<HmacTokenScheme>,
) -> Result<(Address), ProtectionError> {
    let pop_token = extract_pop(&header_map).ok_or(ProtectionError::MissingToken)?;
    token_scheme
        .validate_token(addr.as_body().to_vec(), pop_token)
        .await;
    Ok((addr))
}
