//! This module contains the [`ExtendedPublicKey`] and [`ExtendedPrivateKey`] structs which allow
//! interaction with [`Hierarchical Deterministic Wallets`].
//!
//! [`Hierarchical Deterministic Wallets`]: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki

use std::convert::TryInto;

use ring::hmac::{sign as hmac, Key as HmacKey, HMAC_SHA512};
pub use secp256k1::{Error as SecpError, PublicKey, Secp256k1, SecretKey as PrivateKey};
use thiserror::Error;

/// Error associated with child number construction.
#[derive(Debug, Error)]
#[error("index error: {0}")]
pub struct IndexError(u32);

/// Public key to public key derivation can not be performed for a hardened key.
#[derive(Debug, Error)]
#[error("hardened derivation error")]
pub struct HardenedDeriveError;

/// Represents a child number.
#[derive(Clone, Copy, Debug)]
pub enum ChildNumber {
    /// A "normal" child number is within range [0, 2^31 - 1]
    Normal(u32),
    /// A "hardened" child number is within range [0, 2^31 - 1]
    Hardened(u32),
}

/// Error associated with the derivation of a [`ExtendedPublicKey`].
#[derive(Debug, Error)]
pub enum DeriveError {
    /// Public key to public key derivation can not be performed for a hardened key.
    #[error("hardened derivation error")]
    HardenedDeriveError,
    /// Invalid Tweak.
    #[error(transparent)]
    InvalidTweak(SecpError),
}

impl ChildNumber {
    /// Create a [`ChildNumber::Normal`] from an index, returns an error if the index is not within
    /// [0, 2^31).
    pub fn from_normal_index(index: u32) -> Result<Self, IndexError> {
        if index & (1 << 31) == 0 {
            Ok(ChildNumber::Normal(index))
        } else {
            Err(IndexError(index))
        }
    }

    /// Create a [`ChildNumber::Hardened`] from an index, returns an error if the index is not within
    /// [0, 2^31).
    pub fn from_hardened_index(index: u32) -> Result<Self, IndexError> {
        if index & (1 << 31) == 0 {
            Ok(ChildNumber::Hardened(index))
        } else {
            Err(IndexError(index))
        }
    }
}

impl From<u32> for ChildNumber {
    fn from(number: u32) -> Self {
        if number & (1 << 31) != 0 {
            ChildNumber::Hardened(number ^ (1 << 31))
        } else {
            ChildNumber::Normal(number)
        }
    }
}

/// A wrapper around [`PublicKey`] to allow [`Hierarchical Deterministic Wallets`] public key derivation.
///
/// [`Hierarchical Deterministic Wallets`]: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ExtendedPublicKey {
    public_key: PublicKey,
    chain_code: [u8; 32],
}

impl ExtendedPublicKey {
    /// Construct a new master public key.
    pub fn new_master(public_key: PublicKey, chain_code: [u8; 32]) -> Self {
        Self {
            public_key,
            chain_code,
        }
    }

    /// Get the underlying [`PublicKey`].
    pub fn get_public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Convert into the underlying [`PublicKey`].
    pub fn into_public_key(self) -> PublicKey {
        self.public_key
    }

    /// Convert into the [`PublicKey`] and chain code.
    pub fn as_parts(&self) -> (PublicKey, [u8; 32]) {
        (self.public_key, self.chain_code)
    }

    /// Convert into the [`PublicKey`] and chain code.
    pub fn into_parts(self) -> (PublicKey, [u8; 32]) {
        (self.public_key, self.chain_code)
    }

    /// Attempts to derive an [`ExtendedPublicKey`] from a path.
    ///
    /// The `path` must consist of an iterable collection of [`ChildNumber`]s.
    pub fn derive_public_path<C: secp256k1::Verification, P>(
        &self,
        secp: &Secp256k1<C>,
        path: &P,
    ) -> Result<ExtendedPublicKey, DeriveError>
    where
        for<'a> &'a P: IntoIterator<Item = &'a ChildNumber>,
    {
        let mut path_iter = path.into_iter();
        let mut public_key = if let Some(num) = path_iter.next() {
            self.derive_public_child(secp, *num)?
        } else {
            return Ok(*self);
        };
        for num in path_iter {
            public_key = public_key.derive_public_child(secp, *num)?
        }
        Ok(public_key)
    }

    /// Derive the child [`ExtendedPublicKey`] from a [`ChildNumber`].
    pub fn derive_public_child<C: secp256k1::Verification>(
        &self,
        secp: &Secp256k1<C>,
        child_number: ChildNumber,
    ) -> Result<ExtendedPublicKey, DeriveError> {
        let index = match child_number {
            ChildNumber::Hardened(_) => return Err(DeriveError::HardenedDeriveError),
            ChildNumber::Normal(index) => index,
        };
        let key = HmacKey::new(HMAC_SHA512, &self.chain_code);
        let data = [&self.public_key.serialize()[..], &index.to_be_bytes()[..]].concat();
        let hmac_result = hmac(&key, &data);

        let private_key = PrivateKey::from_slice(&hmac_result.as_ref()[..32]).unwrap(); // This is safe
        let chain_code: [u8; 32] = hmac_result.as_ref()[32..].try_into().unwrap(); // This is safe
        let mut public_key = self.public_key;
        public_key
            .add_exp_assign(secp, &private_key[..])
            .map_err(DeriveError::InvalidTweak)?;

        Ok(ExtendedPublicKey {
            public_key,
            chain_code,
        })
    }
}

