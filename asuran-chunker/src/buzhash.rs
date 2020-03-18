use super::{Chunker, ChunkerError};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::collections::VecDeque;
use std::io::Read;

/// Settings for a `BuzHash` `Chunker`
///
/// Uses a randomized lookup table derived from a nonce provided by the repository
/// key material, to help provide resistance against a chunk size based
/// fingerprinting attack, for users who are concerned about such a thing.
///
/// This is a very tenuous mitigation for such an attack, borrowed from
/// [borg](https://borgbackup.readthedocs.io/en/stable/internals/security.html#fingerprinting),
/// and is, fundamentally, a temporary work around. The "correct" solution is to
/// implement a better repository structure, that does not leak chunk sizes.
#[derive(Clone, Copy)]
pub struct BuzHash {
    table: [u64; 256],
    window_size: u32,
    mask: u64,
    min_size: usize,
    max_size: usize,
}

impl BuzHash {
    pub fn new(nonce: u64, window_size: u32, mask_bits: u32) -> BuzHash {
        let mut table = [0_u64; 256];
        let mut rng = ChaCha20Rng::seed_from_u64(nonce);
        let random_value: u64 = rng.gen();
        for (index, item) in table.iter_mut().enumerate() {
            *item = TABLE[index] ^ random_value;
        }
        BuzHash {
            table,
            window_size,
            min_size: 2_usize.pow(mask_bits - 2),
            max_size: 2_usize.pow(mask_bits + 2),
            mask: 2_u64.pow(mask_bits) - 1,
        }
    }
}

impl BuzHash {
    pub fn with_default(nonce: u64) -> BuzHash {
        Self::new(nonce, 4095, 21)
    }

    #[cfg(test)]
    fn with_default_testing(nonce: u64) -> BuzHash {
        Self::new(nonce, 4095, 14)
    }
}

impl Chunker for BuzHash {
    type Chunks = BuzHashChunker;
    fn chunk_boxed(&self, read: Box<dyn Read + Send + 'static>) -> Self::Chunks {
        BuzHashChunker {
            settings: *self,
            read,
            buffer: VecDeque::new(),
            hash_buffer: VecDeque::new(),
            count: 0,
            hash: 0,
            eof: false,
        }
    }
}

pub struct BuzHashChunker {
    /// Settings for this `Chunker`
    settings: BuzHash,
    /// The reader this `Chunker` is slicing
    read: Box<dyn Read + Send + 'static>,
    /// The in memory buffer used for reading and popping bytes
    buffer: VecDeque<u8>,
    /// The buffer used by the rolling hash
    hash_buffer: VecDeque<u8>,
    /// Bytes in the hash buffer
    count: u32,
    /// The current hash value
    hash: u64,
    eof: bool,
}

impl BuzHashChunker {
    /// Hashes one byte and returns the new hash value
    fn hash_byte(&mut self, byte: u8) -> u64 {
        // determine if removal is needed
        if self.count >= self.settings.window_size {
            let hash = self.hash.rotate_left(1);
            let head = self.hash_buffer.pop_front().unwrap();
            let head = self.settings.table[head as usize].rotate_left(self.settings.window_size);
            let tail = self.settings.table[byte as usize];
            self.hash = hash ^ head ^ tail;
        } else {
            self.count += 1;
            let hash = self.hash.rotate_left(1);
            let tail = self.settings.table[byte as usize];
            self.hash = hash ^ tail;
        }

        self.hash_buffer.push_back(byte);
        self.hash
    }

    /// Reads up to `max_size` bytes into the internal buffer
    fn top_off_buffer(&mut self) -> Result<(), ChunkerError> {
        // Check to see if we need topping off
        if self.buffer.len() >= self.settings.max_size {
            Ok(())
        } else {
            // Create a temporary buffer that allows for the number of bytes needed to fill the
            // buffer. The result of this should not underflow as the buffer should never exceed
            // max_size in size.
            let tmp_buffer_size = self.settings.max_size - self.buffer.len();
            let mut tmp_buffer: Vec<u8> = vec![0_u8; tmp_buffer_size];
            let mut bytes_read = 0;
            while !self.eof && bytes_read < tmp_buffer_size {
                let local_bytes_read = self.read.read(&mut tmp_buffer[bytes_read..])?;
                // Update the length
                bytes_read += local_bytes_read;
                // If the number of bytes read was zero, set the eof flag
                if local_bytes_read == 0 {
                    self.eof = true;
                }
            }
            // Push the elements we read from the local buffer to the actual buffer
            for byte in tmp_buffer.iter().take(bytes_read) {
                self.buffer.push_back(*byte);
            }
            Ok(())
        }
    }

