mod models;
use std::{convert::TryInto, fmt};

use ring::digest::{digest, SHA256};
use secp256k1::{key::PublicKey, Error as SecpError, Message, Secp256k1, Signature};

pub use models::{auth_wrapper::SignatureScheme, AuthWrapper};

/// Represents a [`AuthWrapper`] post-parsing.
pub struct ParsedAuthWrapper {
    pub public_key: PublicKey,
    pub signature: Signature,
    pub scheme: SignatureScheme,
    pub payload: Vec<u8>,
    pub payload_digest: [u8; 32],
}

/// The error associated with validation and parsing of the [`AuthWrapper`].
#[derive(Debug)]
pub enum ParseError {
    PublicKey(SecpError),
    Signature(SecpError),
    UnsupportedScheme,
    FraudulentDigest,
    DigestAndPayloadMissing,
    UnexpectedLengthDigest,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::PublicKey(err) => return err.fmt(f),
            Self::Signature(err) => return err.fmt(f),
            Self::UnsupportedScheme => "unsupported signature scheme",
            Self::FraudulentDigest => "fraudulent digest",
            Self::DigestAndPayloadMissing => "digest and payload missing",
            Self::UnexpectedLengthDigest => "unexpected length digest",
        };
        f.write_str(printable)
    }
}

impl AuthWrapper {
    /// Parse the [`AuthWrapper`] to construct a [`ParsedAuthWrapper`].
    ///
    /// The involves deserialization of both public keys, calculation of the payload digest, and coercion of byte fields
    /// into fixed-length arrays.
    #[inline]
    pub fn parse(self) -> Result<ParsedAuthWrapper, ParseError> {
        // Parse public key
        let public_key = PublicKey::from_slice(&self.pub_key).map_err(ParseError::PublicKey)?;

        // Parse scheme
        let scheme = SignatureScheme::from_i32(self.scheme).ok_or(ParseError::UnsupportedScheme)?;

        // Parse signature
        let signature = Signature::from_compact(&self.signature).map_err(ParseError::Signature)?;

        // Construct and validate payload digest
        let payload_digest = match self.payload_digest.len() {
            0 => {
                if self.serialized_payload.is_empty() {
                    return Err(ParseError::DigestAndPayloadMissing.into());
                } else {
                    let payload_digest = digest(&SHA256, &self.serialized_payload);
                    let digest_arr: [u8; 32] = payload_digest.as_ref().try_into().unwrap();
                    digest_arr
                }
            }
            32 => {
                let payload_digest = digest(&SHA256, &self.serialized_payload);
                if *payload_digest.as_ref() != self.payload_digest[..] {
                    return Err(ParseError::FraudulentDigest.into());
                }
                let digest_arr: [u8; 32] = self.payload_digest[..].try_into().unwrap();
                digest_arr
            }
            _ => return Err(ParseError::UnexpectedLengthDigest.into()),
        };

        Ok(ParsedAuthWrapper {
            public_key,
            scheme,
            signature,
            payload_digest,
            payload: self.serialized_payload,
        })
    }
}

/// Error associated with verifying the signature of an [`AuthWrapper`].
#[derive(Debug)]
pub enum VerifyError {
    Message(SecpError),
    InvalidSignature(SecpError),
    UnsupportedScheme,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(err) => err.fmt(f),
            Self::InvalidSignature(err) => err.fmt(f),
            Self::UnsupportedScheme => f.write_str("unsupported signature scheme"),
        }
    }
}

impl ParsedAuthWrapper {
    /// Verify the signature on [`ParsedAuthWrapper`].
    #[inline]
    pub fn verify(&self) -> Result<(), VerifyError> {
        if self.scheme != SignatureScheme::Schnorr {
            // TODO: Support Schnorr
            return Err(VerifyError::UnsupportedScheme);
        }
        // Verify signature on the message
        let msg =
            Message::from_slice(self.payload_digest.as_ref()).map_err(VerifyError::Message)?;
        let secp = Secp256k1::verification_only();
        secp.verify(&msg, &self.signature, &self.public_key)
            .map_err(VerifyError::InvalidSignature)?;
        Ok(())
    }
}
