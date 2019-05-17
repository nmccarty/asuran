use blake2::Blake2b;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

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
    /// Produces a MAC for the data using the algorthim specified in the tag,
    /// and verfies it against the supplied MAC
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
