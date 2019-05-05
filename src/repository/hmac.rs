use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// HMAC Algorithim
pub enum HMAC {
    SHA256,
}

impl HMAC {
    /// Produces a MAC for the given data with the given key
    pub fn mac(&self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            HMAC::SHA256 => {
                let mut mac = HmacSha256::new_varkey(key).unwrap();
                mac.input(data);
                mac.result().code().to_vec()
            }
        }
    }
    /// Verifies the data given the data, a MAC, and a key
    pub fn verify(&self, input_mac: &[u8], data: &[u8], key: &[u8]) -> bool {
        let mut mac = HmacSha256::new_varkey(key).unwrap();
        mac.input(data);
        let result = mac.verify(input_mac);
        result.is_ok()
    }
}
