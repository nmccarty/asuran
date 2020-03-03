/*!
This module contains a 'dumb' warapper enum around the types of repository
backends that `asuran-cli` currently supports.
*/

use asuran::repository::backend::common::sync_backend::BackendHandle;
use asuran::repository::backend::flatfile::FlatFile;
use asuran::repository::backend::multifile::MultiFile;
use asuran::repository::backend::*;
use asuran::repository::{Chunk, ChunkID};

use async_trait::async_trait;
use std::collections::HashSet;
#[derive(Debug, Clone)]
pub enum DynamicIndex {
    MultiFile(<MultiFile as Backend>::Index),
    FlatFile(<BackendHandle<FlatFile> as Backend>::Index),
}
#[derive(Debug, Clone)]
pub enum DynamicManifest {
    MultiFile(<MultiFile as Backend>::Manifest),
    FlatFile(<BackendHandle<FlatFile> as Backend>::Manifest),
}
#[derive(Debug, Clone)]
pub enum DynamicBackend {
    MultiFile(MultiFile),
    FlatFile(BackendHandle<FlatFile>),
}

#[async_trait]
impl Index for DynamicIndex {
    async fn lookup_chunk(&mut self, id: asuran::repository::ChunkID) -> Option<SegmentDescriptor> {
        match self {
            DynamicIndex::MultiFile(x) => x.lookup_chunk(id).await,
            DynamicIndex::FlatFile(x) => x.lookup_chunk(id).await,
        }
    }
    async fn set_chunk(
        &mut self,
        id: asuran::repository::ChunkID,
        location: SegmentDescriptor,
    ) -> Result<()> {
        match self {
            DynamicIndex::MultiFile(x) => x.set_chunk(id, location).await,
            DynamicIndex::FlatFile(x) => x.set_chunk(id, location).await,
        }
    }
    async fn commit_index(&mut self) -> Result<()> {
        match self {
            DynamicIndex::MultiFile(x) => x.commit_index().await,
            DynamicIndex::FlatFile(x) => x.commit_index().await,
        }
    }
    async fn count_chunk(&mut self) -> usize {
        match self {
            DynamicIndex::MultiFile(x) => x.count_chunk().await,
            DynamicIndex::FlatFile(x) => x.count_chunk().await,
        }
    }
    async fn known_chunks(&mut self) -> HashSet<ChunkID> {
        match self {
            DynamicIndex::MultiFile(x) => x.known_chunks().await,
            DynamicIndex::FlatFile(x) => x.known_chunks().await,
        }
    }
}

#[async_trait]
impl Manifest for DynamicManifest {
    type Iterator = Box<dyn Iterator<Item = asuran::manifest::StoredArchive>>;
    async fn last_modification(&mut self) -> Result<chrono::DateTime<chrono::FixedOffset>> {
        match self {
            DynamicManifest::MultiFile(x) => x.last_modification().await,
            DynamicManifest::FlatFile(x) => x.last_modification().await,
        }
    }
    async fn chunk_settings(&mut self) -> asuran::repository::ChunkSettings {
        match self {
            DynamicManifest::MultiFile(x) => x.chunk_settings().await,
            DynamicManifest::FlatFile(x) => x.chunk_settings().await,
        }
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        match self {
            DynamicManifest::MultiFile(x) => Box::new(x.archive_iterator().await),
            DynamicManifest::FlatFile(x) => Box::new(x.archive_iterator().await),
        }
    }
    async fn write_chunk_settings(
        &mut self,
        settings: asuran::repository::ChunkSettings,
    ) -> Result<()> {
        match self {
            DynamicManifest::MultiFile(x) => x.write_chunk_settings(settings).await,
            DynamicManifest::FlatFile(x) => x.write_chunk_settings(settings).await,
        }
    }
    async fn write_archive(&mut self, archive: asuran::manifest::StoredArchive) -> Result<()> {
        match self {
            DynamicManifest::MultiFile(x) => x.write_archive(archive).await,
            DynamicManifest::FlatFile(x) => x.write_archive(archive).await,
        }
    }
    async fn touch(&mut self) -> Result<()> {
        match self {
            DynamicManifest::MultiFile(x) => x.touch().await,
            DynamicManifest::FlatFile(x) => x.touch().await,
        }
    }
}

#[async_trait]
impl Backend for DynamicBackend {
    type Manifest = DynamicManifest;
    type Index = DynamicIndex;
    fn get_index(&self) -> Self::Index {
        match self {
            DynamicBackend::MultiFile(x) => DynamicIndex::MultiFile(x.get_index()),
            DynamicBackend::FlatFile(x) => DynamicIndex::FlatFile(x.get_index()),
        }
    }
    async fn write_key(&self, key: &asuran::repository::EncryptedKey) -> Result<()> {
        match self {
            DynamicBackend::MultiFile(x) => x.write_key(key).await,
            DynamicBackend::FlatFile(x) => x.write_key(key).await,
        }
    }
    async fn read_key(&self) -> Result<asuran::repository::EncryptedKey> {
        match self {
            DynamicBackend::MultiFile(x) => x.read_key().await,
            DynamicBackend::FlatFile(x) => x.read_key().await,
        }
    }
    fn get_manifest(&self) -> Self::Manifest {
        match self {
            DynamicBackend::MultiFile(x) => DynamicManifest::MultiFile(x.get_manifest()),
            DynamicBackend::FlatFile(x) => DynamicManifest::FlatFile(x.get_manifest()),
        }
    }
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        match self {
            DynamicBackend::MultiFile(x) => x.read_chunk(location).await,
            DynamicBackend::FlatFile(x) => x.read_chunk(location).await,
        }
    }
    async fn write_chunk(
        &mut self,
        chunk: Chunk,
        id: asuran::repository::ChunkID,
    ) -> Result<SegmentDescriptor> {
        match self {
            DynamicBackend::MultiFile(x) => x.write_chunk(chunk, id).await,
            DynamicBackend::FlatFile(x) => x.write_chunk(chunk, id).await,
        }
    }
    async fn close(self) {
        match self {
            DynamicBackend::MultiFile(x) => x.close().await,
            DynamicBackend::FlatFile(x) => x.close().await,
        }
    }
}

impl From<MultiFile> for DynamicBackend {
    fn from(backend: MultiFile) -> Self {
        DynamicBackend::MultiFile(backend)
    }
}

impl From<BackendHandle<FlatFile>> for DynamicBackend {
    fn from(backend: BackendHandle<FlatFile>) -> Self {
        DynamicBackend::FlatFile(backend)
    }
}