/// A wrapper around [`PrivateKey`] to allow [`Hierarchical Deterministic Wallets`] public key derivation.
///
/// [`Hierarchical Deterministic Wallets`]: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ExtendedPrivateKey {
    private_key: PrivateKey,
    chain_code: [u8; 32],
}

impl ExtendedPrivateKey {
    /// Construct a new master private key.
    pub fn new_master(private_key: PrivateKey, chain_code: [u8; 32]) -> Self {
        ExtendedPrivateKey {
            private_key,
            chain_code,
        }
    }

    /// Get the underlying [`PrivateKey`].
    pub fn get_private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    /// Convert into the underlying [`PrivateKey`].
    pub fn into_private_key(self) -> PrivateKey {
        self.private_key
    }

    /// Convert into the [`PrivateKey`] and chain code.
    pub fn into_parts(self) -> (PrivateKey, [u8; 32]) {
        (self.private_key, self.chain_code)
    }

    /// Derive an child [`ExtendedPrivateKey`] from a path.
    ///
    /// The `path` must consist of an iterable collection of [`ChildNumber`]s.
    pub fn derive_private_path<C: secp256k1::Signing, P>(
        &self,
        secp: &Secp256k1<C>,
        path: &P,
    ) -> ExtendedPrivateKey
    where
        for<'a> &'a P: IntoIterator<Item = &'a ChildNumber>,
    {
        let mut path_iter = path.into_iter();
        let mut private_key = if let Some(num) = path_iter.next() {
            self.derive_private_child(secp, *num)
        } else {
            return *self;
        };
        for num in path_iter {
            private_key = private_key.derive_private_child(secp, *num);
        }
        private_key
    }

    /// Derive child [`ExtendedPrivateKey`].
    pub fn derive_private_child<C: secp256k1::Signing>(
        &self,
        secp: &Secp256k1<C>,
        child_number: ChildNumber,
    ) -> ExtendedPrivateKey {
        // Calculate HMAC
        let key = HmacKey::new(HMAC_SHA512, &self.chain_code);
        let hmac_result = match child_number {
            ChildNumber::Normal(index) => {
                // Non-hardened key: compute public data and use that
                let raw_public_key =
                    PublicKey::from_secret_key(secp, &self.private_key).serialize();
                let data = [&raw_public_key[..], &index.to_be_bytes()].concat();
                hmac(&key, &data)
            }
            ChildNumber::Hardened(index) => {
                // Hardened key: use only secret data to prevent public derivation
                let data = [&[0], &self.private_key[..], &index.to_be_bytes()].concat();
                hmac(&key, &data)
            }
        };

        // Construct new private key
        let mut private_key = PrivateKey::from_slice(&hmac_result.as_ref()[..32]).unwrap(); // This is safe
        private_key.add_assign(&self.private_key[..]).unwrap(); // This is safe

        // Construct new extended private key
        let chain_code = hmac_result.as_ref()[32..].try_into().unwrap(); // This is safe
        ExtendedPrivateKey {
            private_key,
            chain_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::thread_rng;
    use secp256k1::Secp256k1;

    #[test]
    fn child_derivation() {
        let secp = Secp256k1::new();
        let mut rng = thread_rng();
        let private_key = PrivateKey::new(&mut rng);
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let hd_private_key = ExtendedPrivateKey::new_master(private_key, [0; 32]);
        let hd_public_key = ExtendedPublicKey::new_master(public_key, [0; 32]);

        let new_hd_private_key =
            hd_private_key.derive_private_child(&secp, ChildNumber::Normal(32));
        let new_hd_public_key = hd_public_key
            .derive_public_child(&secp, ChildNumber::Normal(32))
            .unwrap();

        assert_eq!(
            PublicKey::from_secret_key(&secp, &new_hd_private_key.into_private_key()),
            new_hd_public_key.into_public_key()
        );
    }

    #[test]
    fn child_derivation_normal_path_a() {
        let secp = Secp256k1::new();
        let mut rng = thread_rng();

        let path = [ChildNumber::Normal(32), ChildNumber::Normal(4)];

        let private_key = PrivateKey::new(&mut rng);
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let hd_private_key = ExtendedPrivateKey::new_master(private_key, [0; 32]);
        let hd_public_key = ExtendedPublicKey::new_master(public_key, [0; 32]);

        let new_hd_private_key = hd_private_key.derive_private_path(&secp, &path);
        let new_hd_public_key = hd_public_key.derive_public_path(&secp, &path).unwrap();

        assert_eq!(
            PublicKey::from_secret_key(&secp, &new_hd_private_key.into_private_key()),
            new_hd_public_key.into_public_key()
        );
    }

    #[test]
    fn child_derivation_normal_path_b() {
        let secp = Secp256k1::new();
        let mut rng = thread_rng();

        let path = [
            ChildNumber::Normal(32),
            ChildNumber::Normal(4),
            ChildNumber::Normal(54),
        ];

        let private_key = PrivateKey::new(&mut rng);
        let hd_private_key = ExtendedPrivateKey::new_master(private_key, [0; 32]);

        let hd_private_key_a = hd_private_key.derive_private_path(&secp, &path);
        let hd_private_key_b = hd_private_key
            .derive_private_child(&secp, path[0])
            .derive_private_child(&secp, path[1])
            .derive_private_child(&secp, path[2]);

        assert_eq!(hd_private_key_a, hd_private_key_b);
    }
}
