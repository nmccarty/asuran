use std::io::Read;

#[cfg(feature = "profile")]
use flamer::*;

pub struct Slice {
    pub data: Vec<u8>,
    pub start: u64,
    pub end: u64,
}

pub struct IteratedReader<R: Read> {
    /// Internal Reader
    reader: R,
    /// Hasher
    hasher: BuzHash,
    /// Hash Mask
    mask: u32,
    /// Minimum chunk size
    min_size: usize,
    /// Maximum chunk size
    max_size: usize,
    /// read buffer
    buffer: [u8; 8192],
    /// location of the cursor in the buffer
    cursor: usize,
    /// length of the data in the buffer
    buffer_len: usize,
    /// location of the cursor in the file
    location: usize,
    /// Flag for if end of file reached
    finished: bool,
}

impl<R: Read> Iterator for IteratedReader<R> {
    type Item = Slice;

    #[cfg_attr(feature = "profile", flame)]
    fn next(&mut self) -> Option<Slice> {
        if self.finished {
            None
        } else {
            let start = self.location;
            let mut end = self.location;
            let mut output = Vec::<u8>::new();
            let hasher = &mut self.hasher;
            hasher.reset();
            let mut split = false;
            while !split {
                if self.cursor < self.buffer_len {
                    let byte = self.buffer[self.cursor];
                    output.push(byte);
                    let hash = hasher.hash_byte(byte);
                    let len = output.len();
                    if (hash & self.mask) == 0 && (len >= self.min_size) && (len <= self.max_size) {
                        split = true;
                        end = self.location;
                    }

                    self.location += 1;
                    self.cursor += 1;
                } else {
                    self.cursor = 0;
                    let result = self.reader.read(&mut self.buffer);
                    match result {
                        Err(_) => {
                            split = true;
                            end = self.location;
                            self.finished = true;
                        }
                        Ok(0) => {
                            split = true;
                            end = self.location;
                            self.finished = true;
                        }
                        Ok(n) => {
                            self.buffer_len = n;
                        }
                    }
                }
            }

            Some(Slice {
                data: output,
                start: start as u64,
                end: end as u64,
            })
        }
    }
}

#[derive(Clone)]
pub struct Chunker {
    /// Hash Mask
    mask: u32,
    /// Hasher
    hasher: BuzHash,
    /// Mask bits count
    mask_bits: u32,
}

impl Chunker {
    /// Creates a new chunker with the given window and mask bits
    pub fn new(window: u64, mask_bits: u32, nonce: u32) -> Chunker {
        Chunker {
            mask: 2_u32.pow(mask_bits) - 1,
            hasher: BuzHash::new(nonce, window as u32),
            mask_bits,
        }
    }

    /// Produces an iterator over the slices in a file
    pub fn chunked_iterator<R: Read>(&self, reader: R) -> IteratedReader<R> {
        IteratedReader {
            reader,
            hasher: self.hasher.clone(),
            mask: self.mask,
            min_size: 2_usize.pow(self.mask_bits - 2),
            max_size: 2_usize.pow(self.mask_bits + 2),
            buffer: [0_u8; 8192],
            cursor: 0,
            buffer_len: 0,
            location: 0,
            finished: false,
        }
    }
}

/// Buzhash implemtation
#[derive(Clone)]
struct BuzHash {
    state: u32,
    buf: Vec<u8>,
    size: u32,
    shiftn: u64,
    shiftm: u64,
    bufpos: u32,
    overflow: bool,
    table: Vec<u32>,
}

impl BuzHash {
    fn new(nonce: u32, size: u32) -> BuzHash {
        let mut table = BUZTABLE.to_vec();
        for val in table.iter_mut() {
            *val ^= nonce;
        }

        let shiftn = u64::from(size % 32);
        let shiftm = 32 - shiftn;

        BuzHash {
            table,
            state: 0,
            size,
            buf: vec![0u8; size as usize],
            shiftn,
            shiftm,
            bufpos: 0,
            overflow: false,
        }
    }

