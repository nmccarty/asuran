//! Asuran `FlatFile`'s are structured as an initial header, followed by an append
//! only log of 'entries'.
//!
//! # Initial Header
//!
//! The initial header contains three components:
//!
//! 1. Magic Number
//!
//!     The magic number identifying asuran `FlatFile`s is the 8-byte string
//!     `b"ASURAN_F"`.
//!
//! 2. Length of header
//!
//!     The total length of the encrypted key, in bytes, as a u16.
//!
//! 3. The `EncryptedKey`
//!
//!     The serialized, encrypted key material for this repository.
//!
//! The first byte of the first entry immediately follows the last byte of the
//! initial header
//!
//! # Entries
//!
//! An entry is composed of three parts:
//!
//! 1. The Header
//!
//!     The header is a sequence of three u16s, each indicating the major, minor,
//!     and patch version of the version of asuran to last write to the file. This
//!     is then followed by the 16-byte implementation UUID. This is then followed
//!     by two `u64`s, the first being the location of the footer, and the second
//!     being the location of the next header. The location of the next header will
//!     be beyond the end of the file if you are reading the last entry in a file.
//!
//! 2. The Body
//!
//!     The body is a length of concatenated raw chunk bodies, and does not contain
//!     any structure beyond being a list of bytes.
//!
//! 3. The Footer
//!
//!     The footer contains two parts, a `u64` describing the length of the
//!     following `Chunk`, then the serialized `EntryFooterData` struct, wrapped and
//!     encrypted/compressed in a `Chunk`
//!
//! `FlatFile` repositories are always terminated with an `EntryHeader` with the
//! `footer_offset` and `next_header_offset` set to 0. This is intended to be
//! overridden during the next writing session.
use super::sync_backend::{SyncBackend, SyncIndex, SyncManifest};
use crate::repository::backend::{
    BackendError, Chunk, ChunkID, ChunkSettings, EncryptedKey, Result, SegmentDescriptor,
    StoredArchive,
};
use crate::repository::Key;
use asuran_core::repository::backend::flatfile::{
    EntryFooter, EntryFooterData, EntryHeader, FlatFileHeader,
};
use asuran_core::repository::chunk::{ChunkBody, ChunkHeader};

use chrono::{DateTime, FixedOffset};

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub use asuran_core::repository::backend::flatfile::MAGIC_NUMBER;

/// A view over a generic `FlatFile` backend.
///
/// This generic backend can accept any (owned) `Read + Write + Seek`, and will
/// implement the same binary format on top of it.
///
/// See module level documentation for details.
pub struct GenericFlatFile<F: Read + Write + Seek + 'static> {
    file: F,
    path: PathBuf,
    chunk_settings: ChunkSettings,
    index: HashMap<ChunkID, SegmentDescriptor>,
    length_map: HashMap<SegmentDescriptor, u64>,
    manifest: Vec<StoredArchive>,
    entry_footer_data: EntryFooterData,
    chunk_settings_modified: bool,
    enc_key: EncryptedKey,
    key: Key,
    chunk_headers: HashMap<SegmentDescriptor, ChunkHeader>,
    header_offset: u64,
}

impl<F: Read + Write + Seek + 'static> Debug for GenericFlatFile<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenericFlatFile")
            .field("file_type", &std::any::type_name::<F>())
            .field("path", &self.path)
            .finish()
    }
}

