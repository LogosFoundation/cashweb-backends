#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb-relay` is a library providing serialization/deserialization, encryption/decryption/verification of
//! structures in the [`Relay Protocol`].
//!
//! [`Relay Protocol`]: https://github.com/cashweb/specifications/blob/master/authorization-wrapper/specification.mediawiki

#[allow(unreachable_pub, missing_docs)]
mod models;
pub mod stamp;

use std::convert::TryInto;

use aes::{
    cipher::generic_array::{typenum::U16, GenericArray},
    Aes128,
};
use bitcoin::transaction::Transaction;
use block_modes::{block_padding::Pkcs7, BlockMode, BlockModeError, Cbc};
use prost::{DecodeError as MessageDecodeError, Message as _};
use ring::{
    digest::{digest, SHA256},
    hmac::{sign, Key as HmacKey, HMAC_SHA256},
};
use secp256k1::{key::PublicKey, Error as SecpError, Secp256k1};
use thiserror::Error;

pub mod secp {
    //! This module contains re-exported `secp256k1` primitives.

    pub use secp256k1::{
        key::{PublicKey, SecretKey as PrivateKey},
        Error as SecpError, Secp256k1,
    };
}

pub use crate::models::{
    message::EncryptionScheme, Message, MessagePage, MessageSet, Payload, PayloadPage, Profile,
};
use stamp::*;

type Aes128Cbc = Cbc<Aes128, Pkcs7>;

/// Represents a [Message](struct.Message.html) post-parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMessage {
    /// The source public key.
    pub source_public_key: PublicKey,
    /// The destinations public key.
    pub destination_public_key: PublicKey,
    /// Maleable server time.
    pub received_time: i64,
    /// The SHA-256 digest of the payload.
    pub payload_digest: [u8; 32],
    /// The stamp attached to the message.
    pub stamp: Stamp,
    /// The encryption scheme used on the serialized `Payload` to produce the `payload` field.
    pub scheme: EncryptionScheme,
    /// The `salt` is used to salt both the `payload_hmac` and the encryption key.
    pub salt: Vec<u8>,
    /// The HMAC of the `payload`, specifically `HMAC(HMAC(sdG, salt), payload_digest)`
    pub payload_hmac: [u8; 32],
    /// The size, in bytes, of the `payload`.
    pub payload_size: u64,
    /// The encrypted `payload`.
    pub payload: Vec<u8>,
}

impl ParsedMessage {
    /// Convert [`ParsedMessage`] into a [`Message`].
    pub fn into_message(self) -> Message {
        Message {
            source_public_key: self.source_public_key.serialize().to_vec(),
            destination_public_key: self.destination_public_key.serialize().to_vec(),
            received_time: self.received_time,
            payload_digest: self.payload_digest.to_vec(),
            stamp: Some(self.stamp),
            scheme: self.scheme.into(),
            salt: self.salt,
            payload_hmac: self.payload_hmac.to_vec(),
            payload_size: self.payload_size,
            payload: self.payload,
        }
    }
}

/// Error associated with [`Message`] parsing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// Unable to calculate the [`Message::payload_digest`].
    #[error(transparent)]
    Digest(DigestError),
    /// Unable to parse the [`Message::source_public_key`].
    #[error("source public key: {0}")]
    SourcePublicKey(SecpError),
    /// Unable to parse the [`Message::destination_public_key`].
    #[error("destination public key: {0}")]
    DestinationPublicKey(SecpError),
    /// Stamp information missing.
    #[error("missing stamp")]
    MissingStamp,
    /// Unsupported stamp type given.
    #[error("unsupported stamp type")]
    UnsupportedStampType,
    /// Payload HMAC was an unexpected length.
    #[error("unexpected length payload hmac")]
    UnexpectedLengthPayloadHmac,
}

/// Error associated with getting the [`Message::payload_digest`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DigestError {
    /// Both the digest and payload are missing.
    #[error("digest and payload missing")]
    DigestAndPayloadMissing,
    /// Fraudulent digest.
    #[error("fraudulent digest")]
    FraudulentDigest,
    /// Digest was an unexpected length.
    #[error("unexpected length digest")]
    UnexpectedLengthDigest,
}

impl Message {
    /// Get the SHA-256 digest of the `payload`, if `payload_digest` is missing then calculate it.
    #[inline]
    pub fn digest(&self) -> Result<[u8; 32], DigestError> {
        // Calculate payload digest
        let payload_digest: [u8; 32] = match self.payload_digest.len() {
            0 => {
                // Check payload is not missing too
                if self.payload.is_empty() {
                    return Err(DigestError::DigestAndPayloadMissing);
                }

                // Calculate digest
                let payload_digest: [u8; 32] =
                    digest(&SHA256, &self.payload).as_ref().try_into().unwrap(); // This is safe

                payload_digest
            }
            32 => {
                // Check digest is correct when payload is not missing
                if !self.payload.is_empty() {
                    // Calculate digest
                    let payload_digest: [u8; 32] =
                        digest(&SHA256, &self.payload).as_ref().try_into().unwrap(); // This is safe

                    if payload_digest[..] != self.payload_digest[..] {
                        return Err(DigestError::FraudulentDigest);
                    }
                    payload_digest
                } else {
                    let slice = &self.payload_digest[..];
                    slice.try_into().unwrap()
                }
            }
            _ => return Err(DigestError::UnexpectedLengthDigest),
        };

        Ok(payload_digest)
    }