    fn hash_byte(&mut self, byte: u8) -> u32 {
        if self.bufpos == self.size {
            self.overflow = true;
            self.bufpos = 0;
        }

        let mut state = (self.state << 1) | (self.state >> 31);

        if self.overflow {
            let to_shift = self.table[self.buf[self.bufpos as usize] as usize];
            state ^= (to_shift << self.shiftn) | (to_shift >> self.shiftm)
        }

        self.buf[self.bufpos as usize] = byte;
        self.bufpos += 1;

        state ^= self.table[byte as usize];
        self.state = state;

        state
    }

    fn reset(&mut self) {
        self.state = 0;
        self.bufpos = 0;
        self.overflow = false;
    }
}

const BUZTABLE: [u32; 256] = [
    0x12bd_9527,
    0xf414_0cea,
    0x987b_d6e1,
    0x7907_9850,
    0xafbf_d539,
    0xd350_ce0a,
    0x8297_3931,
    0x9fc3_2b9c,
    0x2800_3b88,
    0xc30c_13aa,
    0x6b67_8c34,
    0x5844_ef1d,
    0xaa55_2c18,
    0x4a77_d3e8,
    0xd1f6_2ea0,
    0x6599_417c,
    0xfbe3_0e7a,
    0xf9e2_d5ee,
    0xa1fc_a42e,
    0x4154_8969,
    0x116d_5b59,
    0xaeda_1e1a,
    0xc519_1c17,
    0x54b9_a3cb,
    0x727e_492a,
    0x5c43_2f91,
    0x31a5_0bce,
    0xc269_6af6,
    0x217c_8020,
    0x1262_aefc,
    0xace7_5924,
    0x9876_a04f,
    0xaf30_0bc2,
    0x3ffc_e3f6,
    0xd668_0fb5,
    0xd0b1_ced8,
    0x6651_f842,
    0x736f_adef,
    0xbc2d_3429,
    0xb03d_2904,
    0x7e63_4ba4,
    0xdfd8_7d8c,
    0x7988_d63a,
    0x4be4_d933,
    0x6a8d_0382,
    0x9e13_2d62,
    0x3ee9_c95f,
    0xfec0_5b97,
    0x6907_ad34,
    0x8616_cfcc,
    0xa6aa_bf24,
    0x8ad1_c92e,
    0x4f2a_ffc0,
    0xb875_19db,
    0x6576_eaf6,
    0x15db_e00a,
    0x63e1_dd82,
    0xa36b_6a81,
    0xeead_99b3,
    0xbc6a_4309,
    0x3478_d1a7,
    0x2182_bcc0,
    0xdd50_cfce,
    0x7cb2_5580,
    0x7307_5483,
    0x503b_7f42,
    0x4cd5_0d63,
    0x3f4d_94c9,
    0x385f_cbb7,
    0x90da_f16c,
    0xece1_0b8e,
    0x11c1_cb04,
    0x816a_899b,
    0x69a2_9d06,
    0xfb09_0b37,
    0xf98e_f13c,
    0x0765_3435,
    0x9f15_dc42,
    0x3b43_abdf,
    0x1334_283f,
    0x93f3_d9af,
    0x0cbd_fe71,
    0xa788_a614,
    0x4f54_d2f0,
    0xd437_4fc7,
    0x7055_7ce7,
    0xf741_fce8,
    0xe4b6_f661,
    0xc630_cb98,
    0x387a_6366,
    0x72f4_28fd,
    0x5390_09db,
    0xc53e_3810,
    0x1e1a_52e5,
    0x7d68_16b0,
    0x040f_9b81,
    0x9c99_c9fb,
    0x9f3a_f3d2,
    0x774d_1061,
    0xd5c8_40ea,
    0x8e14_80fe,
    0x6ee4_023c,
    0x2fbd_a535,
    0xd88e_ff7a,
    0xd863_2a2a,
    0x43c4_e024,
    0x3ef2_7971,
    0xc728_66fd,
    0xe35c_c630,
    0x46d9_6220,
    0x437a_8384,
    0xe92c_af0c,
    0x6290_a47e,
    0xa7bb_9238,
    0x0e10_00f9,
    0x49e7_6bdc,
    0x3acf_b4b8,
    0x0358_2b8e,
    0x6ea2_de4e,
    0x2ec1_008d,
    0xfcc8_df69,
    0x91c2_fe0a,
    0xb471_c7d9,
    0x778b_e812,
    0x70d2_9ad1,
    0x7641_1cbf,
    0xc302_e81c,
    0x4e44_5194,
    0x22e3_aa72,
    0xb657_62e9,
    0xa280_db05,
    0x827a_a70e,
    0x4c53_1a9d,
    0x7a60_bf4a,
    0x8fd9_5a44,
    0x2289_aef0,
    0xcd50_ddc4,
    0x639a_ae69,
    0x5fe8_5ed6,
    0x4ed7_24ff,
    0x00f0_4f7d,
    0x95a5_fcb0,
    0x8825_5d15,
    0xa603_d2c9,
    0xf695_6a5b,
    0x53ea_7f3e,
    0xb570_f225,
    0x2b3b_e203,
    0xa181_e40e,
    0xc413_cdce,
    0xa7cb_1ebb,
    0xcf25_8b1f,
    0x516e_b016,
    0xca20_4586,
    0xd1e6_9894,
    0xe85a_73d3,
    0x7db2_d382,
    0xae73_b463,
    0x3598_d643,
    0x5087_c864,
    0xd91f_30b6,
    0xe1d4_d1e7,
    0x73b3_b337,
    0xceac_1233,
    0x8edf_7845,
    0xa69c_45c9,
    0xdb5d_b3ab,
    0x28cf_ade8,
    0xebfa_49e7,
    0xcbc2_a659,
    0x59cc_e971,
    0x959a_01af,
    0x8ee9_aae7,
    0xfb2f_01c6,
    0x5a75_2836,
    0x9ed1_2981,
    0x618d_05b6,
    0x93ec_12b3,
    0x4590_c779,
    0xed13_17a2,
    0x03fe_5835,
    0x7ad3_c6f7,
    0xd4aa_d5b5,
    0x1a99_5ed7,
    0x247b_faa4,
    0x69c2_c799,
    0x745f_a405,
    0xc5b9_f239,
    0xc3d9_aebc,
    0xa6f6_0e0b,
    0xdf1e_91d7,
    0xab8e_041c,
    0xee31_88c6,
    0x3737_7a9e,
    0xc0e1_a3bf,
    0x19a5_a9e4,
    0x56cb_9556,
    0xc4d3_3d3f,
    0xfb1e_b03e,
    0xf955_7057,
    0x1be3_1d37,
    0xd1fa_65f1,
    0xf518_d714,
    0x570a_c722,
    0xf26c_f66a,
    0x2479_4d47,
    0x8ba2_e402,
    0x3f51_37e6,
    0x35be_1453,
    0x4335_0478,
    0x9f05_ee88,
    0x364c_f9cf,
    0x39a2_3ee7,
    0xa4db_8d49,
    0xc2eb_b3d2,
    0xc6fb_99d5,
    0xe014_dfb0,
    0x7156_d425,
    0xe090_a87a,
    0x4cc1_2f78,
    0x1b30_f503,
    0x0669_4a7a,
    0x6819_8cd1,
    0x2f83_45bd,
    0x9d79_198e,
    0xd871_943f,
    0x22ef_6cf4,
    0xe81b_1c15,
    0x067b_61d8,
    0xfc4e_a4f5,
    0xfe6d_ab57,
    0x1bf7_44ba,
    0xa70b_6a25,
    0xafe6_e412,
    0xc6c1_a05c,
    0x8ffb_e3ce,
    0xc427_0af1,
    0xf3f3_6373,
    0xc450_7dd8,
    0x5e6f_d1e2,
    0x58cd_9739,
    0x47d3_c5b5,
    0xe1d5_a343,
    0x3d4d_ea4a,
    0x893d_91ae,
    0xbb2a_5e2a,
    0x0d57_b800,
    0x652a_7cc9,
    0x6a68_ccfd,
    0x6252_9f0b,
    0xec5f_36d6,
    0x766c_ceda,
    0x96ca_63ef,
    0xa049_9838,
    0xd903_0f59,
    0x8185_f4d2,
];
