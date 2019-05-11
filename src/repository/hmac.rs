use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use blake2::Blake2b;

#[cfg(feature = "profile")]
use flamer::*;

type HmacSha256 = Hmac<Sha256>;

/// HMAC Algorithim
#[derive(Deserialize, Serialize, Copy, Clone)]
pub enum HMAC {
    SHA256,
    Blake2b,
}

impl HMAC {
    #[cfg_attr(feature = "profile", flame)]
    /// Produces a MAC for the given data with the given key
    pub fn mac(self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            HMAC::SHA256 => {
                let mut mac = HmacSha256::new_varkey(key).unwrap();
                mac.input(data);
                mac.result().code().to_vec()
            }
            HMAC::Blake2b => {
                let mut mac = Blake2b::new_varkey(key).unwrap();
                mac.input(data);
                mac.result().code().to_vec()
            }
        }
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Verifies the data given the data, a MAC, and a key
    pub fn verify(self, input_mac: &[u8], data: &[u8], key: &[u8]) -> bool {
        match self {
            HMAC::SHA256 => {
                let mut mac = HmacSha256::new_varkey(key).unwrap();
                mac.input(data);
                let result = mac.verify(input_mac);
                result.is_ok()
            }
            HMAC::Blake2b => {
                let mut mac = Blake2b::new_varkey(key).unwrap();
                mac.input(data);
                let result = mac.verify(input_mac);
                result.is_ok()
            }
        }
    }
}
