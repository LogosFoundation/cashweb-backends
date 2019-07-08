// https://github.com/brentongunning/rust-bch/blob/master/src/address/cashaddr.rs

use super::*;

use crate::crypto::errors::CryptoError;

pub struct CashAddrCodec;

impl AddressCodec for CashAddrCodec {
    fn encode(raw: &[u8], network: Network) -> Result<String, CryptoError> {
        let version_byte = match raw.len() {
            20 => version_byte_flags::SIZE_160,
            24 => version_byte_flags::SIZE_192,
            28 => version_byte_flags::SIZE_224,
            32 => version_byte_flags::SIZE_256,
            40 => version_byte_flags::SIZE_320,
            48 => version_byte_flags::SIZE_384,
            56 => version_byte_flags::SIZE_448,
            64 => version_byte_flags::SIZE_512,
            _ => return Err(CryptoError::Encoding),
        };

        // Get prefix
        let prefix = match network {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        };

        // Generate the payload used both for calculating the checkum and the resulting address
        // It consists of a single version byte and the data to encode (pubkey hash) in 5-bit chunks
        let mut payload = Vec::with_capacity(1 + raw.len());
        payload.push(version_byte);
        payload.extend(raw);
        let payload5bit = convert_bits(&payload, 8, 5, true);

        // Generate the 40-bit checksum
        // The prefix used in the checksum calculation is the string prefix's lower 5 bits of each character.
        let checksum_input_len = prefix.len() + 1 + payload5bit.len() + 8;
        let mut checksum_input = Vec::with_capacity(checksum_input_len);
        for c in prefix.chars() {
            checksum_input.push((c as u8) & 31);
        }
        checksum_input.push(0); // 0 for prefix
        checksum_input.extend(&payload5bit);
        for _ in 0..8 {
            checksum_input.push(0); // Placeholder for checksum
        }
        let checksum = polymod(&checksum_input);

        // Start building the cashaddr string with the prefix first
        let mut cashaddr = String::new();
        cashaddr.push_str(&prefix);
        cashaddr.push(':');

        // Encode the rest of the cashaddr string (payload and checksum)
        for d in payload5bit.iter() {
            cashaddr.push(CHARSET[*d as usize] as char);
        }
        for i in (0..8).rev() {
            let c = ((checksum >> (i * 5)) & 31) as u8;
            cashaddr.push(CHARSET[c as usize] as char);
        }

        Ok(cashaddr)
    }

    fn decode(input: &str, network: Network) -> Result<Address, CryptoError> {
        // Do some sanity checks on the string
        let mut upper = false;
        let mut lower = false;
        for c in input.chars() {
            if c.is_lowercase() {
                if upper {
                    return Err(CryptoError::Decoding);
                }
                lower = true;
            } else if c.is_uppercase() {
                if lower {
                    return Err(CryptoError::Decoding);
                }
                upper = true;
            }
        }

        // Get prefix
        let prefix = match network {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        };

        // Split the prefix from the rest
        let parts: Vec<&str> = input.split(':').collect();
        if parts.len() != 2 {
            return Err(CryptoError::Decoding);
        }
        if parts[0].to_lowercase() != prefix {
            return Err(CryptoError::Decoding);
        }

        // Verify the checksum
        let mut checksum_input = Vec::with_capacity(input.len());
        for c in prefix.chars() {
            checksum_input.push((c as u8) & 31);
        }
        checksum_input.push(0); // 0 for prefix
        for c in parts[1].chars() {
            if c as u32 > 127 {
                return Err(CryptoError::Decoding);
            }
            let d = CHARSET_REV[c as usize];
            if d == -1 {
                return Err(CryptoError::Decoding);
            }
            checksum_input.push(d as u8);
        }
        let checksum = polymod(&checksum_input);
        if checksum != 0 {
            return Err(CryptoError::Decoding);
        }

        // Extract the payload squeezed between the prefix and checksum in the checksum_input
        let lower = parts[0].len() + 1;
        let upper = checksum_input.len() - 8;
        let payload = convert_bits(&checksum_input[lower..upper], 5, 8, false);

        // Verify the version byte
        let version = payload[0];
        let encoded_data = payload[1..].to_vec();

        let version_size = version & version_byte_flags::SIZE_MASK;
        if (version_size == version_byte_flags::SIZE_160 && encoded_data.len() != 20)
            || (version_size == version_byte_flags::SIZE_192 && encoded_data.len() != 24)
            || (version_size == version_byte_flags::SIZE_224 && encoded_data.len() != 28)
            || (version_size == version_byte_flags::SIZE_256 && encoded_data.len() != 32)
            || (version_size == version_byte_flags::SIZE_320 && encoded_data.len() != 40)
            || (version_size == version_byte_flags::SIZE_384 && encoded_data.len() != 48)
            || (version_size == version_byte_flags::SIZE_448 && encoded_data.len() != 56)
            || (version_size == version_byte_flags::SIZE_512 && encoded_data.len() != 64)
        {
            return Err(CryptoError::Decoding);
        }

        // Extract the address type and return
        let version_type = version & version_byte_flags::TYPE_MASK;
        if version_type == version_byte_flags::TYPE_P2PKH {
            Ok(Address {
                scheme: AddressScheme::CashAddr,
                payload: encoded_data,
            })
        } else {
            Err(CryptoError::Decoding)
        }
    }
}

