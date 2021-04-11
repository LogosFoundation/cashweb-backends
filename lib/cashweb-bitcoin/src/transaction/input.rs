//! This module contains the [`Input`] struct which represents a Bitcoin transaction input.
//! It enjoys [`Encodable`] and [`Decodable`].

use bytes::{Buf, BufMut};
use thiserror::Error;

use super::{
    outpoint::{DecodeError as OutpointDecodeError, Outpoint},
    script::Script,
};
use crate::{
    var_int::{DecodeError as VarIntDecodeError, VarInt},
    Decodable, Encodable,
};

/// Error associated with [`Input`] deserialization.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Failed to decode [`Outpoint`].
    #[error("outpoint: {0}")]
    Outpoint(OutpointDecodeError),
    /// Failed to decode script length [`VarInt`].
    #[error("script length: {0}")]
    ScriptLen(VarIntDecodeError),
    /// Exhausted buffer when decoding `script` field.
    #[error("script too short")]
    ScriptTooShort,
    /// Exhausted buffer when decoding `sequence` field.
    #[error("sequence number too short")]
    SequenceTooShort,
}

/// Represents an input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Input {
    pub outpoint: Outpoint,
    pub script: Script,
    pub sequence: u32,
}

impl Encodable for Input {
    #[inline]
    fn encoded_len(&self) -> usize {
        self.outpoint.encoded_len()
            + self.script.len_varint().encoded_len()
            + self.script.encoded_len()
            + 4
    }

    #[inline]
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        self.outpoint.encode_raw(buf);
        self.script.len_varint().encode_raw(buf);
        self.script.encode_raw(buf);
        buf.put_u32_le(self.sequence);
    }
}

impl Decodable for Input {
    type Error = DecodeError;

    #[inline]
    fn decode<B: Buf>(mut buf: &mut B) -> Result<Self, Self::Error> {
        // Parse outpoint
        let outpoint = Outpoint::decode(&mut buf).map_err(Self::Error::Outpoint)?;

        // Parse script
        let script_len: u64 = VarInt::decode(&mut buf)
            .map_err(Self::Error::ScriptLen)?
            .into();
        let script_len = script_len as usize;
        if buf.remaining() < script_len {
            return Err(Self::Error::ScriptTooShort);
        }
        let mut raw_script = vec![0; script_len];
        buf.copy_to_slice(&mut raw_script);
        let script = raw_script.into();

        // Parse sequence number
        if buf.remaining() < 4 {
            return Err(Self::Error::SequenceTooShort);
        }
        let sequence = buf.get_u32_le();

        Ok(Input {
            outpoint,
            script,
            sequence,
        })
    }
}
