//! This module contains the [`Output`] struct which represents a Bitcoin transaction output.
//! It enjoys [`Encodable`] and [`Decodable`].

use bytes::{Buf, BufMut};
use thiserror::Error;

use super::script::Script;
use crate::{
    var_int::{DecodeError as VarIntDecodeError, VarInt},
    Decodable, Encodable,
};

/// Error associated with [`Output`] deserialization.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Value is too short.
    #[error("value too short")]
    ValueTooShort,
    /// Unable to decode the script length variable-length integer.
    #[error("script length: {0}")]
    ScriptLen(VarIntDecodeError),
    /// Script is too short.
    #[error("script too short")]
    ScriptTooShort,
}

/// Represents an output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Output {
    pub value: u64,
    pub script: Script,
}

impl Encodable for Output {
    #[inline]
    fn encoded_len(&self) -> usize {
        8 + self.script.len_varint().encoded_len() + self.script.encoded_len()
    }

    #[inline]
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        buf.put_u64_le(self.value);
        self.script.len_varint().encode_raw(buf);
        self.script.encode_raw(buf);
    }
}

impl Decodable for Output {
    type Error = DecodeError;

    #[inline]
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, Self::Error> {
        // Get value
        if buf.remaining() < 8 {
            return Err(Self::Error::ValueTooShort);
        }
        let value = buf.get_u64_le();

        // Get script
        let script_len: u64 = VarInt::decode(buf).map_err(Self::Error::ScriptLen)?.into();
        let script_len = script_len as usize;
        if buf.remaining() < script_len {
            return Err(Self::Error::ScriptTooShort);
        }
        let mut raw_script = vec![0; script_len];
        buf.copy_to_slice(&mut raw_script);
        let script = raw_script.into();
        Ok(Output { value, script })
    }
}
