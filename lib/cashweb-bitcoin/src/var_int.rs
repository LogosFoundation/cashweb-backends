//! This module contains the [`VarInt`] struct which represents a variable-length integer.
//! It enjoys [`Encodable`] and [`Decodable`].

use bytes::{Buf, BufMut};
use thiserror::Error;

use super::{Decodable, Encodable};

/// Error associated with [`VarInt`] deserialization.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Buffer supplied was too short
    #[error("varint too short")]
    TooShort,
    /// The [`VarInt`] supplied was non-minimal.
    #[error("varint non-minimal")]
    NonMinimal,
}

/// Represents a variable-length integer.
#[derive(Clone, Debug, PartialEq)]
pub struct VarInt(pub u64);

impl Into<u64> for VarInt {
    fn into(self) -> u64 {
        self.0
    }
}

impl Encodable for VarInt {
    #[inline]
    fn encoded_len(&self) -> usize {
        match self.0 {
            0..=0xfc => 1,
            0xfd..=0xffff => 3,
            0x10000..=0xffffffff => 5,
            _ => 9,
        }
    }

    #[inline]
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        match self.0 {
            0..=0xfc => {
                buf.put_uint_le(self.0, 1);
            }
            0xfd..=0xffff => {
                buf.put_u8(0xfd);
                buf.put_uint_le(self.0, 2);
            }
            0x10000..=0xffffffff => {
                buf.put_u8(0xfe);
                buf.put_uint_le(self.0, 4);
            }
            _ => {
                buf.put_u8(0xff);
                buf.put_u64_le(self.0);
            }
        }
    }
}

impl Decodable for VarInt {
    type Error = DecodeError;

    /// Parse variable-length integer.
    #[inline]
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, Self::Error> {
        if !buf.has_remaining() {
            return Err(Self::Error::TooShort);
        }
        let first_byte = buf.get_u8();
        match first_byte {
            0xff => {
                if buf.remaining() < 8 {
                    return Err(Self::Error::TooShort);
                }
                let x = buf.get_u64_le();
                if x < 0x100000000 {
                    Err(Self::Error::NonMinimal)
                } else {
                    Ok(Self(x))
                }
            }
            0xfe => {
                if buf.remaining() < 4 {
                    return Err(Self::Error::TooShort);
                }
                let x = buf.get_uint_le(4);
                if x < 0x10000 {
                    Err(Self::Error::NonMinimal)
                } else {
                    Ok(Self(x))
                }
            }
            0xfd => {
                if buf.remaining() < 2 {
                    return Err(Self::Error::TooShort);
                }
                let x = buf.get_uint_le(2);
                if x < 0xfd {
                    Err(Self::Error::NonMinimal)
                } else {
                    Ok(Self(x))
                }
            }
            n => Ok(VarInt(n.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        let var_int = VarInt(10);
        let capacity = var_int.encoded_len();
        let mut raw = Vec::with_capacity(capacity);
        var_int.encode(&mut raw).unwrap();
        assert_eq!(raw, vec![10u8]);

        let var_int = VarInt(0xfc);
        let mut raw = Vec::with_capacity(var_int.encoded_len());
        var_int.encode_raw(&mut raw);
        assert_eq!(raw, vec![0xfcu8]);

        let var_int = VarInt(0xfd);
        let mut raw = Vec::with_capacity(var_int.encoded_len());
        var_int.encode_raw(&mut raw);
        assert_eq!(raw, vec![0xfdu8, 0xfd, 0]);

        let var_int = VarInt(0xfff);
        let mut raw = Vec::with_capacity(var_int.encoded_len());
        var_int.encode_raw(&mut raw);
        assert_eq!(raw, vec![0xfdu8, 0xff, 0xf]);

        let var_int = VarInt(0xf0f0f0f);
        let mut raw = Vec::with_capacity(var_int.encoded_len());
        var_int.encode_raw(&mut raw);
        assert_eq!(raw, vec![0xfeu8, 0xf, 0xf, 0xf, 0xf]);

        let var_int = VarInt(0xf0f0f0f0f0e0);
        let mut raw = Vec::with_capacity(var_int.encoded_len());
        var_int.encode_raw(&mut raw);
        assert_eq!(raw, vec![0xffu8, 0xe0, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0, 0, 0]);
    }
}