    /// Attempts to get another slice from the reader
    fn next_chunk(&mut self) -> Result<Vec<u8>, ChunkerError> {
        // Attempt to top off the buffer, this will ensure that we have either hit EoF or that there
        // are at least max_size bytes in the buffer
        self.top_off_buffer()?;
        // Check to see if there are any bytes in the buffer first. Since we just attempted to top
        // off the buffer, if we are still empty, that is because there are no more bytes to read.
        if self.buffer.is_empty() {
            // Go ahead and flag an empty status
            Err(ChunkerError::Empty)
        } else {
            // Check to see if we have flagged EoF, and the buffer is smaller than min_size
            if self.eof && self.buffer.len() <= self.settings.min_size {
                // In this case, there are no more bytes to read, and the remaining number of bytes
                // in the buffer is less that the minimum size slice we are allowed to produce, so
                // we just gather up those bytes and return them
                Ok(self.buffer.drain(..).collect())
            } else {
                let mut output = Vec::<u8>::new();
                let mut split = false;
                while !split && output.len() < self.settings.max_size && !self.buffer.is_empty() {
                    // Get the next byte and add it to the output
                    // This unwrap is valid because we ensure the buffer isnt empty in the loop
                    // conditional
                    let byte = self.buffer.pop_front().unwrap();
                    output.push(byte);
                    // Hash it
                    let hash = self.hash_byte(byte);
                    split = (hash & self.settings.mask == 0)
                        && (output.len() >= self.settings.min_size);
                }
                Ok(output)
            }
        }
    }
}

impl Iterator for BuzHashChunker {
    type Item = Result<Vec<u8>, ChunkerError>;
    fn next(&mut self) -> Option<Result<Vec<u8>, ChunkerError>> {
        let slice = self.next_chunk();
        if let Err(ChunkerError::Empty) = slice {
            None
        } else {
            Some(slice)
        }
    }
}