    /// Parse the [Message](struct.Message.html) to construct a [ParsedMessage](struct.ParsedMessage.html).
    ///
    /// The involves deserialization of both public keys, calculation of the payload digest, and coercion of byte fields into arrays.
    #[inline]
    pub fn parse(self) -> Result<ParsedMessage, ParseError> {
        // Decode public keys
        let source_public_key =
            PublicKey::from_slice(&self.source_public_key).map_err(ParseError::SourcePublicKey)?;
        let destination_public_key = PublicKey::from_slice(&self.destination_public_key)
            .map_err(ParseError::DestinationPublicKey)?;

        // Calculate payload digest
        let payload_digest = self.digest().map_err(ParseError::Digest)?;

        // Parse stamp data
        let stamp = self.stamp.ok_or(ParseError::MissingStamp)?;

        // Parse scheme
        let scheme =
            EncryptionScheme::from_i32(self.scheme).ok_or(ParseError::UnsupportedStampType)?;

        // Parse payload_hmac
        let payload_hmac: [u8; 32] = self.payload_hmac[..]
            .try_into()
            .map_err(|_| ParseError::UnexpectedLengthPayloadHmac)?;

        Ok(ParsedMessage {
            source_public_key,
            destination_public_key,
            received_time: self.received_time,
            payload_digest,
            stamp,
            scheme,
            salt: self.salt,
            payload_hmac,
            payload_size: self.payload_size,
            payload: self.payload,
        })
    }
}

/// Create the merged key from the source public key and destination private key.
#[inline]
pub fn create_merged_key(
    source_public_key: PublicKey,
    private_key: &[u8],
) -> Result<PublicKey, SecpError> {
    // Create merged key
    let mut merged_key = source_public_key;
    merged_key.mul_assign(&Secp256k1::verification_only(), private_key)?;
    Ok(merged_key)
}

/// Create shared key.
#[inline]
pub fn create_shared_key(
    source_public_key: PublicKey,
    private_key: &[u8],
    salt: &[u8],
) -> Result<[u8; 32], SecpError> {
    // Create merged key
    let merged_key = create_merged_key(source_public_key, private_key)?;
    let raw_merged_key = merged_key.serialize();

    let key = HmacKey::new(HMAC_SHA256, &raw_merged_key);
    let digest = sign(&key, salt);
    let shared_key: [u8; 32] = digest.as_ref().try_into().unwrap(); // This is safe
    Ok(shared_key)
}

/// Message authentication failed, the calculated HMAC did not match the one given.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("invalid hmac")]
pub struct InvalidHmac;

/// Authenticate the [`Payload`] and return the merged key.
#[inline]
pub fn authenticate(
    shared_key: &[u8],
    payload_digest: &[u8],
    payload_hmac: &[u8],
) -> Result<(), InvalidHmac> {
    // HMAC shared_key with payload_digest
    let shared_key = HmacKey::new(HMAC_SHA256, shared_key);
    let payload_hmac_expected = sign(&shared_key, payload_digest);

    // Check equality
    if payload_hmac_expected.as_ref() != payload_hmac {
        return Err(InvalidHmac);
    }
    Ok(())
}

/// The result of [`open`] or [`open_in_place`].
///
/// [`open`]: ParsedMessage::open
/// [`open_in_place`]: ParsedMessage::open_in_place
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    /// Decoded transactions
    pub txs: Vec<Transaction>,
    /// Decrypted and deserialized payload.
    pub payload: Payload,
}

/// Error associated with [`open`] or [`open_in_place`].
///
/// [`open`]: ParsedMessage::open
/// [`open_in_place`]: ParsedMessage::open_in_place
#[derive(Debug, Clone, Error)]
pub enum OpenError {
    /// Invalid stamp.
    #[error("stamp errror: {0}")]
    Stamp(StampError),
    /// Failed to construct shared key.
    #[error("shared key: {0}")]
    SharedKey(SecpError),
    /// Failed authentication.
    #[error("authentication failed")]
    Authentication,
    /// Failed to decode the plaintext [`Payload`].
    #[error("payload decoding failure: {0}")]
    Payload(MessageDecodeError),
    /// Failed to decrypt the ciphertext [`Payload`].
    #[error("decryption failure: {0}")]
    Decrypt(BlockModeError),
}

impl ParsedMessage {
    /// Calculate the merged key from the destination private key.
    #[inline]
    pub fn create_merged_key(&self, private_key: &[u8]) -> Result<PublicKey, SecpError> {
        create_merged_key(self.source_public_key, private_key)
    }

    /// Create the shared key from the destination private key, a salt, and a list of HKDF info.
    #[inline]
    pub fn create_shared_key(
        &self,
        private_key: &[u8],
        salt: &[u8],
    ) -> Result<[u8; 32], SecpError> {
        create_shared_key(self.source_public_key, private_key, salt)
    }

