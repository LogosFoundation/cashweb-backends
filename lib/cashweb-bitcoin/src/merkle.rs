//! This module implements a naive algorithm for calculating a merkle root as
//! per the Bitcoin specification. This differs from bitcoin in that odd elements
//! use the null hash, rather than duplicating the same value twice.
use std::convert::TryInto;

use ring::digest::{digest, SHA256};

/// Poop poop
pub fn sha256d(raw: &[u8]) -> [u8; 32] {
    digest(&SHA256, digest(&SHA256, &raw).as_ref())
        .as_ref()
        .try_into()
        .unwrap()
}

/// Calculates the merkle root of a list of hashes inline
/// into the allocated slice.
///
/// In most cases, you'll want to use [lotus_merkle_root] instead.
pub fn lotus_merkle_root_inline(hashes: &mut [[u8; 32]], height: u8) -> ([u8; 32], u8) {
    let len = hashes.len();

    // Base case
    if len == 0 {
        return ([0; 32], height - 1);
    }
    if len == 1 {
        return (hashes[0], height);
    }
    // Recursion
    for idx in 0..((len + 1) / 2) {
        let idx1 = 2 * idx;
        let hash1 = hashes[idx1];
        let hash2 = if idx1 + 1 == len {
            [0; 32]
        } else {
            hashes[idx1 + 1]
        };
        hashes[idx] = sha256d(&[hash1, hash2].concat());
    }
    let half_len = len / 2 + len % 2;
    lotus_merkle_root_inline(&mut hashes[0..half_len], height + 1)
}

/// poop
pub fn lotus_merkle_root(hashes: Vec<[u8; 32]>) -> ([u8; 32], u8) {
    // Recursion
    let mut alloc = Vec::from(hashes);
    lotus_merkle_root_inline(&mut alloc, 1)
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use crate::merkle::lotus_merkle_root;

    #[test]
    fn test_merkle_calc() {
        for (raw_hashes, result, height) in test_txs_for_txid() {
            let hashes = raw_hashes
                .into_iter()
                .map(|raw_hash| hex::decode(raw_hash).unwrap().try_into().unwrap())
                .collect();
            let (calculated_root, calculated_height) = lotus_merkle_root(hashes);
            assert_eq!(calculated_height, height);
            assert_eq!(calculated_root.to_vec(), hex::decode(result).unwrap());
        }
    }

    fn test_txs_for_txid() -> Vec<(Vec<&'static str>, &'static str, u8)> {
        vec![(
            vec![
                "e23332d02cb6d57a49c06a59703baaaf455853264f1423910575abd624a4e1e4",
                "72cb37ccacdea3cb2cd67e1347b6b8b3a14d2a1b5451116355fe90198592ef92",
                "d0a8ca39679de35aaa3d6723b4c233645a697b45ec15c7666a25bb90c9e6f72e",
                "9ba388c52699bc72f8379563e6b52d927a7a9ebbe32e36aa4b9cf81529af68bf",
                "ab04de835e1a8e6b6b8b34c0ded11c7e6daa56ce601b2adc88ad28fb2d33ed24",
                "65f8620810bebbd7c3a6e240c57a3564b6813dae8cb361513756a4c9bd273de1",
                "c426e7b28e41283876a136e49cd2f95025beb460497aa57046236e26e8dc56e9",
                "a07b4c3c6ad007fb7ed67ed6801f1b2de685b6d1804eeb23593c789fb666375c",
                "4ad082fddec4ac4150b023d1c3846739b89b3060d7776dc21df34cdeb7baf9c2",
                "b23a2c3147be85c39b5527f1c9e9385ac890eef83c4ff1ff10a52642bb4c94d2",
                "21e8ea8e4eb09922ee1bfb64bbe8daeb871036a0dd5173ff35cd617b08b7c9b1",
                "1e2be67c89f7b043d5790727d683a2b0b8ebfa3a247d1614f6dea195f8b43313",
                "f1f2624fcc5938669ddfae2989d0bbd8ca0f9b6a0e8e25b5dab6f853a408f394",
                "717fe31e625b855e03e43deeb7ac874654dcbfbc3f11870e1f668e45fb8b5fe5",
                "a9065f1b0257e7fe91ec1579042173d4eb31f579a83f2302d2f168df0245e4ae",
            ],
            "5965ad54a6a855f00257c2f658fd717b98d7f2ae1ec2ac6ecfbd75e0a0feef5f",
            5,
        )]
    }
}