// Prefixes
const MAINNET_PREFIX: &str = "bitcoincash";
const TESTNET_PREFIX: &str = "bchtest";

// Cashaddr lookup tables to convert a 5-bit number to an ascii character and back
const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const CHARSET_REV: [i8; 128] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    15, -1, 10, 17, 21, 20, 26, 30, 7, 5, -1, -1, -1, -1, -1, -1, -1, 29, -1, 24, 13, 25, 9, 8, 23,
    -1, 18, 22, 31, 27, 19, -1, 1, 0, 3, 16, 11, 28, 12, 14, 6, 4, 2, -1, -1, -1, -1, -1, -1, 29,
    -1, 24, 13, 25, 9, 8, 23, -1, 18, 22, 31, 27, 19, -1, 1, 0, 3, 16, 11, 28, 12, 14, 6, 4, 2, -1,
    -1, -1, -1, -1,
];

// Flags for the version byte
#[allow(dead_code)]
mod version_byte_flags {
    pub const TYPE_MASK: u8 = 0x78;
    pub const TYPE_P2PKH: u8 = 0x00;

    pub const SIZE_MASK: u8 = 0x07;
    pub const SIZE_160: u8 = 0x00;
    pub const SIZE_192: u8 = 0x01;
    pub const SIZE_224: u8 = 0x02;
    pub const SIZE_256: u8 = 0x03;
    pub const SIZE_320: u8 = 0x04;
    pub const SIZE_384: u8 = 0x05;
    pub const SIZE_448: u8 = 0x06;
    pub const SIZE_512: u8 = 0x07;
}

fn convert_bits(data: &[u8], inbits: u8, outbits: u8, pad: bool) -> Vec<u8> {
    assert!(inbits <= 8 && outbits <= 8);
    // num_bytes = ceil(len * 8 / 5)
    let num_bytes = (data.len() * inbits as usize + outbits as usize - 1) / outbits as usize;
    let mut ret = Vec::with_capacity(num_bytes);
    let mut acc: u16 = 0; // accumulator of bits
    let mut num: u8 = 0; // num bits in acc
    let groupmask = (1 << outbits) - 1;
    for d in data.iter() {
        // We push each input chunk into a 16-bit accumulator
        acc = (acc << inbits) | u16::from(*d);
        num += inbits;
        // Then we extract all the output groups we can
        while num > outbits {
            ret.push((acc >> (num - outbits)) as u8);
            acc &= !(groupmask << (num - outbits));
            num -= outbits;
        }
    }
    if pad {
        // If there's some bits left, pad and add it
        if num > 0 {
            ret.push((acc << (outbits - num)) as u8);
        }
    } else {
        // If there's some bits left, figure out if we need to remove padding and add it
        let padding = (data.len() * inbits as usize) % outbits as usize;
        if num as usize > padding {
            ret.push((acc >> padding) as u8);
        }
    }
    ret
}

