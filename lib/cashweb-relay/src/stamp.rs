//! This module contains the [`Stamp`] message and methods for verifying and constructing them.

use bitcoin::{
    bip32::*,
    transaction::{DecodeError as TransactionDecodeError, Transaction},
    Decodable,
};
use ring::digest::{digest, SHA256};
use ripemd160::{Digest, Ripemd160};
use secp256k1::{
    key::{PublicKey, SecretKey as PrivateKey},
    Error as SecpError, Secp256k1,
};
use thiserror::Error;

pub use crate::{
    create_shared_key,
    models::{stamp::StampType, Stamp, StampOutpoints},
};

/// Error associated with verification of stamps.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StampError {
    /// Failed to decode a transaction.
    #[error("failed to decode transaction: {0}")]
    Decode(TransactionDecodeError),
    /// A specified stamp output doesn't exist.
    #[error("missing output")]
    MissingOutput,
    /// A specified stamp output was not a pay-to-pubkey-hash.
    #[error("output is non-p2pkh")]
    NotP2PKH,
    /// A specified stamp output contained an unexpected address.
    #[error("unexpected address: {0:?} != {1:?}")]
    UnexpectedAddress(Vec<u8>, Vec<u8>),
    /// Combination of public keys was degenerate.
    #[error("degenerate pubkey combination")]
    DegenerateCombination,
    /// Child numbers given caused an overflow.
    #[error("child number is too large")]
    ChildNumberOverflow,
    /// Unsupported stamp type.
    #[error("unsupported stamp type")]
    UnsupportedStampType,
    /// Stamp type was `None`.
    #[error("stamp type is none")]
    NoneType,
}

impl Stamp {
    /// Verify that the stamp covers the payload_digest.
    #[inline]
    pub fn verify_stamp(
        &self,
        payload_digest: &[u8; 32],
        destination_public_key: &PublicKey,
    ) -> Result<Vec<Transaction>, StampError> {
        verify_stamp(
            &self.stamp_outpoints,
            payload_digest,
            destination_public_key,
            StampType::from_i32(self.stamp_type).ok_or(StampError::UnsupportedStampType)?, // This is safe
        )
    }
}

/// Verify that the stamp covers the payload_digest.
#[inline]
pub fn verify_stamp(
    stamp_outpoints: &[StampOutpoints],
    payload_digest: &[u8; 32],
    destination_public_key: &PublicKey,
    stamp_type: StampType,
) -> Result<Vec<Transaction>, StampError> {
    if stamp_type == StampType::None {
        return Err(StampError::NoneType);
    }

    // Calculate master pubkey
    let payload_secret_key = PrivateKey::from_slice(&payload_digest.as_ref()).unwrap(); // This is safe
    let payload_public_key =
        PublicKey::from_secret_key(&Secp256k1::signing_only(), &payload_secret_key);
    let combined_key = destination_public_key
        .combine(&payload_public_key)
        .map_err(|_| StampError::DegenerateCombination)?;
    let master_pk = ExtendedPublicKey::new_master(combined_key, *payload_digest);

    // Calculate intermediate child
    let intermediate_child = master_pk
        .derive_public_path(
            &Secp256k1::verification_only(),
            &[
                ChildNumber::from_normal_index(44).unwrap(),
                ChildNumber::from_normal_index(145).unwrap(),
            ],
        )
        .unwrap(); // This is safe

    let context = Secp256k1::verification_only();
    let mut txs = Vec::with_capacity(stamp_outpoints.len());
    for (tx_num, outpoint) in stamp_outpoints.iter().enumerate() {
        let tx =
            Transaction::decode(&mut outpoint.stamp_tx.as_slice()).map_err(StampError::Decode)?;

        // Calculate intermediate child
        let child_number = ChildNumber::from_normal_index(tx_num as u32)
            .map_err(|_| StampError::ChildNumberOverflow)?;
        let tx_child = intermediate_child
            .derive_public_child(&context, child_number)
            .unwrap(); // TODO: Double check this is safe

        for (index, vout) in outpoint.vouts.iter().enumerate() {
            let output = tx
                .outputs
                .get(*vout as usize)
                .ok_or(StampError::MissingOutput)?;
            let script = &output.script;
            if !script.is_p2pkh() {
                return Err(StampError::NotP2PKH);
            }
            let pubkey_hash = &script.as_bytes()[3..23]; // This is safe as we've checked it's a p2pkh

            // Derive child key
            let child_number = ChildNumber::from_normal_index(index as u32)
                .map_err(|_| StampError::ChildNumberOverflow)?;
            let child_key = tx_child
                .derive_public_child(&context, child_number)
                .unwrap(); // TODO: Double check this is safe
            let raw_child_key = child_key.get_public_key().serialize();
            let sha256_digest = digest(&SHA256, &raw_child_key);
            let hash160_digest = Ripemd160::digest(sha256_digest.as_ref());

            // Check equivalence
            if &hash160_digest[..] != pubkey_hash {
                return Err(StampError::UnexpectedAddress(
                    hash160_digest.to_vec(),
                    pubkey_hash.to_vec(),
                ));
            }
        }

        txs.push(tx);
    }

    Ok(txs)
}

/// Error associated with creating stamp private keys.
#[derive(Debug, Error)]
pub enum StampKeyError {
    /// Degenerate addition of private keys.
    #[error(transparent)]
    Addition(SecpError),
    /// Child numbers given caused an overflow.
    #[error("child number is too large")]
    ChildNumberOverflow,
}

/// Construct stamp private keys.
///
/// The `output_profile` is an iterable collection of the number of each stamp vouts.
pub fn create_stamp_private_keys<O>(
    mut private_key: PrivateKey,
    payload_digest: &[u8; 32],
    output_profile: O,
) -> Result<Vec<Vec<PrivateKey>>, StampKeyError>
where
    for<'a> &'a O: IntoIterator<Item = &'a u32>,
{
    let context = Secp256k1::signing_only();
    private_key
        .add_assign(payload_digest.as_ref())
        .map_err(StampKeyError::Addition)?;
    let master_private_key = ExtendedPrivateKey::new_master(private_key, *payload_digest);

    // Create intermediate child
    let path_prefix = [
        ChildNumber::from_normal_index(44).unwrap(),
        ChildNumber::from_normal_index(145).unwrap(),
    ];
    let intermediate_child =
        master_private_key.derive_private_path::<_, [ChildNumber; 2]>(&context, &path_prefix);
    output_profile
        .into_iter()
        .enumerate()
        .map(|(tx_num, n_index)| {
            // Create intermediate child
            let child_number = ChildNumber::from_normal_index(tx_num as u32)
                .map_err(|_| StampKeyError::ChildNumberOverflow)?;
            let tx_child = intermediate_child.derive_private_child(&context, child_number);
            let private_keys_inner: Result<Vec<_>, _> = (0..*n_index)
                .map(|index| {
                    let child_number = ChildNumber::from_normal_index(index)
                        .map_err(|_| StampKeyError::ChildNumberOverflow)?;
                    let tx_child = tx_child.derive_private_child(&context, child_number);
                    Ok(tx_child.into_private_key())
                })
                .collect();
            private_keys_inner
        })
        .collect()
}
