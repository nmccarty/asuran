#[cfg(feature = "blake2b_simd")]
use blake2b_simd::blake2bp;
#[cfg(feature = "blake2b_simd")]
use blake2b_simd::Params;
use cfg_if::cfg_if;
#[allow(unused_imports)]
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
#[cfg(feature = "sha2")]
use sha2::Sha256;
#[cfg(feature = "sha3")]
use sha3::Sha3_256;

use crate::repository::Key;

#[cfg(not(any(
    feature = "blake2b_simd",
    feature = "sha2",
    feature = "sha3",
    feature = "blake3"
)))]
compile_error!("Asuran requires at least one HMAC algorithim to be enabled.");

#[cfg(feature = "sha2")]
type HmacSha256 = Hmac<Sha256>;
#[cfg(feature = "sha3")]
type HmacSHA3 = Hmac<Sha3_256>;

/// Tag for the HMAC algorithim used by a particular chunk
#[derive(Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum HMAC {
    SHA256,
    Blake2b,
    Blake2bp,
    Blake3,
    SHA3,
}

impl HMAC {
    /// Produces a MAC for the given data with the given key, using the
    /// algorthim specified in the tag.
    #[allow(unused_variables)]
    fn internal_mac(self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            HMAC::SHA256 => {
                cfg_if! {
                    if #[cfg(feature = "sha2")] {
                        let mut mac = HmacSha256::new_varkey(key).unwrap();
                        mac.input(data);
                        mac.result().code().to_vec()
                    } else {
                        unimplemented!("Asuran was not compiled with SHA2 support")
                    }
                }
            }
            HMAC::Blake2b => {
                cfg_if! {
                    if #[cfg(feature = "blake2b_simd")] {
                        Params::new()
                            .hash_length(64)
                            .key(key)
                            .hash(data)
                            .as_bytes()
                            .to_vec()
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE2b support")
                    }
                }
            }
            HMAC::Blake2bp => {
                cfg_if! {
                    if #[cfg(feature = "blake2b_simd")] {
                        blake2bp::Params::new()
                            .hash_length(64)
                            .key(key)
                            .hash(data)
                            .as_bytes()
                            .to_vec()
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE2b support")
                    }
                }
            }
            HMAC::Blake3 => {
                cfg_if! {
                    if #[cfg(feature = "blake3")] {
                        let mut tmp_key = [0_u8; 32];
                        tmp_key.copy_from_slice(&key[..32]);
                        blake3::keyed_hash(&tmp_key, data).as_bytes().to_vec()
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE3 support")
                    }
                }
            }
            HMAC::SHA3 => {
                cfg_if! {
                    if #[cfg(feature = "sha3")] {
                        let mut mac = HmacSHA3::new_varkey(key).unwrap();
                        mac.input(data);
                        mac.result().code().to_vec()
                    } else {
                        unimplemented!("Asuran was not compiled with SHA3 support")
                    }
                }
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

    /// Produces a MAC for the data using the algorthim specified in the tag,
    /// and verfies it against the supplied MAC
    #[allow(unused_variables)]
    pub fn verify_hmac(self, input_mac: &[u8], data: &[u8], key: &Key) -> bool {
        let key = key.hmac_key();
        match self {
            HMAC::SHA256 => {
                cfg_if! {
                    if #[cfg(feature = "sha2")] {
                        let mut mac = HmacSha256::new_varkey(key).unwrap();
                        mac.input(data);
                        let result = mac.verify(input_mac);
                        result.is_ok()
                    } else {
                        unimplemented!("Asuran was not compiled with SHA2 support")
                    }
                }
            }
            HMAC::Blake2b => {
                cfg_if! {
                    if #[cfg(feature = "blake2b_simd")] {
                        let hash = Params::new().hash_length(64).key(key).hash(data);
                        hash.eq(input_mac)
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE2b support")
                    }
                }
            }
            HMAC::Blake2bp => {
                cfg_if! {
                    if #[cfg(feature = "blake2b_simd")] {
                        let hash = blake2bp::Params::new().hash_length(64).key(key).hash(data);
                        hash.eq(input_mac)
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE2b support")
                    }
                }
            }
            HMAC::Blake3 => {
                cfg_if! {
                    if #[cfg(feature = "blake3")] {
                        let mut tmp_hash = [0_u8; 32];
                        tmp_hash.copy_from_slice(&input_mac[..32]);
                        let mut tmp_key = [0_u8; 32];
                        tmp_key.copy_from_slice(&key[..32]);
                        let input_hash = blake3::Hash::from(tmp_hash);
                        let output_hash = blake3::keyed_hash(&tmp_key, data);
                        output_hash.eq(&input_hash)
                    } else {
                        unimplemented!("Asuran was not compiled with BLAKE3 support")
                    }
                }
            }
            HMAC::SHA3 => {
                cfg_if! {
                    if #[cfg(feature = "sha3")] {
                        let mut mac = HmacSHA3::new_varkey(key).unwrap();
                        mac.input(data);
                        let result = mac.verify(input_mac);
                        result.is_ok()
                    } else {
                        unimplemented!("Asuran was not compiled with SHA3 support")
                    }
                }
            }
        }
    }
}
