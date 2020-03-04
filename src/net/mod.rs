pub mod filters;
pub mod messages;
pub mod payments;
pub mod protection;
pub mod ws;

pub use filters::*;
pub use messages::*;
pub use payments::*;
pub use protection::*;
pub use ws::*;

use std::convert::Infallible;

use bitcoincash_addr::Address;
use warp::{
    http::Response,
    hyper::Body,
    reject::{Reject, Rejection},
};

#[derive(Debug)]
pub struct AddressDecode(
    bitcoincash_addr::cashaddr::DecodingError,
    bitcoincash_addr::base58::DecodingError,
);

impl Reject for AddressDecode {}

pub fn address_decode(addr_str: &str) -> Result<Address, AddressDecode> {
    // Convert address
    Address::decode(&addr_str).map_err(|(cash_err, base58_err)| AddressDecode(cash_err, base58_err))
}

pub fn address_recovery(err: &AddressDecode) -> Response<Body> {
    Response::builder()
        .status(400)
        .body(Body::from(format!("{}, {}", err.0, err.1)))
        .unwrap()
}

pub async fn handle_rejection(err: Rejection) -> Result<Response<Body>, Infallible> {
    if let Some(err) = err.find::<AddressDecode>() {
        log::error!("{:#?}", err);
        return Ok(address_recovery(err));
    }
    if let Some(err) = err.find::<FilterError>() {
        log::error!("{:#?}", err);
        return Ok(filter_error_recovery(err));
    }
    if let Some(err) = err.find::<PaymentError>() {
        log::error!("{:#?}", err);
        return Ok(payment_error_recovery(err));
    }
    if let Some(err) = err.find::<ProtectionError>() {
        log::error!("{:#?}", err);
        return Ok(protection_error_recovery(err).await);
    }
    unreachable!()
}