impl<F: Read + Write + Seek + 'static> GenericFlatFile<F> {
    /// Opens up a new `GenericFlatFile` over the provided `Read + Write + Seek`
    ///
    /// If the given 'file' is empty, it will write the initial headers, otherwise, it
    /// will walk through all the headers and footers in the repository, and parse them
    /// in to their needed forms.
    ///
    /// If the given 'file' is empty, and thus the repository is being initialized,
    /// chunk settings and an encrypted key *must* be passed.
    ///
    /// # Errors
    ///
    /// - If an underlying I/O Error occurs
    /// - If the caller is initializing a repository, but did not provide chunk settings
    ///   or an encrypted key, `Err(ManifestError)`
    /// - If the caller provides an encrypted key for an already initialized repository
    /// - If decoding of the encrypted key or any of the headers/footers fails
    ///   `Err(FlatFileError)`
    /// - If any of the chunks described by the footers do not have an associated `ChunkHeader`
    /// - If an already initalized repository does not contain any footers
    #[allow(clippy::too_many_lines)]
    pub fn new_raw(
        mut file: F,
        path: impl AsRef<Path>,
        settings: Option<ChunkSettings>,
        key: Key,
        enc_key: Option<EncryptedKey>,
    ) -> Result<GenericFlatFile<F>> {
        // Check to see if file is empty, if so we need to write an initial header
        let file_length = file.seek(SeekFrom::End(0))?;
        if file_length == 0 {
            // We need to have chunk settings and an encrypted key in this case
            let settings = settings.ok_or_else(|| {
                BackendError::ManifestError(
                    "Attempted to create a FlatFile without supplying chunk settings".to_string(),
                )
            })?;
            let enc_key = enc_key.ok_or_else(|| {
                BackendError::ManifestError(
                    "Attempted to create a FlatFile without supplying an encrypted key".to_string(),
                )
            })?;
            // Create the header and write it
            let header = FlatFileHeader::new(&enc_key)?;
            header.to_write(&mut file)?;
            let header =
                EntryHeader::new(&*crate::VERSION_STRUCT, 0, 0, *crate::IMPLEMENTATION_UUID)?;
            // save the header_location
            let header_location = file.seek(SeekFrom::End(0))?;
            // Write the header
            header.to_write(&mut file)?;

            let flat_file = GenericFlatFile {
                file,
                path: path.as_ref().to_owned(),
                chunk_settings: settings,
                index: HashMap::new(),
                length_map: HashMap::new(),
                manifest: Vec::new(),
                entry_footer_data: EntryFooterData::new(settings),
                chunk_settings_modified: true,
                enc_key,
                key,
                chunk_headers: HashMap::new(),
                header_offset: header_location,
            };
            Ok(flat_file)
        } else {
            let path: PathBuf = path.as_ref().to_owned();
            // First read the header for the file
            file.seek(SeekFrom::Start(0))?;
            let global_header = FlatFileHeader::from_read(&mut file)?;
            // Extract the encrypted key and flag an error if the user is trying to set ones
            if enc_key.is_some() {
                return Err(BackendError::ManifestError(
                    "Attempted to set a key on an already existing flatfile repository".to_string(),
                ));
            }
            let enc_key = global_header.key()?;
            // Extract the first entry header
            let mut header_offset = file.seek(SeekFrom::Current(0))?;
            let mut entry_header = EntryHeader::from_read(&mut file)?;
            // Create a place to put the chunk settings
            let mut chunk_settings: Option<ChunkSettings> = None;
            // Places to put our stuff
            let mut index = HashMap::new();
            let mut length_map = HashMap::new();
            let mut manifest = Vec::new();
            let mut chunk_headers = HashMap::new();
            // Parse all the headers and footers
            while entry_header.footer_offset != 0 && entry_header.next_header_offset != 0 {
                // Read the associated footer
                file.seek(SeekFrom::Start(entry_header.footer_offset))?;
                let footer = EntryFooter::from_read(&mut file)?.into_data(&key)?;
                // Update the chunk settings
                chunk_settings = Some(footer.chunk_settings);
                // Parse the chunk locations into segment descriptors
                for (id, start, length) in footer.chunk_locations {
                    let descriptor = SegmentDescriptor {
                        segment_id: 0,
                        start,
                    };
                    // load that into our index
                    index.insert(id, descriptor);
                    // load it into our length map
                    length_map.insert(descriptor, length);
                    // load the header and put it into the map
                    // TODO: move out of the map instead of clone
                    let header = footer
                        .chunk_headers
                        .get(&id)
                        .ok_or_else(|| {
                            BackendError::IndexError(format!(
                                "Chunk with id {:?} did not have an associated header.",
                                id
                            ))
                        })?
                        .clone();
                    chunk_headers.insert(descriptor, header);
                }

                // Load any archives
                for (id, timestamp) in footer.archives {
                    // Temporary hack, the name field is pending removal
                    manifest.push(StoredArchive {
                        id,
                        name: "".to_string(),
                        timestamp,
                    });
                }

                // Load up the next header
                header_offset = file.seek(SeekFrom::Start(entry_header.next_header_offset))?;
                entry_header = EntryHeader::from_read(&mut file)?;
            }
            // If we haven't set chunk settings yet, we have an invalid repository
            let chunk_settings = chunk_settings.ok_or_else(|| {
                BackendError::ManifestError(format!(
                    "FlatFile repository at {:?} did not contain any valid entries",
                    path
                ))
            })?;

            let flat_file = GenericFlatFile {
                file,
                path,
                chunk_settings,
                index,
                length_map,
                manifest,
                entry_footer_data: EntryFooterData::new(chunk_settings),
                chunk_settings_modified: false,
                enc_key,
                key,
                chunk_headers,
                header_offset,
            };

            Ok(flat_file)
        }
    }

    /// Attempts to read an `EncryptedKey` from the header of the provided repository
    /// file
    ///
    /// # Errors
    ///
    /// - If an underlying I/O error occurs
    /// - If decoding the `EncryptedKey` fails
    pub fn load_encrypted_key(mut file: F) -> Result<EncryptedKey> {
        file.seek(SeekFrom::Start(0))?;
        let header = FlatFileHeader::from_read(&mut file)?;
        Ok(header.key()?)
    }
}

