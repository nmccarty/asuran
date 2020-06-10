use super::util::LockedFile;
use super::SFTPConnection;
use crate::repository::backend::common::sync_backend::SyncManifest;
use crate::repository::backend::common::{ManifestID, ManifestTransaction};
use crate::repository::backend::BackendError;
use crate::repository::{ChunkSettings, Key};
use crate::{manifest::StoredArchive, repository::backend::Result};

use chrono::prelude::*;
use petgraph::Graph;
use serde_cbor as cbor;
use ssh2::FileStat;

use std::collections::{HashMap, HashSet};
use std::io::{Seek, SeekFrom};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug)]
pub struct SFTPManifest {
    connection: SFTPConnection,
    known_entries: HashMap<ManifestID, ManifestTransaction>,
    verified_memo_pad: HashSet<ManifestID>,
    heads: Vec<ManifestID>,
    file: LockedFile,
    key: Key,
    chunk_settings: ChunkSettings,
    path: PathBuf,
}

impl SFTPManifest {
    /// Will attempt to open or create a manifest at the location pointed to by the path variable of
    /// the given settings at the given server
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::filter_map)]
    pub fn connect(
        settings: impl Into<SFTPConnection>,
        key: &Key,
        chunk_settings: Option<ChunkSettings>,
    ) -> Result<Self> {
        let connection = settings.into().with_connection()?;
        let sftp = connection
            .sftp()
            .expect("Connected successful, but no sftp session?");
        let repository_path = PathBuf::from(&connection.settings().path);
        // Create repository path if it does not exist
        if sftp.stat(&repository_path).is_err() {
            sftp.mkdir(&repository_path, 0o775)?;
        }
        // Construct the path of the manifest folder
        let manifest_path = repository_path.join("manifest");
        // Check to see if it exists
        if let Ok(file_stat) = sftp.stat(&manifest_path) {
            // If it is a file, and not a folder, return failure
            if file_stat.file_type().is_file() {
                return Err(BackendError::ManifestError(format!(
                    "Failed to load manifest, {:?} is a file, not a directory",
                    manifest_path
                )));
            }
        } else {
            // Create the manifest directory
            sftp.mkdir(&manifest_path, 0o775)?;
        }

        // Get the list of manifest files and sort them by ID
        let mut items = sftp
            .readdir(&manifest_path)?
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

        // Collect all known transactions
        let mut known_entries = HashMap::new();
        for (_, path) in &items {
            // Open the file
            let mut file = sftp.open(path)?;
            // Keep deserializing transactions until we hit an error
            let de = cbor::Deserializer::from_reader(&mut file);
            let mut de = de.into_iter::<ManifestTransaction>();
            while let Some(tx) = de.next().and_then(std::result::Result::ok) {
                known_entries.insert(tx.tag(), tx);
            }
        }

        let mut file = None;
        // Attempt to find an unlocked file
        for (_, path) in &items {
            let locked_file = LockedFile::open_read_write(path, Rc::clone(&sftp))?;
            if let Some(f) = locked_file {
                file = Some(f);
                break;
            }
        }

        // If we were unable to find an unlocked file, go ahead and make one
        let file = file.unwrap_or_else(|| {
            let id = if items.is_empty() {
                0
            } else {
                items[items.len() - 1].0 + 1
            };
            let path = manifest_path.join(id.to_string());
            LockedFile::open_read_write(path, Rc::clone(&sftp))
                .expect("Unable to create new lock file (IO error)")
                .expect("Somehow, our newly created lock file is already locked")
        });

        let sfile_path = manifest_path.join("chunk.settings");
        let chunk_settings = if let Some(chunk_settings) = chunk_settings {
            // Attempt to open the chunk settings file and update it
            let mut sfile = LockedFile::open_read_write(&sfile_path, Rc::clone(&sftp))?
                .ok_or_else(|| {
                    BackendError::ManifestError("Unable to lock chunk.settings".to_string())
                })?;
            // Clear out the file
            sftp.setstat(
                &sfile_path,
                FileStat {
                    size: Some(0),
                    uid: None,
                    gid: None,
                    perm: None,
                    atime: None,
                    mtime: None,
                },
            )?;
            // Write out new chunksettings
            cbor::ser::to_writer(&mut sfile, &chunk_settings)?;
            chunk_settings
        } else {
            let mut sfile = sftp.open(&sfile_path)?;
            cbor::de::from_reader(&mut sfile)?
        };

        // Construct the manifest
        let mut manifest = SFTPManifest {
            connection,
            known_entries,
            verified_memo_pad: HashSet::new(),
            heads: Vec::new(),
            file,
            key: key.clone(),
            chunk_settings,
            path: manifest_path,
        };
        // Build the list of heads
        manifest.build_heads();
        // Verify each head
        for head in manifest.heads.clone() {
            if !manifest.verify_tx(head) {
                return Err(BackendError::ManifestError(format!(
                    "Manifest Transaction failed verification! {:?}",
                    manifest.known_entries.get(&head).ok_or_else(|| BackendError::Unknown("Failed to get the head of the known entries list while reporting an error".to_string()))?
                )));
            }
        }

        Ok(manifest)
    }

    /// Gets the heads from a list of transactions
    fn build_heads(&mut self) {
        // Create the graph
        let mut graph: Graph<ManifestID, ()> = Graph::new();
        let mut index_map = HashMap::new();
        // Add each transaction to our map
        for tx in self.known_entries.values() {
            let tag = tx.tag();
            let id = graph.add_node(tag);
            index_map.insert(tag, id);
        }
        // Go through each transaction in the graph, adding an edge in the new -> old direction
        // These unwraps are safe because we just added these entries to our hashmap
        for tx in self.known_entries.values() {
            let id = index_map.get(&tx.tag()).unwrap();
            for other_tx in tx.previous_heads() {
                let other_id = index_map.get(&other_tx).unwrap();
                graph.update_edge(*id, *other_id, ());
            }
        }
        // reverse all the nodes, so they now point from old to new
        graph.reverse();
        // Find all nodes with no outgoing edges, these are our heads
        let mut heads = Vec::new();
        for (tag, id) in &index_map {
            let mut edges = graph.edges(*id);
            if edges.next() == None {
                heads.push(*tag);
            }
        }

        self.heads = heads;
    }

    /// Verifies a transaction and all of its parents
    fn verify_tx(&mut self, id: ManifestID) -> bool {
        if self.verified_memo_pad.contains(&id) {
            true
        } else {
            let tx = self
                .known_entries
                .get(&id)
                .expect("Item in verified memo pad was not in known_entries")
                .clone();
            if tx.verify(&self.key) {
                self.verified_memo_pad.insert(id);
                for parent in tx.previous_heads() {
                    if !self.verify_tx(*parent) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
    }
}

impl SyncManifest for SFTPManifest {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&mut self) -> Result<chrono::DateTime<chrono::FixedOffset>> {
        if self.heads.is_empty() {
            Ok(Local::now().with_timezone(Local::now().offset()))
        } else {
            let first_head = self
                .known_entries
                .get(&self.heads[0])
                .expect("Item in heads was not in known entries");
            let mut max = first_head.timestamp();
            for id in &self.heads {
                let tx = self.known_entries.get(id).ok_or_else(|| {
                    BackendError::ManifestError("Unable to load timestamp".to_string())
                })?;
                if tx.timestamp() > max {
                    max = tx.timestamp()
                }
            }
            Ok(max)
        }
    }
    fn chunk_settings(&mut self) -> ChunkSettings {
        self.chunk_settings
    }
    fn archive_iterator(&mut self) -> Self::Iterator {
        let mut items = self.known_entries.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));
        items.reverse();
        items
            .into_iter()
            .map(StoredArchive::from)
            .collect::<Vec<_>>()
            .into_iter()
    }
    fn write_chunk_settings(&mut self, chunk_settings: ChunkSettings) -> Result<()> {
        let sftp = self.connection.sftp().unwrap();
        let sfile_path = self.path.join("chunk.settings");
        // Attempt to open the chunk settings file and update it
        let mut sfile =
            LockedFile::open_read_write(&sfile_path, Rc::clone(&sftp))?.ok_or_else(|| {
                BackendError::ManifestError("Unable to lock chunk.settings".to_string())
            })?;
        // Clear out the file
        sftp.setstat(
            &sfile_path,
            FileStat {
                size: Some(0),
                uid: None,
                gid: None,
                perm: None,
                atime: None,
                mtime: None,
            },
        )?;
        // Write out new chunksettings
        cbor::ser::to_writer(&mut sfile, &chunk_settings)?;
        self.chunk_settings = chunk_settings;
        Ok(())
    }
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        // Create the transaction
        let tx = ManifestTransaction::new(
            &self.heads,
            archive.id(),
            archive.timestamp(),
            self.chunk_settings.hmac,
            &self.key,
        );
        // Write the transaction to the file
        let file = &mut self.file;
        file.seek(SeekFrom::End(0))?;
        cbor::ser::to_writer(file, &tx)?;
        // Add the transaction to our entries list
        let id = tx.tag();
        self.known_entries.insert(id, tx);
        // Update our heads to only contain this transaction
        self.heads = vec![id];
        Ok(())
    }
    fn touch(&mut self) -> Result<()> {
        // Touch doesn't actually do anything with this implementation
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{Compression, Encryption, HMAC};
    use crate::repository::backend::sftp::SFTPSettings;
    use std::collections::HashSet;
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

    fn get_manifest(
        path: impl AsRef<str>,
        key: &Key,
        chunk_settings: Option<ChunkSettings>,
    ) -> SFTPManifest {
        let path = path.as_ref().to_string();
        SFTPManifest::connect(get_settings(path), key, chunk_settings)
            .expect("Unable to connect to manifest")
    }

    #[test]
    fn connect() {
        let key = Key::random(32);
        get_manifest(
            "asuran/manifest-connect",
            &key,
            Some(ChunkSettings::lightweight()),
        );
    }

    #[test]
    #[should_panic]
    fn creation_without_settings() {
        let key = Key::random(32);
        get_manifest("asuran/manifest-creation-without-settings", &key, None);
    }

    #[test]
    fn chunk_settings() {
        let key = Key::random(32);
        let mut manifest = get_manifest(
            "asuran/manifest-chunksettings",
            &key,
            Some(ChunkSettings::lightweight()),
        );
        let settings = ChunkSettings {
            compression: Compression::ZStd { level: 1 },
            encryption: Encryption::new_aes256ctr(),
            hmac: HMAC::Blake3,
        };
        manifest
            .write_chunk_settings(settings)
            .expect("Unable to write chunk settings");
        drop(manifest);
        let mut manifest = get_manifest("asuran/manifest-chunksettings", &key, None);
        let new_settings = manifest.chunk_settings();
        assert!(settings == new_settings)
    }

    #[test]
    fn touch_and_modification() {
        let key = Key::random(32);
        let mut manifest = get_manifest(
            "asuran/manifest_touch",
            &key,
            Some(ChunkSettings::lightweight()),
        );
        manifest.touch().unwrap();
        let x = manifest.last_modification().unwrap();
        drop(manifest);
        let mut manifest = get_manifest("asuran/manifest_touch", &key, None);
        std::thread::sleep(std::time::Duration::from_secs(2));
        manifest.touch().unwrap();
        drop(manifest);
        let mut manifest = get_manifest("asuran/manifest_touch", &key, None);
        let y = manifest.last_modification().unwrap();
        assert!(x != y)
    }

    #[test]
    fn archives() {
        let key = Key::random(32);
        let dummy_archives: HashSet<StoredArchive> =
            (0..10).map(|_| StoredArchive::dummy_archive()).collect();

        let mut manifest = get_manifest(
            "asuran/manifest_archives",
            &key,
            Some(ChunkSettings::lightweight()),
        );

        for archive in &dummy_archives {
            manifest
                .write_archive(archive.clone())
                .expect("Unable to write archive");
        }

        drop(manifest);
        let mut manifest = get_manifest("asuran/manifest_archives", &key, None);

        let output: HashSet<StoredArchive> = manifest.archive_iterator().collect();

        assert!(dummy_archives == output);
    }
}
