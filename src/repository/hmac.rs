use blake2b_simd::Params;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::repository::Key;

#[cfg(feature = "profile")]
use flamer::*;

type HmacSha256 = Hmac<Sha256>;

/// Tag for the HMAC algorithim used by a particular chunk
#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
pub enum HMAC {
    SHA256,
    Blake2b,
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
        }
    }
}
