//! This module contains the [`Outpoint`] struct which represents a Bitcoin transaction outpoint.
//! It enjoys [`Encodable`] and [`Decodable`].

use bytes::{Buf, BufMut};
use thiserror::Error;

use crate::{Decodable, Encodable};

/// Represents an outpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Outpoint {
    pub tx_id: [u8; 32],
    pub vout: u32,
}

impl Encodable for Outpoint {
    #[inline]
    fn encoded_len(&self) -> usize {
        32 + 4
    }

    #[inline]
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        buf.put(&self.tx_id[..]);
        buf.put_u32_le(self.vout);
    }
}

/// Error associated with [`Outpoint`] deserialization.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("outpoint too short")]
pub struct DecodeError;

impl Decodable for Outpoint {
    type Error = DecodeError;

    #[inline]
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, Self::Error> {
        if buf.remaining() < 32 + 4 {
            return Err(DecodeError);
        }
        let mut tx_id = [0; 32];
        buf.copy_to_slice(&mut tx_id);
        let vout = buf.get_u32_le();

        Ok(Outpoint { tx_id, vout })
    }
}
