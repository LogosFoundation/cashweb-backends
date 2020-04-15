use std::fmt;

use bitcoin::consensus::encode::Error as TxDeserializeError;
use bitcoin::{
    util::{
        psbt::serialize::Deserialize,
        {
            bip32::{ChainCode, ChildNumber, ExtendedPubKey},
            key,
        },
    },
    Transaction,
};
use bitcoin_hashes::{hash160, Hash};
use sha2::{Digest, Sha256};
use secp256k1::{
    key::{PublicKey, SecretKey},
    Secp256k1,
};

use crate::{
    bitcoin::{BitcoinClient, HttpConnector, NodeError},
    models::relay::messaging::StampOutpoints,
    SETTINGS,
};

#[derive(Debug)]
pub enum StampError {
    Decode(TxDeserializeError),
    MissingOutput,
    NotP2PKH,
    TxReject(NodeError),
    UnexpectedAddress,
    DegenerateCombination,
    ChildNumberOverflow,
}

impl fmt::Display for StampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::Decode(err) => return err.fmt(f),
            Self::MissingOutput => "missing output",
            Self::NotP2PKH => "non-p2pkh",
            Self::TxReject(err) => return err.fmt(f),
            Self::UnexpectedAddress => "unexpected address",
            Self::DegenerateCombination => "degenerate pubkey combination",
            Self::ChildNumberOverflow => "child number is too large",
        };
        f.write_str(printable)
    }
}

pub async fn verify_stamps(
    stamp_outpoints: &[StampOutpoints],
    serialized_payload: &[u8],
    destination_pubkey: PublicKey,
    bitcoin_client: BitcoinClient<HttpConnector>,
) -> Result<(), StampError> {
    // Calculate master pubkey
    let payload_digest = Sha256::digest(serialized_payload);
    let payload_secret_key = SecretKey::from_slice(&payload_digest).unwrap(); // TODO: Double check this is safe
    let payload_public_key =
        PublicKey::from_secret_key(&Secp256k1::signing_only(), &payload_secret_key);
    let combined_key = destination_pubkey
        .combine(&payload_public_key)
        .map_err(|_| StampError::DegenerateCombination)?;
    let public_key = key::PublicKey {
        compressed: true,
        key: combined_key,
    };
    let master_pk = ExtendedPubKey {
        public_key,
        network: SETTINGS.network.into(),
        depth: 0,
        parent_fingerprint: Default::default(),
        child_number: ChildNumber::from(0),
        chain_code: ChainCode::from(&payload_digest[..]),
    };

    // Calculate intermediate child
    let intermediate_child = master_pk
        .derive_pub(
            &Secp256k1::verification_only(),
            &[
                ChildNumber::from_normal_idx(44).unwrap(),
                ChildNumber::from_normal_idx(145).unwrap(),
            ],
        )
        .unwrap(); // This is safe

    let context = Secp256k1::verification_only();
    for (tx_num, outpoint) in stamp_outpoints.iter().enumerate() {
        let tx = Transaction::deserialize(&outpoint.stamp_tx).map_err(StampError::Decode)?;

        // Calculate intermediate child
        let child_number = ChildNumber::from_normal_idx(tx_num as u32)
            .map_err(|_| StampError::ChildNumberOverflow)?;
        let tx_child = intermediate_child.ckd_pub(&context, child_number).unwrap(); // TODO: Double check this is safe

        for vout in &outpoint.vouts {
            let output = tx
                .output
                .get(*vout as usize)
                .ok_or(StampError::MissingOutput)?;
            let script = &output.script_pubkey;
            if !script.is_p2pkh() {
                return Err(StampError::NotP2PKH);
            }
            let pubkey_hash = &script.as_bytes()[3..23]; // This is safe as we've checked it's a p2pkh

            // Derive child key
            let child_number =
                ChildNumber::from_normal_idx(*vout).map_err(|_| StampError::ChildNumberOverflow)?;
            let child_key = tx_child.ckd_pub(&context, child_number).unwrap(); // TODO: Double check this is safe
            let raw_child_key = child_key.public_key.to_bytes();
            let raw_child_hash = hash160::Hash::hash(&raw_child_key);

            // Check equivalence
            if &raw_child_hash[..] != pubkey_hash {
                return Err(StampError::UnexpectedAddress);
            }
        }

        bitcoin_client
            .send_tx(&outpoint.stamp_tx)
            .await
            .map_err(StampError::TxReject)?;
    }

    Ok(())
}
