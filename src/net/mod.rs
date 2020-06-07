pub mod metadata;
pub mod payments;
pub mod peers;
pub mod protection;

pub use metadata::*;
pub use payments::*;
pub use peers::*;
pub use protection::*;

use std::{convert::Infallible, fmt};

use bitcoincash_addr::Address;
use warp::{
    filters::body::BodyDeserializeError,
    http::Response,
    hyper::Body,
    reject::{PayloadTooLarge, Reject, Rejection},
};

pub const SAMPLING: &str = "Sample-Peers";
pub const HEADER_VALUE_FALSE: &str = "false";

#[derive(Debug)]
pub struct AddressDecode(
    bitcoincash_addr::cashaddr::DecodingError,
    bitcoincash_addr::base58::DecodingError,
);

impl Reject for AddressDecode {}

impl fmt::Display for AddressDecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.0, self.1)
    }
}

/// Helper method for decoding an address string.
pub fn address_decode(addr_str: &str) -> Result<Address, AddressDecode> {
    // Convert address
    Address::decode(&addr_str).map_err(|(cash_err, base58_err)| AddressDecode(cash_err, base58_err))
}

impl IntoResponse for AddressDecode {
    fn to_status(&self) -> u16 {
        400
    }
}

/// Helper trait for converting errors into a response.
pub trait IntoResponse: fmt::Display + Sized {
    /// Convert error into a status code.
    fn to_status(&self) -> u16;

    /// Convert error into a `Response`.
    fn into_response(&self) -> Response<Body> {
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

/// Global rejection handler, takes an rejection and converts it into a `Response`.
pub async fn handle_rejection(err: Rejection) -> Result<Response<Body>, Infallible> {
    if let Some(err) = err.find::<BodyDeserializeError>() {
        log::error!("{:#?}", err);
        return Ok(Response::builder()
            .status(400)
            .body(Body::from("unexpected body serialization"))
            .unwrap());
    }

    if let Some(err) = err.find::<AddressDecode>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }
    if let Some(err) = err.find::<GetMetadataError>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }
    if let Some(err) = err.find::<PutMetadataError>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }
    if let Some(err) = err.find::<PaymentError>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }

    if let Some(err) = err.find::<PaymentRequestError>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }

    if let Some(err) = err.find::<PeerError>() {
        log::error!("{:#?}", err);
        return Ok(err.into_response());
    }

    if let Some(err) = err.find::<ProtectionError>() {
        log::error!("{:#?}", err);
        return Ok(protection_error_recovery(err).await);
    }

    if let Some(err) = err.find::<PayloadTooLarge>() {
        log::error!("{:#?}", err);
        return Ok(Response::builder().status(413).body(Body::empty()).unwrap());
    }

    if err.is_not_found() {
        log::error!("{:#?}", err);
        return Ok(Response::builder().status(404).body(Body::empty()).unwrap());
    }

    log::error!("unexpected error found {:?}", err);
    Ok(Response::builder().status(500).body(Body::empty()).unwrap())
}