/// static lookup table for the buzhash chunker. Gets xored with a random number before using to
/// prevent fingerprinting.
#[rustfmt::skip]
const TABLE: [u64; 256] = [
    0x6b54_2913_a0bf_10b9, 0x9742_e2a3_f8e8_70ea, 0x130a_f9fb_bd66_6517, 0x7b98_742f_1f33_2bd4,
    0x77f9_fe87_7305_c867, 0x2e8a_0729_dd6a_56c6, 0x4846_1d47_519a_514d, 0x148f_0930_4b18_b7e1,
    0xeff0_60ad_db44_8f05, 0x3fd2_08df_b2b6_61bf, 0x92ed_9fac_7757_da02, 0x9890_66f7_81f3_df17,
    0xf759_b46e_04df_6137, 0xf140_f47b_d73e_63ed, 0xf47a_0888_31c4_6493, 0xcce6_7dee_b83b_8a30,
    0x6db1_6b76_03b0_4434, 0x27b5_395a_c7f9_7e31, 0x68d7_d5d8_e9b8_966a, 0xdd2b_6ac2_9477_6b21,
    0x4e8f_42d9_b98c_d3d0, 0x8343_e377_dd8b_b0db, 0xb0b4_9c12_e8e6_18e3, 0x8e3e_dff6_9beb_3b34,
    0x31f6_7291_cdf9_586f, 0x11bc_f046_4083_6f64, 0x20f7_8c06_efaa_04c7, 0xd399_8102_b1e5_417e,
    0xe81b_a447_0f7e_9166, 0x9554_9f38_c3e3_2b6f, 0x1673_5757_184c_4c0c, 0x18ce_7ade_bc10_4602,
    0x1474_ec62_2fb7_17c1, 0x6fb2_c4fc_71d0_9422, 0x51fe_015b_05e6_1f5b, 0x5a63_3185_082a_e21a,
    0xb559_9139_9a2b_d259, 0x44f4_e4ea_1064_7cee, 0x0a5a_3955_2edc_a548, 0xee5d_4730_b32a_08d5,
    0x6cc7_bf2c_02c7_e6e6, 0xe393_e89b_a294_7ec9, 0x2f3c_e87d_28bb_bdd2, 0x6bf1_d5bc_1a25_92e6,
    0xc8eb_ca0a_b1e8_069d, 0xc4b1_edaf_c1d1_cc78, 0x724e_cff5_ef00_af4a, 0x3a2e_9da7_8dc0_0014,
    0xcdc5_70e2_75be_f997, 0xc20d_200e_df53_aded, 0xd8c4_2bff_eea9_6033, 0xadb1_20a0_ecc3_744c,
    0x4509_4e45_4419_9edb, 0x27c6_0bae_0321_8287, 0x0845_19cb_8152_0816, 0x979c_1e93_1f7c_fa3d,
    0x5191_703e_045d_4aba, 0xc1de_3842_1dc4_c014, 0xf52e_58c6_5462_18f7, 0x3c64_9af9_179c_3d56,
    0xddb5_05d5_082b_bc35, 0xab86_f198_0182_4d07, 0xc3bb_2ad6_ba8c_9ce5, 0x4003_590f_678f_d3ff,
    0xdc3c_d736_792e_3819, 0x9dde_db0e_3f8d_e3ee, 0x1788_6b7b_36fe_5bea, 0xdf18_a6f9_af79_6d19,
    0x531e_7d1d_e2c0_e168, 0x0fb2_256b_12f2_bc79, 0x6b07_2cc6_806f_88c4, 0xe373_c181_aa6a_072b,
    0x9121_b386_be0f_096f, 0x6295_9333_5b80_b86e, 0x2be0_e1a3_343a_7f10, 0x8715_4d9d_a71f_2f70,
    0x239c_e3db_a59c_6026, 0xfb86_3213_6263_8202, 0x0d12_5b98_69d5_efd4, 0x1576_7418_5599_aeef,
    0xeb20_100e_caf9_61b6, 0xba1b_c540_d2ed_c6d8, 0xe0d0_0fa0_ea40_e5c6, 0xdd0c_977f_918b_e20d,
    0xb9dd_4874_f354_0d0a, 0x108b_c36a_3b1e_0392, 0x7874_f59f_5612_debd, 0x036a_d229_f518_398c,
    0xacd8_fae2_a5fa_0308, 0xeb52_1425_c1fd_c800, 0x116f_d32b_0460_02ed, 0xc56c_4198_8025_5603,
    0xf464_0718_e7aa_b52f, 0x0f08_e034_b47f_3859, 0xa326_49d0_24bc_aaf8, 0xdaeb_37e1_c401_e6b3,
    0xcd1e_e36d_094e_7dd1, 0x037c_4969_7eda_2314, 0x68d7_d1a6_0891_93a0, 0x3041_cb24_e95e_5da7,
    0x1077_33ab_ed4b_b164, 0x33ad_f393_9463_de9c, 0x1ec1_790d_7e78_665c, 0xf2f3_1770_204e_3624,
    0x930c_c021_9bf9_908c, 0xd91a_ead0_99db_3e63, 0x9d77_c065_6b66_35bf, 0x1e0c_adfe_a30e_3975,
    0xdb0c_a32a_1a7b_42cb, 0x4060_3407_de64_2795, 0xfa03_d79b_e2f2_8e00, 0xf65f_7de0_eaa5_049f,
    0xfed8_5bf9_bc67_dff0, 0xc785_06df_14e3_e32c, 0xd5cf_d4b1_2398_df0b, 0xa9df_93c1_6ede_e081,
    0x02f1_59dc_8561_043f, 0x4609_1aa5_ffbd_824f, 0xb0c4_762e_dd41_1498, 0xc41b_25b0_02e1_dc2f,
    0x3438_1220_061b_98b1, 0x0fb4_6b0b_27f4_2cdb, 0x3e18_6326_3608_ca96, 0x2060_6c15_77b4_3e5a,
    0xb3a7_2d87_f113_e5d2, 0xd0d1_9db4_1683_e37a, 0x26ab_5067_ba2b_1870, 0xc79d_34be_3aaa_e7bc,
    0x3c42_502a_7177_e118, 0xa1a5_6be4_10a7_653a, 0x4b51_2ad9_5b42_61d2, 0x16f3_a6cb_2a20_13f3,
    0x674d_ca61_7452_2eb8, 0x06e2_18a6_63c9_bc4a, 0x8098_3aea_d34d_d8d7, 0x4c3a_11b9_ee09_32eb,
    0xa80d_6fca_5fd3_8539, 0xfad5_daa6_91cb_d22f, 0x4869_17fd_5086_736a, 0x70f9_e5ff_2542_d8ed,
    0xebe9_10a6_e078_9990, 0xbe95_2f5d_4d8e_6d13, 0x14bb_f5a0_5796_0b06, 0x114b_626d_4015_d519,
    0xf946_ad10_bcbe_6227, 0xd2d9_9340_d9b4_d522, 0x766b_5aec_925a_dceb, 0xc4b3_1c3d_a235_90c6,
    0xe5e5_60e3_97e6_2d71, 0xcf09_f8de_ea59_5da1, 0xc7be_ce80_fdf8_3ff5, 0xd024_0c56_4915_2db1,
    0xf5e0_579c_eaf4_d269, 0x0513_fd9f_e984_5c7f, 0x4ffe_97a1_9256_ecfb, 0x2033_5ade_b09b_314e,
    0x5dab_ef93_7587_c2fc, 0x45ca_c916_ef7b_42eb, 0xcc60_ba4c_a717_4a84, 0xc0a6_a055_494e_66c3,
    0x6d22_9164_2e08_022d, 0x6a4a_ff72_4e8d_afdb, 0x5ddc_abf9_1c8e_06ad, 0x7f3a_2401_0f92_d992,
    0x6221_8f70_4715_d3cc, 0x8072_59f7_a9e5_9693, 0x141d_15f9_bd00_a2ef, 0xb80d_fd10_ac47_b381,
    0x68b5_fb0b_8f16_9451, 0xd717_c3ff_da36_f522, 0x98a8_9f7b_5578_32ee, 0x0cc3_67ff_65b1_c4fc,
    0x5c7d_91b2_65af_b584, 0x3bfe_d46c_1fff_35e7, 0x57b5_1a79_541d_2df0, 0xa670_83d9_39fd_bca8,
    0x01ce_8a6a_807c_da39, 0x358b_4961_1ed2_5ced, 0xb228_4f28_4a33_2f16, 0xab73_d575_2adb_3ac0,
    0x1663_e144_39c2_129e, 0xeac7_b7da_7070_6611, 0xd5d0_f419_eac3_ad95, 0xf7aa_0f5f_46d4_7c4d,
    0xac04_b804_e624_5cde, 0xfa8b_b4b9_8a01_faf1, 0xe36b_3af9_6b0d_ecce, 0x66a3_2e04_4b06_ddd0,
    0x68d1_2709_5d0d_fa2e, 0xf1fc_af7a_e709_8b29, 0xbea3_c5d2_d66d_93f6, 0x028a_d26c_b8bf_6451,
    0xd342_9797_fff3_ca2a, 0x4f2d_4448_59bd_ec39, 0xcd66_d41c_0dfb_027c, 0x0ea7_1c62_3cdf_5106,
    0x6cb2_8202_d2de_35ed, 0x7c89_f672_343f_2c90, 0x8aae_1881_5da2_f59d, 0x897d_22ef_4dc2_4669,
    0xc6ed_6e9c_0f02_cdd0, 0x2a32_de26_5514_f398, 0x94c5_64ba_8ed1_16b2, 0xb339_ad09_6726_df0c,
    0xbe48_8d80_7e88_8e26, 0xcaa1_ba0f_c3bf_fb3d, 0xb4b4_45dd_c491_1c42, 0x0a8f_c30a_1a8c_b5f7,
    0xaa16_fe03_a920_3fa9, 0x2e07_69c2_769d_6795, 0x74f6_26ef_73d0_8f25, 0x144f_4e57_74e9_3945,
    0xc397_1334_d5d5_d6ee, 0x641d_9617_37c7_cb0f, 0x4a54_b67a_8b28_43ba, 0xaa6a_a59e_02d7_e9e0,
    0x7de7_a3d4_6cd3_8262, 0x8945_a42d_78ce_ff90, 0xb618_de78_e33d_7945, 0x64f3_9c33_952f_3da6,
    0x1701_8ec5_b04b_bd8b, 0x637a_e53a_b6b8_04e9, 0x1dfa_420a_3a49_f13b, 0x297d_bcc6_8345_6123,
    0x7b9f_a65a_f043_048d, 0x334e_2615_18ad_8f1a, 0x8d4d_817c_dc21_e387, 0xb13a_2ed1_d8fe_1511,
    0xa99d_ecb8_8c31_9972, 0x3702_9ac4_2d53_8902, 0x7c14_9d5b_8024_f2fe, 0xe680_2aa6_5e61_fd0f,
    0xdd6d_9ddb_e899_0b5f, 0xbdfe_040d_4cb5_1865, 0xcfec_c244_f664_f1d4, 0x5f22_5619_573d_86ea,
    0x4eeb_73a9_8a90_db42, 0x3ebc_4c85_cd6e_e310, 0x1037_0abd_f02e_eb5a, 0x1823_58d5_4130_a1c8,
    0x1898_8f45_f022_82df, 0xad25_3a82_462c_18e8, 0xf36f_70a4_c87c_efcc, 0x836a_b5c5_3a7d_3118,
    0xfe13_05d4_cf86_3f54, 0x148d_2069_0d87_0790, 0x83c6_3a17_21b9_4de3, 0xbb85_6205_e6c7_b355,
    0x39f1_be23_ac04_391a, 0xb8fc_8e4f_b0af_cb0d, 0xad6a_8af6_f376_a27a, 0x9166_a89a_9458_033d,
    0x305c_2dc4_ae74_30a7, 0xa592_fff4_6764_dca5, 0x6a3c_7ca0_1fb1_6edf, 0x2ea8_3c49_5a48_5b32,
];

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use std::io::Cursor;

    // Provides a test slice 5 times the default max size in length
    fn get_test_data() -> Vec<u8> {
        let size = BuzHash::with_default_testing(0).max_size * 10;
        let mut vec = vec![0_u8; size];
        rand::thread_rng().fill_bytes(&mut vec);
        vec
    }

    // Data should be split into one or more chunks.
    //
    // In this case, the data is larger than `max_size`, so it should be more than one chunk
    #[test]
    fn one_or_more_chunks() {
        let data = get_test_data();
        let cursor = Cursor::new(data);
        let chunker = BuzHash::with_default_testing(0);
        let chunks = chunker
            .chunk(cursor)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert!(chunks.len() > 1);
    }

    // Data should be identical after reassembaly by simple concatenation
    #[test]
    fn reassemble_data() {
        let data = get_test_data();
        let cursor = Cursor::new(data.clone());
        let chunks = BuzHash::with_default_testing(0)
            .chunk(cursor)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let rebuilt: Vec<u8> = chunks.concat();
        assert_eq!(data, rebuilt);
    }

    // Running the chunker over the same data twice should result in identical chunks
    #[test]
    fn identical_chunks() {
        let data = get_test_data();
        let cursor1 = Cursor::new(data.clone());
        let chunks1 = BuzHash::with_default_testing(0)
            .chunk(cursor1)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let cursor2 = Cursor::new(data);
        let chunks2 = BuzHash::with_default_testing(0)
            .chunk(cursor2)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(chunks1, chunks2);
    }

    // Verifies that this `Chunker` does not produce chunks larger than its max size
    #[test]
    fn max_size() {
        let data = get_test_data();
        let max_size = BuzHash::with_default_testing(0).max_size;

        let chunks = BuzHash::with_default_testing(0)
            .chunk(Cursor::new(data))
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        for chunk in chunks {
            assert!(chunk.len() <= max_size);
        }
    }

    // Verifies that this `Chunker`, at most, produces 1 under-sized chunk
    #[test]
    fn min_size() {
        let data = get_test_data();
        let min_size = BuzHash::with_default_testing(0).min_size;

        let chunks = BuzHash::with_default_testing(0)
            .chunk(Cursor::new(data))
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        let mut undersized_count = 0;
        for chunk in chunks {
            if chunk.len() < min_size {
                undersized_count += 1;
            }
        }

        assert!(undersized_count <= 1);
    }
}
