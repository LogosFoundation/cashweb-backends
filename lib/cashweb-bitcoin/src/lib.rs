#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb-bitcoin` is a library providing serialization/deserialization of Bitcoin structures,
//!  utility methods for signing, and methods for [`Hierarchical Deterministic Wallets`] use.
//!
//! [`Hierarchical Deterministic Wallets`]: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki

pub mod bip32;
pub mod prelude;
pub mod transaction;
pub mod var_int;

use std::convert::TryFrom;

use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Insufficient capacity in buffer when encoding a Bitcoin structure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("buffer has insufficient capacity")]
pub struct InsufficientCapacity;

/// Provides a common interface for the serialization of bitcoin structures.
pub trait Encodable: Sized {
    /// Returns the encoded length of the message.
    fn encoded_len(&self) -> usize;

    /// Encodes structure to a buffer.
    #[inline]
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<(), InsufficientCapacity> {
        if buf.remaining_mut() < self.encoded_len() {
            return Err(InsufficientCapacity);
        }
        self.encode_raw(buf);
        Ok(())
    }

    /// Encodes structure to a buffer. This panics if buffer contains insufficient capacity.
    fn encode_raw<B: BufMut>(&self, buf: &mut B);
}

/// Provides a common interface for the deserialization of bitcoin structures.
pub trait Decodable: Sized {
    /// Error associated with decoding a Bitcoin structure.
    type Error;

    /// Decode a buffer.
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, Self::Error>;
}

/// Enumeration of all standard Bitcoin networks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Main network
    Mainnet,
    /// Test network
    Testnet,
    /// Regression test network
    Regtest,
}

/// Network was unexpected.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("unexpected network given")]
pub struct UnexpectedNetwork;

impl TryFrom<String> for Network {
    type Error = UnexpectedNetwork;

    fn try_from(network: String) -> Result<Self, Self::Error> {
        match network.as_str() {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "regtest" => Ok(Self::Regtest),
            _ => Err(UnexpectedNetwork),
        }
    }
}

impl Into<String> for Network {
    fn into(self) -> String {
        match self {
            Self::Mainnet => "mainnet".to_string(),
            Self::Testnet => "testnet".to_string(),
            Self::Regtest => "regtest".to_string(),
        }
    }
}

impl std::string::ToString for Network {
    fn to_string(&self) -> String {
        match self {
            Self::Mainnet => "mainnet".to_string(),
            Self::Testnet => "testnet".to_string(),
            Self::Regtest => "regtest".to_string(),
        }
    }
}
