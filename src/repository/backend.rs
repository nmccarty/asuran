use std::io::Read;

use crate::repository::*;

pub trait Backend {
    /// Takes a block key, and returns a Read over the block
    fn get_block(&self, key: &Key) -> Box<dyn Read>;
}