impl<F: Read + Write + Seek + 'static> SyncManifest for GenericFlatFile<F> {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    /// Assumes archives were written in chronological order, and returns the timestamp
    /// of the last archive written.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there are no archives in this repository
    fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        if self.manifest.is_empty() {
            Err(BackendError::ManifestError(
                "No archives/timestamps present".to_string(),
            ))
        } else {
            let archive = &self.manifest[self.manifest.len() - 1];
            Ok(archive.timestamp())
        }
    }
    /// Returns the cached `ChunkSettings` stored in the struct
    fn chunk_settings(&mut self) -> ChunkSettings {
        self.chunk_settings
    }
    /// Modifies the cached `ChunkSettings`, as well as the ones in the
    /// `EntryFooterData`. Additionally sets the dirty flag on the chunk settings, so if
    /// only the chunk settings were modified, this change will still get persisted to
    /// the repository.
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.chunk_settings = settings;
        self.entry_footer_data.chunk_settings = settings;
        self.chunk_settings_modified = true;
        Ok(())
    }
    /// Clones the cached `manifest` `Vec` and turns it into an iterator
    fn archive_iterator(&mut self) -> Self::Iterator {
        self.manifest.clone().into_iter()
    }
    /// Adds the archive to the cached `manifest` `Vec`, as well as to the `EntryFooterData`
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        self.entry_footer_data
            .add_archive(archive.id, archive.timestamp);
        self.manifest.push(archive);
        Ok(())
    }
    /// This repository type does not support touching, so this does nothing
    fn touch(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<F: Read + Write + Seek + 'static> SyncIndex for GenericFlatFile<F> {
    /// Simply looks up the chunk in the cached `index` map
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.index.get(&id).copied()
    }
    /// Updates the cached `index` map, as well as adds the chunk to the
    /// `EntryFooterData`
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `Chunk` had not been previously written with
    /// `write_chunk`, and thus has an unknown length.
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        let length = self.length_map.get(&location).ok_or_else(|| {
            BackendError::IndexError(format!(
                "Attempted to add chunk with id {:?} to the index, whose length was not known",
                id
            ))
        })?;
        self.index.insert(id, location);
        let location = location.start;
        self.entry_footer_data.add_chunk(id, location, *length);
        Ok(())
    }
    /// Collects the keys from the cached `index` map into a `HashSet`
    fn known_chunks(&mut self) -> HashSet<ChunkID> {
        self.index.keys().copied().collect()
    }
    /// Flush the `EntryFooterDisk` to disk and make a new one
    fn commit_index(&mut self) -> Result<()> {
        // First check and see if we need to do anything
        if self.chunk_settings_modified || self.entry_footer_data.dirty() {
            // Reset the chunk_settings_modified flag
            self.chunk_settings_modified = false;
            // Make a new footer and swap it out
            let mut footer = EntryFooterData::new(self.chunk_settings);
            std::mem::swap(&mut self.entry_footer_data, &mut footer);
            // Pack the footer up
            let footer = EntryFooter::from_data(&footer, &self.key, self.chunk_settings);
            // seek to the end of the file
            let file = &mut self.file;
            let footer_location = file.seek(SeekFrom::End(0))?;
            // Write the footer
            footer.to_write(Write::by_ref(file))?;
            // Write a new, blank header
            let header_location = file.seek(SeekFrom::End(0))?;
            EntryHeader::new(&*crate::VERSION_STRUCT, 0, 0, *crate::IMPLEMENTATION_UUID)?
                .to_write(Write::by_ref(file))?;
            // Go back and update the previous header
            file.seek(SeekFrom::Start(self.header_offset))?;
            EntryHeader::new(
                &*crate::VERSION_STRUCT,
                footer_location,
                header_location,
                *crate::IMPLEMENTATION_UUID,
            )?
            .to_write(Write::by_ref(file))?;
            // Update our bookkeeping
            self.header_offset = header_location;

            // All done
            Ok(())
        } else {
            // Noting to do, just return OK
            Ok(())
        }
    }
    /// Returns the size of the cached `index` map
    fn chunk_count(&mut self) -> usize {
        self.index.len()
    }
}

