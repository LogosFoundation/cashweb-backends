mod messages;
mod payments;
mod profiles;
mod protection;
mod ws;

pub use messages::*;
pub use payments::*;
pub use profiles::*;
pub use protection::*;
pub use ws::*;

use std::{convert::Infallible, fmt};

use bitcoincash_addr::Address;
use thiserror::Error;
use tracing::error;
use warp::{
    http::Response,
    hyper::Body,
    reject::{PayloadTooLarge, Reject, Rejection},
};

#[derive(Debug, Error)]
pub enum AddressDecode {
    #[error("address decoding failed: {0}, {1}")]
    Decode(
        bitcoincash_addr::cashaddr::DecodingError,
        bitcoincash_addr::base58::DecodingError,
    ),
    #[error("expected address payload of length 20, found {0}")]
    UnexpectedBodyLength(usize),
}

impl Reject for AddressDecode {}

pub fn address_decode(addr_str: &str) -> Result<Address, AddressDecode> {
    // Convert address
    let address = Address::decode(addr_str)
        .map_err(|(cash_err, base58_err)| AddressDecode::Decode(cash_err, base58_err))?;

    // Check address payload is correct length
    let body_len = address.as_body().len();
    if body_len != 20 {
        return Err(AddressDecode::UnexpectedBodyLength(body_len));
    }
    Ok(address)
}

impl ToResponse for AddressDecode {
    fn to_status(&self) -> u16 {
        400
    }
}

pub trait ToResponse: fmt::Display + Sized {
    fn to_status(&self) -> u16;

    fn to_response(&self) -> Response<Body> {
        let status = self.to_status();

        if status != 500 {
            Response::builder()
                .status(status)
                .body(Body::from(self.to_string()))
                .unwrap()
        } else {
            Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap()
        }
    }
}

pub async fn handle_rejection(err: Rejection) -> Result<Response<Body>, Infallible> {
    if let Some(err) = err.find::<AddressDecode>() {
        error!(message = "failed to decode address", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<GetProfileError>() {
        error!(message = "failed to get profile", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<PutProfileError>() {
        error!(message = "failed to put profile", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<GetMessageError>() {
        error!(message = "failed to get messages", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<PutMessageError>() {
        error!(message = "failed to put messages", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<PaymentError>() {
        error!(message = "payment failed", error = %err);
        return Ok(err.to_response());
    }

    if let Some(err) = err.find::<ProtectionError>() {
        error!(message = "protection triggered", error = %err);
        return Ok(protection_error_recovery(err).await);
    }

    if err.find::<PayloadTooLarge>().is_some() {
        error!("payload too large");
        return Ok(Response::builder().status(413).body(Body::empty()).unwrap());
    }

    if err.is_not_found() {
        error!("page not found");
        return Ok(Response::builder().status(404).body(Body::empty()).unwrap());
    }

    error!(message = "unexpected error", error = ?err);
    Ok(Response::builder().status(500).body(Body::empty()).unwrap())
}
