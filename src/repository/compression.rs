/// Compression algorithim
pub enum Compression {
    NoCompression,
    ZStd { level: i32 },
}
