#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb-payments` is a library providing structures and utilities related to
//! the [`BIP70: Payment Protocol`] and a [`Wallet`] structure to allow receiving
//! payments.
//!
//! [`Wallet`]: wallet::Wallet
//! [`BIP70: Payment Protocol`]: https://github.com/bitcoin/bips/blob/master/bip-0070.mediawiki

pub mod wallet;

use bytes::Buf;
use http::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use prost::{DecodeError, Message};
use thiserror::Error;

#[allow(missing_docs)]
pub mod bip70 {
    //! This module contains structures related to the [`BIP70: Payment Protocol`]
    //!
    //! [`BIP70: Payment Protocol`]: https://github.com/bitcoin/bips/blob/master/bip-0070.mediawiki

    include!(concat!(env!("OUT_DIR"), "/bip70.rs"));
}

use bip70::Payment;

/// Error associated with payment preprocessing.
#[derive(Debug, Error)]
pub enum PreprocessingError {
    /// Missing the `application/bitcoincash-paymentack` header.
    #[error("missing accept header")]
    MissingAcceptHeader,
    /// Missing the `application/bitcoincash-payment` header.
    #[error("invalid content-type")]
    MissingContentTypeHeader,
    /// Failed to decode the `Payment` protobuf.
    #[error("payment decoding failure: {0}")]
    PaymentDecode(DecodeError),
}

/// Validates and parses the BIP70 payment.
pub async fn preprocess_payment<B: Buf>(
    headers: HeaderMap,
    body: B,
) -> Result<Payment, PreprocessingError> {
    // Bitcoin Cash Headers
    let bch_content_type_value = HeaderValue::from_static("application/bitcoincash-payment");
    let bch_accept_value = HeaderValue::from_static("application/bitcoincash-paymentack");

    // Check for content-type header
    if !headers
        .get_all(CONTENT_TYPE)
        .iter()
        .any(|header_val| header_val == bch_content_type_value)
    {
        return Err(PreprocessingError::MissingContentTypeHeader);
    }

    // Check for accept header
    if !headers
        .get_all(ACCEPT)
        .iter()
        .any(|header_val| header_val == bch_accept_value)
    {
        return Err(PreprocessingError::MissingAcceptHeader);
    }

    // Read and parse payment proto
    let payment = bip70::Payment::decode(body).map_err(PreprocessingError::PaymentDecode)?;

    Ok(payment)
}