// Calculates a 40-bit checksum given a vector of 5-bit values. The checksum is a BCH code
// over GF(32). The Bitcoin ABC implementation describes this function in detail.
fn polymod(v: &[u8]) -> u64 {
    let mut c: u64 = 1;
    for d in v.iter() {
        let c0: u8 = (c >> 35) as u8;
        c = ((c & 0x0007_ffff_ffff) << 5) ^ u64::from(*d);
        if c0 & 0x01 != 0 {
            c ^= 0x0098_f2bc_8e61;
        }
        if c0 & 0x02 != 0 {
            c ^= 0x0079_b76d_99e2;
        }
        if c0 & 0x04 != 0 {
            c ^= 0x00f3_3e5f_b3c4;
        }
        if c0 & 0x08 != 0 {
            c ^= 0x00ae_2eab_e2a8;
        }
        if c0 & 0x10 != 0 {
            c ^= 0x001e_4f43_e470;
        }
    }
    c ^ 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;

    #[test]
    fn mainnet_20byte() {
        // 20-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("F5BF48B397DAE70BE82B3CCA4793F8EB2B6CDAC9").unwrap(),
            "bitcoincash:qr6m7j9njldwwzlg9v7v53unlr4jkmx6eylep8ekg2",
        );
    }

    #[test]
    fn mainnet_24byte() {
        // 24-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("7ADBF6C17084BC86C1706827B41A56F5CA32865925E946EA").unwrap(),
            "bitcoincash:q9adhakpwzztepkpwp5z0dq62m6u5v5xtyj7j3h2ws4mr9g0",
        );
    }

    #[test]
    fn mainnet_28byte() {
        // 28-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("3A84F9CF51AAE98A3BB3A78BF16A6183790B18719126325BFC0C075B").unwrap(),
            "bitcoincash:qgagf7w02x4wnz3mkwnchut2vxphjzccwxgjvvjmlsxqwkcw59jxxuz",
        );
    }

    #[test]
    fn mainnet_32byte() {
        // 32-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("3173EF6623C6B48FFD1A3DCC0CC6489B0A07BB47A37F47CFEF4FE69DE825C060")
                .unwrap(),
            "bitcoincash:qvch8mmxy0rtfrlarg7ucrxxfzds5pamg73h7370aa87d80gyhqxq5nlegake",
        );
    }

    #[test]
    fn mainnet_40byte() {
        // 40-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("C07138323E00FA4FC122D3B85B9628EA810B3F381706385E289B0B25631197D194B5C238BEB136FB").unwrap(),
            "bitcoincash:qnq8zwpj8cq05n7pytfmskuk9r4gzzel8qtsvwz79zdskftrzxtar994cgutavfklv39gr3uvz",
        );
    }

    #[test]
    fn mainnet_48byte() {
        // 48-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("E361CA9A7F99107C17A622E047E3745D3E19CF804ED63C5C40C6BA763696B98241223D8CE62AD48D863F4CB18C930E4C").unwrap(),
            "bitcoincash:qh3krj5607v3qlqh5c3wq3lrw3wnuxw0sp8dv0zugrrt5a3kj6ucysfz8kxwv2k53krr7n933jfsunqex2w82sl",
        );
    }

    #[test]
    fn mainnet_56byte() {
        // 56-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("D9FA7C4C6EF56DC4FF423BAAE6D495DBFF663D034A72D1DC7D52CBFE7D1E6858F9D523AC0A7A5C34077638E4DD1A701BD017842789982041").unwrap(),
            "bitcoincash:qmvl5lzvdm6km38lgga64ek5jhdl7e3aqd9895wu04fvhlnare5937w4ywkq57juxsrhvw8ym5d8qx7sz7zz0zvcypqscw8jd03f",
        );
    }
    #[test]
    fn mainnet_64byte() {
        // 64-byte public key hash on mainnet
        verify(
            Network::Mainnet,
            &hex::decode("D0F346310D5513D9E01E299978624BA883E6BDA8F4C60883C10F28C2967E67EC77ECC7EEEAEAFC6DA89FAD72D11AC961E164678B868AEEEC5F2C1DA08884175B").unwrap(),
            "bitcoincash:qlg0x333p4238k0qrc5ej7rzfw5g8e4a4r6vvzyrcy8j3s5k0en7calvclhw46hudk5flttj6ydvjc0pv3nchp52amk97tqa5zygg96mtky5sv5w",
        );
    }

    fn verify(network: Network, data: &Vec<u8>, cashaddr: &str) {
        assert!(
            CashAddrCodec::encode(data, network.clone()).unwrap() == cashaddr.to_ascii_lowercase()
        );
        let decoded = CashAddrCodec::decode(cashaddr, network).unwrap();
        assert!(decoded.as_ref().to_vec() == *data);
    }
}
