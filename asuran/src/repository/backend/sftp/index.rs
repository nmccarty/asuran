use super::util::LockedFile;
use super::SFTPConnection;
use crate::repository::backend::common::sync_backend::SyncIndex;
use crate::repository::backend::common::IndexTransaction;
use crate::repository::backend::{BackendError, Result, SegmentDescriptor};
use crate::repository::ChunkID;

use serde_cbor as cbor;

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{BufWriter, Seek, SeekFrom};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug)]
pub struct SFTPIndex {
    state: HashMap<ChunkID, SegmentDescriptor>,
    file: LockedFile,
    changes: Vec<IndexTransaction>,
}

impl SFTPIndex {
    #[allow(clippy::filter_map)]
    pub fn connect(settings: impl Into<SFTPConnection>) -> Result<Self> {
        // First make sure that we have a connection
        let connection = settings.into().with_connection()?;
        // Get our sftp connection
        let sftp = connection
            .sftp()
            .expect("Connection succeed but no sftp session?");
        // Construct the path of the index folder
        let repository_path = PathBuf::from(&connection.settings().path);
        // Create repository path if it does not exist
        if sftp.stat(&repository_path).is_err() {
            sftp.mkdir(&repository_path, 0o775)?;
        }
        let index_path = repository_path.join("index");
        // Check to see if it exists
        if let Ok(file_stat) = sftp.stat(&index_path) {
            // If it is a file and not a folder, return failure
            if file_stat.file_type().is_file() {
                return Err(BackendError::IndexError(format!(
                    "Failed to load index, {:?} is a file, not a directory",
                    index_path
                )));
            }
        } else {
            // Create the index directory
            sftp.mkdir(&index_path, 0o775)?;
        }
        // Create the state map
        let mut state: HashMap<ChunkID, SegmentDescriptor> = HashMap::new();

        // Get the list of files and sort them by id
        let mut items = sftp
            .readdir(&index_path)?
            .into_iter()
            // Make sure its a file
            .filter(|(_path, file_stat)| file_stat.file_type().is_file())
            // Now that we only have files, drop the file_stat
            .map(|(path, _file_stat)| path)
            // Make sure the file name component is a number, and map to (number, path)
            .filter_map(|path| {
                path.file_name()
                    .and_then(|x| x.to_string_lossy().parse::<u64>().ok())
                    .map(|x| (x, path))
            })
            .collect::<Vec<_>>();
        // Sort the list of files by id
        items.sort_by(|a, b| a.0.cmp(&b.0));

        // Iterate through each file, adding all the transactions to our state hashmap
        for (_, path) in &items {
            let mut file = sftp.open(path)?;
            // Keep deserializing transactions until we encounter an error
            let de = cbor::Deserializer::from_reader(&mut file);
            let mut de = de.into_iter::<IndexTransaction>();
            while let Some(tx) = de.next().and_then(std::result::Result::ok) {
                state.insert(tx.chunk_id, tx.descriptor);
            }
        }

        // Check to see if there are any unlocked files, and if so, use the first
        for (_, path) in &items {
            let locked_file = LockedFile::open_read_write(path, Rc::clone(&sftp))?;
            if let Some(file) = locked_file {
                return Ok(SFTPIndex {
                    state,
                    file,
                    changes: Vec::new(),
                });
            }
        }

        // If we have gotten here, there are no unlocked files, creating one
        let id = if items.is_empty() {
            0
        } else {
            items[items.len() - 1].0 + 1
        };

        let path = index_path.join(id.to_string());
        let file = LockedFile::open_read_write(path, Rc::clone(&sftp))?
            .expect("Somehow, we aren't able to lock our newly created index file");

        Ok(SFTPIndex {
            state,
            file,
            changes: Vec::new(),
        })
    }
}

impl SyncIndex for SFTPIndex {
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.state.get(&id).copied()
    }
    #[allow(clippy::map_entry)]
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        if !self.state.contains_key(&id) {
            self.state.insert(id, location);
            let transaction = IndexTransaction {
                chunk_id: id,
                descriptor: location,
            };
            self.changes.push(transaction);
        }
        Ok(())
    }
    fn known_chunks(&mut self) -> HashSet<ChunkID> {
        self.state
            .keys()
            .copied()
            .chain(self.changes.iter().map(|x| x.chunk_id))
            .collect()
    }
    fn commit_index(&mut self) -> Result<()> {
        let mut file = BufWriter::new(&mut self.file);
        file.seek(SeekFrom::End(0))?;
        for tx in self.changes.drain(0..self.changes.len()) {
            cbor::ser::to_writer(&mut file, &tx)?;
        }
        drop(file);
        self.file.fsync()?;
        Ok(())
    }
    fn chunk_count(&mut self) -> usize {
        self.state.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::sftp::SFTPSettings;
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
    fn connect() {
        let _index = get_index("asuran/index_connect");
    }

    #[test]
    fn set_lookup_chunk() {
        let mut index = get_index("asuran/index_set_lookup_chunk");
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
        let mut index = get_index("asuran/index_set_lookup_chunk");
        let result = index
            .lookup_chunk(chunk_id)
            .expect("Unable to read chunk location");

        assert!(descriptor == result);
    }

    #[test]
    fn chunk_count() {
        let mut index = get_index("asuran/index_chunk_count");
        let descriptor = SegmentDescriptor {
            segment_id: 42,
            start: 43,
        };
        for _ in 0..10 {
            index
                .set_chunk(ChunkID::random_id(), descriptor)
                .expect("Unable to set chunk");
        }
        println!("Index changes length: {}", index.changes.len());
        println!("Index state length: {}", index.state.len());
        index.commit_index().expect("Unable to commit index");
        drop(index);
        let mut index = get_index("asuran/index_chunk_count");
        assert_eq!(index.chunk_count(), 10);
    }

    #[test]
    fn known_chunks() {
        let mut index = get_index("asuran/index_known_chunks");
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
        index.commit_index().expect("unable to commit index");
        drop(index);
        let mut index = get_index("asuran/index_known_chunks");
        let new_chunks = index.known_chunks();
        assert!(new_chunks == chunks);
    }
}
