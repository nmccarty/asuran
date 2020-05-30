use stream_cipher::generic_array::GenericArray;
use stream_cipher::SyncStreamCipher;
use zeroize::Zeroize;

use std::cmp;

/// This function performs AES256CTR encryption, unconditionally using the safe/soft implementation
/// of aes
fn aes_soft_256_ctr(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    use aes_soft::Aes256;
    use block_cipher_trait::BlockCipher;
    use ctr::Ctr128;

    let mut proper_key: [u8; 32] = [0; 32];
    proper_key[..cmp::min(key.len(), 32)].clone_from_slice(&key[..cmp::min(key.len(), 32)]);
    let aes = Aes256::new_varkey(key).expect(
        "Attemped to instantiate an AES cipher with an invalid length key in internal method.",
    );
    let iv = GenericArray::from_slice(iv);
    let mut encryptor: Ctr128<Aes256> = Ctr128::from_cipher(aes, iv);
    let mut final_result = data.to_vec();
    encryptor.apply_keystream(&mut final_result);

    proper_key.zeroize();
    final_result
}

/// This function performs AES256CTR, unconditionally using aesni. Will blow up if called on a
/// machine without AESNI instructions
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "aes")]
#[target_feature(enable = "sse3")]
unsafe fn aesni_256_ctr(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    use aesni::Aes256Ctr;
    use stream_cipher::NewStreamCipher;

    let mut proper_key: [u8; 32] = [0; 32];
    proper_key[..cmp::min(key.len(), 32)].clone_from_slice(&key[..cmp::min(key.len(), 32)]);
    let key = GenericArray::from_slice(&key);
    let iv = GenericArray::from_slice(&iv[..]);
    let mut encryptor = Aes256Ctr::new(&key, &iv);
    let mut final_result = data.to_vec();
    encryptor.apply_keystream(&mut final_result);

    proper_key.zeroize();
    final_result
}

/// This function performs AES256CTR using the fastest available implementation supported on the current machine, using runtime feature detection
pub fn aes_256_ctr(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    cfg_if::cfg_if! {
        if #[cfg(any(target_arch = "x86", target_arch = "x86_64"))] {
            use std::is_x86_feature_detected;
            // Check for aes acceleration support
            if is_x86_feature_detected!("aes") && is_x86_feature_detected!("ssse3") {
                // safe because we just verified aes and sse3 support
                unsafe {aesni_256_ctr(data,key,iv)}
            } else {
                aes_soft_256_ctr(data, key, iv)
            }
        } else {
            // We don't support hardware acceleration on this architecture, fall back to software
            // aes
            aes_soft_256_ctr(data, key, iv)
        }
    }
}
