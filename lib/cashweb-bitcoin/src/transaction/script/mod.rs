//! This module contains the [`Script`] struct which represents a Bitcoin transaction script.
//! It enjoys [`Encodable`], and provides some utility methods.

pub mod opcodes;

use crate::{var_int::VarInt, Encodable};

use bytes::BufMut;

/// Represents a script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script(pub Vec<u8>);

impl Into<Vec<u8>> for Script {
    fn into(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for Script {
    fn from(raw: Vec<u8>) -> Self {
        Script(raw)
    }
}

impl Script {
    /// Check whether the script is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Length of the script.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Length of the script as `VarInt`.
    #[inline]
    pub fn len_varint(&self) -> VarInt {
        VarInt(self.len() as u64)
    }

    /// Convert the script into the underlying bytes.
    #[inline]
    pub fn into_bytes(self) -> Vec<u8> {
        self.into()
    }

    /// Converts the script into a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Checks whether the script fits the OP_RETURN pattern.
    #[inline]
    pub fn is_op_return(&self) -> bool {
        !self.0.is_empty() && self.0[0] == opcodes::OP_RETURN
    }

    /// Checks whether the scripts the P2PKH pattern.
    #[inline]
    pub fn is_p2pkh(&self) -> bool {
        self.0.len() == 25
            && self.0[0] == opcodes::OP_DUP
            && self.0[1] == opcodes::OP_HASH160
            && self.0[2] == opcodes::OP_PUSHBYTES_20
            && self.0[23] == opcodes::OP_EQUALVERIFY
            && self.0[24] == opcodes::OP_CHECKSIG
    }
}

impl Encodable for Script {
    #[inline]
    fn encoded_len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        buf.put(&self.0[..]);
    }
}
