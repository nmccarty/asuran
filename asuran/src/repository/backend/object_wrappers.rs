#![allow(clippy::wildcard_imports)]
use super::*;

pub type ManifestObject =
    Box<dyn Manifest<Iterator = Box<dyn Iterator<Item = StoredArchive> + 'static>> + 'static>;
pub type IndexObject = Box<dyn Index + 'static>;
pub type BackendObject = Box<dyn Backend<Index = IndexObject, Manifest = ManifestObject>>;
pub type BackendObjectRef<'a> = &'a dyn Backend<Index = IndexObject, Manifest = ManifestObject>;

/// Wraps a manifest in an object safe way
#[derive(Debug)]
pub struct ManifestWrapper<T: Manifest>(T);

#[async_trait]
impl<T: Manifest> Manifest for ManifestWrapper<T> {
    type Iterator = Box<dyn Iterator<Item = StoredArchive> + 'static>;
    async fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        self.0.last_modification().await
    }
    async fn chunk_settings(&mut self) -> ChunkSettings {
        self.0.chunk_settings().await
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        Box::new(self.0.archive_iterator().await)
    }
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.0.write_chunk_settings(settings).await
    }
    async fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        self.0.write_archive(archive).await
    }
    async fn touch(&mut self) -> Result<()> {
        self.0.touch().await
    }
}

#[async_trait]
impl Manifest for ManifestObject {
    type Iterator = Box<dyn Iterator<Item = StoredArchive> + 'static>;
    async fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        self.last_modification().await
    }
    async fn chunk_settings(&mut self) -> ChunkSettings {
        self.chunk_settings().await
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        self.archive_iterator().await
    }
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.write_chunk_settings(settings).await
    }
    async fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        self.write_archive(archive).await
    }
    async fn touch(&mut self) -> Result<()> {
        self.touch().await
    }
}

#[async_trait]
impl Index for IndexObject {
    async fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.lookup_chunk(id).await
    }
    async fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        self.set_chunk(id, location).await
    }
    async fn known_chunks(&mut self) -> HashSet<ChunkID> {
        self.known_chunks().await
    }
    async fn commit_index(&mut self) -> Result<()> {
        self.commit_index().await
    }
    async fn count_chunk(&mut self) -> usize {
        self.count_chunk().await
    }
}

/// Wraps a Backend in an object safe way
#[derive(Debug, Clone)]
pub struct BackendWrapper<T: Backend>(T);

#[async_trait]
impl<T: Backend> Backend for BackendWrapper<T> {
    type Manifest = ManifestObject;
    type Index = IndexObject;
    fn get_index(&self) -> Self::Index {
        Box::new(self.0.get_index())
    }
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        self.0.write_key(key).await
    }
    async fn read_key(&self) -> Result<EncryptedKey> {
        self.0.read_key().await
    }
    fn get_manifest(&self) -> Self::Manifest {
        Box::new(ManifestWrapper(self.0.get_manifest()))
    }
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.0.read_chunk(location).await
    }
    async fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        self.0.write_chunk(chunk).await
    }
    async fn close(&mut self) {
        self.0.close().await
    }
    fn get_object_handle(&self) -> BackendObject {
        self.0.get_object_handle()
    }
}

#[async_trait]
impl Backend for BackendObject {
    type Manifest = ManifestObject;
    type Index = IndexObject;
    fn get_index(&self) -> Self::Index {
        let x: BackendObjectRef = &*self;
        x.get_index()
    }
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        self.write_key(key).await
    }
    async fn read_key(&self) -> Result<EncryptedKey> {
        self.read_key().await
    }
    fn get_manifest(&self) -> Self::Manifest {
        let x: BackendObjectRef = &*self;
        x.get_manifest()
    }
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.read_chunk(location).await
    }
    async fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        self.write_chunk(chunk).await
    }
    async fn close(&mut self) {
        self.close().await
    }
    fn get_object_handle(&self) -> BackendObject {
        let x: BackendObjectRef = &*self;
        x.get_object_handle()
    }
}

/// Consumes a `Backend` and converts it into a `BackendObject`
pub fn backend_to_object<T: Backend>(backend: T) -> BackendObject {
    Box::new(BackendWrapper(backend))
}

impl Clone for BackendObject {
    fn clone(&self) -> Self {
        self.get_object_handle()
    }
}
