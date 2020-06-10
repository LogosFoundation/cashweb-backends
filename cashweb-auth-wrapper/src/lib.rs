use std::fmt;

use ring::digest::{digest, SHA256};
use secp256k1::{key::PublicKey, Error as SecpError, Message, Secp256k1, Signature};

include!(concat!(env!("OUT_DIR"), "/wrapper.rs"));

#[derive(Debug)]
pub enum ValidationError {
    InvalidSignature(SecpError),
    Message(SecpError),
    PublicKey(SecpError),
    Signature(SecpError),
    UnsupportedScheme,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::InvalidSignature(err) => return err.fmt(f),
            Self::Message(err) => return err.fmt(f),
            Self::PublicKey(err) => return err.fmt(f),
            Self::Signature(err) => return err.fmt(f),
            Self::UnsupportedScheme => "unsupported signature scheme",
        };
        f.write_str(printable)
    }
}

impl AuthWrapper {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let pubkey = PublicKey::from_slice(&self.pub_key).map_err(ValidationError::PublicKey)?;
        if self.scheme != 1 {
            // TODO: Support Schnorr
            return Err(ValidationError::UnsupportedScheme);
        }
        let signature =
            Signature::from_compact(&self.signature).map_err(ValidationError::Signature)?;
        let secp = Secp256k1::verification_only();
        let payload_digest = digest(&SHA256, &self.serialized_payload);
        let msg = Message::from_slice(payload_digest.as_ref()).map_err(ValidationError::Message)?;
        secp.verify(&msg, &signature, &pubkey)
            .map_err(ValidationError::InvalidSignature)?;
        Ok(())
    }
}