    /// Authenticate the HMAC payload and return the merged key.
    #[inline]
    pub fn authenticate(&self, shared_key: &[u8; 32]) -> Result<(), InvalidHmac> {
        authenticate(shared_key, &self.payload_digest, &self.salt)?;

        Ok(())
    }

    /// Verify the stamp on the message and return the decoded transactions.
    #[inline]
    pub fn verify_stamp(&self) -> Result<Vec<Transaction>, StampError> {
        self.stamp
            .verify_stamp(&self.payload_digest, &self.destination_public_key)
    }

    /// Verify the stamp, authenticate the HMAC payload, and then decrypt and decode the payload.
    ///
    /// This is done in-place, replacing the encrypted `payload` field with the plain text.
    #[inline]
    pub fn open_in_place(&mut self, private_key: &[u8]) -> Result<Opened, OpenError> {
        // Verify stamp
        let txs = self.verify_stamp().map_err(OpenError::Stamp)?;

        // Create shared key
        let shared_key = self
            .create_shared_key(private_key, &self.salt)
            .map_err(OpenError::SharedKey)?;

        // Authenticate HMAC payload
        self.authenticate(&shared_key)
            .map_err(|_| OpenError::Authentication)?;

        // Decrypt
        let mut raw_payload = &mut self.payload;
        let (key, iv) = shared_key.split_at(16);
        let key = GenericArray::<u8, U16>::from_slice(&key);
        let iv = GenericArray::<u8, U16>::from_slice(&iv);
        let cipher = Aes128Cbc::new_var(&key, &iv).unwrap(); // This is safe
        cipher
            .decrypt(&mut raw_payload)
            .map_err(OpenError::Decrypt)?;

        // Decode
        let payload = Payload::decode(&mut raw_payload.as_slice()).map_err(OpenError::Payload)?;

        Ok(Opened { txs, payload })
    }

    /// Verify the stamp, authenticate the HMAC payload, and then decrypt and decode the payload.
    #[inline]
    pub fn open(&self, private_key: &[u8]) -> Result<Opened, OpenError> {
        // Verify stamp
        let txs = self.verify_stamp().map_err(OpenError::Stamp)?;

        // Create shared key
        let shared_key = self
            .create_shared_key(private_key, &self.salt)
            .map_err(OpenError::SharedKey)?;

        // Authenticate HMAC payload
        self.authenticate(&shared_key)
            .map_err(|_| OpenError::Authentication)?;

        // Decrypt
        let raw_payload = &self.payload;
        let (key, iv) = shared_key.as_ref().split_at(16);
        let key = GenericArray::<u8, U16>::from_slice(&key);
        let iv = GenericArray::<u8, U16>::from_slice(&iv);
        let cipher = Aes128Cbc::new_var(&key, &iv).unwrap(); // This is safe
        cipher
            .decrypt_vec(raw_payload)
            .map_err(OpenError::Decrypt)?;

        // Decode
        let payload = Payload::decode(&mut raw_payload.as_slice()).map_err(OpenError::Payload)?;

        Ok(Opened { txs, payload })
    }
}

impl MessagePage {
    /// Convert the [MessagePage](struct.MessagePage.html) into a [PayloadPage](struct.PayloadPage.html).
    pub fn into_payload_page(self) -> PayloadPage {
        self.into()
    }
}

impl Into<PayloadPage> for MessagePage {
    fn into(self) -> PayloadPage {
        let payloads: Vec<Vec<u8>> = self
            .messages
            .into_iter()
            .map(|message| message.payload)
            .collect();
        PayloadPage {
            start_time: self.start_time,
            end_time: self.end_time,
            start_digest: self.start_digest,
            end_digest: self.end_digest,
            payloads,
        }
    }
}

/// Encrypt a payload using a shared key.
///
/// Typically the shared key is `HMAC(sdG, salt)` created using the [`create_shared_key`] method.
pub fn encrypt_payload(shared_key: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let (key, iv) = shared_key.as_ref().split_at(16);
    let key = GenericArray::<u8, U16>::from_slice(&key);
    let iv = GenericArray::<u8, U16>::from_slice(&iv);
    let cipher = Aes128Cbc::new_var(&key, &iv).unwrap(); // This is safe
    cipher.encrypt_vec(plaintext)
}

/// Encrypt a payload, in place, using a shared key.
///
/// Typically the shared key is `HMAC(sdG, salt)` created using the [`create_shared_key`] method.
pub fn encrypt_payload_in_place(shared_key: &[u8], payload: &mut [u8]) {
    let (key, iv) = shared_key.as_ref().split_at(16);
    let key = GenericArray::<u8, U16>::from_slice(&key);
    let iv = GenericArray::<u8, U16>::from_slice(&iv);
    let cipher = Aes128Cbc::new_var(&key, &iv).unwrap(); // This is safe
    cipher.encrypt(payload, 0).unwrap(); // TODO: Double check this is safe
}
