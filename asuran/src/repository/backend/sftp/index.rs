use super::SFTPSettings;
use crate::repository::backend::common::sync_backend::SyncIndex;
use crate::repository::backend::{Result, SegmentDescriptor};
use crate::repository::ChunkID;

use std::collections::HashSet;

#[derive(Debug)]
pub struct SFTPIndex {}

impl SFTPIndex {
    pub fn connect(_settings: SFTPSettings) -> Result<Self> {
        todo!()
    }
}

impl SyncIndex for SFTPIndex {
    fn lookup_chunk(&mut self, _id: ChunkID) -> Option<SegmentDescriptor> {
        todo!()
    }
    fn set_chunk(&mut self, _id: ChunkID, _location: SegmentDescriptor) -> Result<()> {
        todo!()
    }
    fn known_chunks(&mut self) -> HashSet<crate::prelude::ChunkID> {
        todo!()
    }
    fn commit_index(&mut self) -> Result<()> {
        todo!()
    }
    fn chunk_count(&mut self) -> usize {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn get_settings(path: String) -> SFTPSettings {
        let hostname = env::var_os("ASURAN_SFTP_HOSTNAME")
            .map(|x| x.into_string().unwrap())
            .expect("Server must be set");
        let username = env::var_os("ASURAN_SFTP_USER")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("asuran".to_string());
        let password = env::var_os("ASURAN_SFTP_PASS")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("asuran".to_string());
        let port = env::var_os("ASURAN_SFTP_PORT")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("22".to_string())
            .parse::<u16>()
            .expect("Unable to parse port");

        SFTPSettings {
            hostname,
            username,
            port: Some(port),
            password: Some(password),
            path,
        }
    }

    fn get_index(path: impl AsRef<str>) -> SFTPIndex {
        let path = path.as_ref().to_string();
        SFTPIndex::connect(get_settings(path)).expect("Unable to connect to index")
    }

    #[test]
    fn set_lookup_chunk() {
        let mut index = get_index("index_set_lookup_chunk");
        let chunk_id = ChunkID::random_id();
        let descriptor = SegmentDescriptor {
            segment_id: 42,
            start: 43,
        };
        index
            .set_chunk(chunk_id, descriptor)
            .expect("Unable to write chunk location");
        index.commit_index().expect("Unable to commit index");
        drop(index);
        let mut index = get_index("index_set_lookup_chunk");
        let result = index
            .lookup_chunk(chunk_id)
            .expect("Unable to read chunk location");

        assert!(descriptor == result);
    }

    #[test]
    fn chunk_count() {
        let mut index = get_index("index_set_lookup_chunk");
        let descriptor = SegmentDescriptor {
            segment_id: 42,
            start: 43,
        };
        for _ in 0..10 {
            index
                .set_chunk(ChunkID::random_id(), descriptor)
                .expect("Unable to set chunk");
        }
        drop(index);
        let mut index = get_index("index_set_lookup_chunk");
        assert_eq!(index.chunk_count(), 10);
    }

    #[test]
    fn known_chunks() {
        let mut index = get_index("index_set_lookup_chunk");
        let descriptor = SegmentDescriptor {
            segment_id: 42,
            start: 43,
        };
        let chunks: HashSet<ChunkID> = (0..10).map(|_| ChunkID::random_id()).collect();
        for chunk in &chunks {
            index
                .set_chunk(*chunk, descriptor)
                .expect("Unable to set chunk");
        }
    }
}
