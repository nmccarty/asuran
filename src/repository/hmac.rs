use blake2b_simd::blake2bp;
use blake2b_simd::Params;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::repository::Key;

#[cfg(feature = "profile")]
use flamer::*;

type HmacSha256 = Hmac<Sha256>;

/// Tag for the HMAC algorithim used by a particular chunk
#[derive(Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum HMAC {
    SHA256,
    Blake2b,
    Blake2bp,
    Blake3,
}

impl HMAC {
    #[cfg_attr(feature = "profile", flame)]
    /// Produces a MAC for the given data with the given key, using the
    /// algorthim specified in the tag.
    fn internal_mac(self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            HMAC::SHA256 => {
                let mut mac = HmacSha256::new_varkey(key).unwrap();
                mac.input(data);
                mac.result().code().to_vec()
            }
            HMAC::Blake2b => Params::new()
                .hash_length(64)
                .key(key)
                .hash(data)
                .as_bytes()
                .to_vec(),
            HMAC::Blake2bp => blake2bp::Params::new()
                .hash_length(64)
                .key(key)
                .hash(data)
                .as_bytes()
                .to_vec(),
            HMAC::Blake3 => {
                let mut tmp_key = [0_u8; 32];
                tmp_key.copy_from_slice(&key[..32]);
                blake3::keyed_hash(&tmp_key, data).as_bytes().to_vec()
            }
        }
    }

    /// Produces a mac for the given data using the HMAC key
    pub fn mac(self, data: &[u8], key: &Key) -> Vec<u8> {
        let key = key.hmac_key();
        self.internal_mac(data, key)
    }

    /// Produces a mac for the given data using the ID key
    pub fn id(self, data: &[u8], key: &Key) -> Vec<u8> {
        let key = key.id_key();
        self.internal_mac(data, key)
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Produces a MAC for the data using the algorthim specified in the tag,
    /// and verfies it against the supplied MAC
    pub fn verify_hmac(self, input_mac: &[u8], data: &[u8], key: &Key) -> bool {
        let key = key.hmac_key();
        match self {
            HMAC::SHA256 => {
                let mut mac = HmacSha256::new_varkey(key).unwrap();
                mac.input(data);
                let result = mac.verify(input_mac);
                result.is_ok()
            }
            HMAC::Blake2b => {
                let hash = Params::new().hash_length(64).key(key).hash(data);
                hash.eq(input_mac)
            }
            HMAC::Blake2bp => {
                let hash = blake2bp::Params::new().hash_length(64).key(key).hash(data);
                hash.eq(input_mac)
            }
            HMAC::Blake3 => {
                let mut tmp_hash = [0_u8; 32];
                tmp_hash.copy_from_slice(&input_mac[..32]);
                let mut tmp_key = [0_u8; 32];
                tmp_key.copy_from_slice(&key[..32]);
                let input_hash = blake3::Hash::from(tmp_hash);
                let output_hash = blake3::keyed_hash(&tmp_key, data);
                output_hash.eq(&input_hash)
            }
        }
    }
}