impl<F: Read + Write + Seek + 'static> SyncBackend for GenericFlatFile<F> {
    type SyncManifest = Self;
    type SyncIndex = Self;
    fn get_index(&mut self) -> &mut Self::SyncIndex {
        self
    }
    fn get_manifest(&mut self) -> &mut Self::SyncManifest {
        self
    }
    /// This operation is not currently supported for `FlatFile` repositories, so we just return an
    /// error
    fn write_key(&mut self, _key: EncryptedKey) -> Result<()> {
        Err(BackendError::Unknown(
            "Changing the key of a FlatFile repository is not supported at this time.".to_string(),
        ))
    }
    /// Return the cached `EncryptedKey`
    fn read_key(&mut self) -> Result<EncryptedKey> {
        Ok(self.enc_key.clone())
    }
    /// Glue together information from the cached `index`, the `length_map`, and the
    /// `chunk_headers` map to find the chunk in the file and reconstruct it.
    ///
    /// # Errors
    ///
    /// - If there is an underlying I/O error
    /// - If the location is not present in the length map (the chunk has not been seen
    ///   before, or it has not been written with `write_chunk`
    /// - If the header is not present in the `chunk_headers` map
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        // Find the start of its chunk, and lookup its length
        let start = location.start;
        let length = *self.length_map.get(&location).ok_or_else(|| {
            BackendError::SegmentError(format!(
                "Attempted to look up chunk with location {:?}, but its length was not known",
                location
            ))
        })?;
        // Seek to the start of the chunk
        let file = &mut self.file;
        file.seek(SeekFrom::Start(start))?;
        // Allocate a buffer to read the bytes into
        let buffer_len: usize = length
            .try_into()
            .expect("Attempted to read a chunk that could not possibly fit into memory");
        let mut buffer = vec![0_u8; buffer_len];
        // Read the body of the chunk
        file.read_exact(&mut buffer[..])?;
        // Find the chunk header
        let header = self
            .chunk_headers
            .get(&location)
            .ok_or_else(|| {
                BackendError::SegmentError(format!(
                    "Attempted to look up chunk with location {:?},\
                 but there was no associated chunk header",
                    location
                ))
            })?
            .clone();
        // Recombine the chunk
        let chunk = Chunk::unsplit(header, ChunkBody(buffer));
        Ok(chunk)
    }
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        let id = chunk.get_id();
        // Seek to the end of the file and record that location
        let file = &mut self.file;
        let location = file.seek(SeekFrom::End(0))?;
        // Split the chunk into its header and body
        let (header, body) = chunk.split();
        // Get the length of the chunk
        let length = body.0.len() as u64;
        // Build the descriptor
        let descriptor = SegmentDescriptor {
            segment_id: 0,
            start: location,
        };
        // Add it to the length map
        self.length_map.insert(descriptor, length);
        // Add the chunk to the location map in the EntryFooterData
        self.entry_footer_data.add_chunk(id, location, length);
        // Add the header to the EntryFooterData and the headers map
        self.entry_footer_data.add_header(id, header.clone());
        self.chunk_headers.insert(descriptor, header);
        // Write the chunk to the file
        file.write_all(&body.0[..])?;

        Ok(descriptor)
    }
}

impl<T: Read + Write + Seek + 'static> Drop for GenericFlatFile<T> {
    fn drop(&mut self) {
        // Attempt to commit the index before dropping, if committing fails, panic
        let res = self.commit_index();
        if res.is_err() && !std::thread::panicking() {
            panic!(
                "Failed to commit index during drop. Path was {:?}",
                self.path
            )
        }
    }
}
