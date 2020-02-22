//! This module contains syncronous versions of the backend trait, as well as an abstraction for
//! implementing the main, async traits through holding the syncronous version in a task.
//!
//! This trait is not meant to be consumed directly by a user of this library.
//!
//! Implementors of this trait are required to be send (as the operations are handled on an async task),
//! however, they are not required to be sync.
//!
//! Addtionally, as only one direct consumer of these traits is expected to exist, the implementors are
//! not required to be `Clone`.
//!
//! Methods in this module are intentionally left undocumented, as they are indented to be syncronus
//! versions of their async equivlants in the main Backend traits.
use crate::manifest::StoredArchive;
use crate::repository::backend::{Backend, Index, Manifest, Result, SegmentDescriptor};
use crate::repository::{ChunkID, ChunkSettings, EncryptedKey};

use async_trait::async_trait;
use chrono::prelude::*;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use std::collections::HashSet;
use tokio::task;

pub trait SyncManifest: Send + std::fmt::Debug {
    type Iterator: Iterator<Item = StoredArchive> + std::fmt::Debug + Send + 'static;
    fn last_modification(&mut self) -> Result<DateTime<FixedOffset>>;
    fn chunk_settings(&mut self) -> ChunkSettings;
    fn archive_iterator(&mut self) -> Self::Iterator;
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()>;
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()>;
    fn touch(&mut self) -> Result<()>;
}

pub trait SyncIndex: Send + std::fmt::Debug {
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor>;
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()>;
    fn known_chunks(&mut self) -> HashSet<ChunkID>;
    fn commit_index(&mut self) -> Result<()>;
    fn chunk_count(&mut self) -> usize;
}

/// Note: In this version of the trait, the get index and get archive methods return mutable references,
/// instead of owned values. As this version of the trait is intrinsically single threaded, implementers
/// are expected to own a single instance of their Index and Manifest impls, and the reference will
/// never leak outside of their container task.
///
/// Also note, that we do not have the close method, as the wrapper type will handle that for us.
pub trait SyncBackend: 'static + Send + std::fmt::Debug {
    type SyncManifest: SyncManifest + 'static;
    type SyncIndex: SyncIndex + 'static;
    fn get_index(&mut self) -> &mut Self::SyncIndex;
    fn get_manifest(&mut self) -> &mut Self::SyncManifest;
    fn write_key(&mut self, key: EncryptedKey) -> Result<()>;
    fn read_key(&mut self) -> Result<EncryptedKey>;
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>>;
    fn write_chunk(&mut self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor>;
}

enum SyncIndexCommand {
    Lookup(ChunkID, oneshot::Sender<Option<SegmentDescriptor>>),
    Set(ChunkID, SegmentDescriptor, oneshot::Sender<Result<()>>),
    KnownChunks(oneshot::Sender<HashSet<ChunkID>>),
    Commit(oneshot::Sender<Result<()>>),
    Count(oneshot::Sender<usize>),
}

enum SyncManifestCommand<I> {
    LastMod(oneshot::Sender<Result<DateTime<FixedOffset>>>),
    ChunkSettings(oneshot::Sender<ChunkSettings>),
    ArchiveIterator(oneshot::Sender<I>),
    WriteChunkSettings(ChunkSettings, oneshot::Sender<Result<()>>),
    WriteArchive(StoredArchive, oneshot::Sender<Result<()>>),
    Touch(oneshot::Sender<Result<()>>),
}

enum SyncBackendCommand {
    ReadChunk(SegmentDescriptor, oneshot::Sender<Result<Vec<u8>>>),
    WriteChunk(Vec<u8>, ChunkID, oneshot::Sender<Result<SegmentDescriptor>>),
    ReadKey(oneshot::Sender<Result<EncryptedKey>>),
    WriteKey(EncryptedKey, oneshot::Sender<Result<()>>),
    Close(oneshot::Sender<()>),
}

enum SyncCommand<I> {
    Index(SyncIndexCommand),
    Manifest(SyncManifestCommand<I>),
    Backend(SyncBackendCommand),
}

/// Wrapper Type for sync backends that converts them into async backends
///
/// Functions by moving the provided back end into a dedicated tokio task, and then sending SyncCommands
/// to instruct that task on what to do.
pub struct BackendHandle<B: SyncBackend> {
    channel:
        mpsc::Sender<SyncCommand<<<B as SyncBackend>::SyncManifest as SyncManifest>::Iterator>>,
}

impl<B> BackendHandle<B>
where
    B: SyncBackend + Send + 'static,
{
    pub fn new(mut backend: B) -> Self {
        let (input, mut output) = mpsc::channel(100);
        task::spawn(async move {
            let mut final_ret: Option<oneshot::Sender<()>> = None;
            while let Some(command) = output.next().await {
                task::block_in_place(|| match command {
                    SyncCommand::Index(index_command) => {
                        let index = backend.get_index();
                        match index_command {
                            SyncIndexCommand::Lookup(id, ret) => {
                                ret.send(index.lookup_chunk(id)).unwrap();
                            }
                            SyncIndexCommand::Set(id, location, ret) => {
                                ret.send(index.set_chunk(id, location)).unwrap();
                            }
                            SyncIndexCommand::KnownChunks(ret) => {
                                ret.send(index.known_chunks()).unwrap();
                            }
                            SyncIndexCommand::Commit(ret) => {
                                ret.send(index.commit_index()).unwrap();
                            }
                            SyncIndexCommand::Count(ret) => {
                                ret.send(index.chunk_count()).unwrap();
                            }
                        };
                    }
                    SyncCommand::Manifest(manifest_command) => {
                        let manifest = backend.get_manifest();
                        match manifest_command {
                            SyncManifestCommand::LastMod(ret) => {
                                ret.send(manifest.last_modification()).unwrap();
                            }
                            SyncManifestCommand::ChunkSettings(ret) => {
                                ret.send(manifest.chunk_settings()).unwrap();
                            }
                            SyncManifestCommand::ArchiveIterator(ret) => {
                                ret.send(manifest.archive_iterator()).unwrap();
                            }
                            SyncManifestCommand::WriteChunkSettings(settings, ret) => {
                                ret.send(manifest.write_chunk_settings(settings)).unwrap();
                            }
                            SyncManifestCommand::WriteArchive(archive, ret) => {
                                ret.send(manifest.write_archive(archive)).unwrap();
                            }
                            SyncManifestCommand::Touch(ret) => {
                                ret.send(manifest.touch()).unwrap();
                            }
                        }
                    }
                    SyncCommand::Backend(backend_command) => match backend_command {
                        SyncBackendCommand::ReadChunk(location, ret) => {
                            ret.send(backend.read_chunk(location)).unwrap();
                        }
                        SyncBackendCommand::WriteChunk(chunk, id, ret) => {
                            ret.send(backend.write_chunk(chunk, id)).unwrap();
                        }
                        SyncBackendCommand::WriteKey(key, ret) => {
                            ret.send(backend.write_key(key)).unwrap();
                        }
                        SyncBackendCommand::ReadKey(ret) => {
                            ret.send(backend.read_key()).unwrap();
                        }
                        SyncBackendCommand::Close(ret) => {
                            final_ret = Some(ret);
                        }
                    },
                });
                if final_ret.is_some() {
                    break;
                }
            }
            std::mem::drop(backend);
            std::mem::drop(output);
            if let Some(ret) = final_ret {
                ret.send(()).unwrap();
            }
        });

        BackendHandle { channel: input }
    }
}

impl<B: SyncBackend> std::fmt::Debug for BackendHandle<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opaque Backend Handle")
    }
}

impl<B: SyncBackend> Clone for BackendHandle<B> {
    fn clone(&self) -> Self {
        BackendHandle {
            channel: self.channel.clone(),
        }
    }
}

#[async_trait]
impl<B: SyncBackend> Manifest for BackendHandle<B> {
    type Iterator = <<B as SyncBackend>::SyncManifest as SyncManifest>::Iterator;
    async fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(SyncManifestCommand::LastMod(i)))
            .await
            .unwrap();
        o.await?
    }
    async fn chunk_settings(&mut self) -> ChunkSettings {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(SyncManifestCommand::ChunkSettings(i)))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(SyncManifestCommand::ArchiveIterator(
                i,
            )))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(
                SyncManifestCommand::WriteChunkSettings(settings, i),
            ))
            .await
            .unwrap();
        o.await?
    }
    async fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(SyncManifestCommand::WriteArchive(
                archive, i,
            )))
            .await
            .unwrap();
        o.await?
    }
    async fn touch(&mut self) -> Result<()> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Manifest(SyncManifestCommand::Touch(i)))
            .await
            .unwrap();
        o.await?
    }
}

#[async_trait]
impl<B: SyncBackend> Index for BackendHandle<B> {
    async fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Index(SyncIndexCommand::Lookup(id, i)))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Index(SyncIndexCommand::Set(id, location, i)))
            .await
            .unwrap();
        o.await?
    }
    async fn known_chunks(&mut self) -> HashSet<ChunkID> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Index(SyncIndexCommand::KnownChunks(i)))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn commit_index(&mut self) -> Result<()> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Index(SyncIndexCommand::Commit(i)))
            .await
            .unwrap();
        o.await?
    }
    async fn count_chunk(&mut self) -> usize {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Index(SyncIndexCommand::Count(i)))
            .await
            .unwrap();
        o.await.unwrap()
    }
}

#[async_trait]
impl<B: SyncBackend> Backend for BackendHandle<B> {
    type Manifest = Self;
    type Index = Self;
    fn get_index(&self) -> Self::Index {
        self.clone()
    }
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        // We pull some jank here to access the channel without having to change the signature to
        // &mut self. This clone should be okay, performance wise, as it should only happen very
        // rarely
        let mut new_self = self.clone();
        let (i, o) = oneshot::channel();
        new_self
            .channel
            .send(SyncCommand::Backend(SyncBackendCommand::WriteKey(
                key.clone(),
                i,
            )))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn read_key(&self) -> Result<EncryptedKey> {
        // We pull some jank here to access the channel without having to change the signature to
        // &mut self. This clone should be okay, performance wise, as it should only happen very
        // rarely
        let mut new_self = self.clone();
        let (i, o) = oneshot::channel();
        new_self
            .channel
            .send(SyncCommand::Backend(SyncBackendCommand::ReadKey(i)))
            .await
            .unwrap();
        o.await?
    }
    fn get_manifest(&self) -> Self::Manifest {
        self.clone()
    }
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Backend(SyncBackendCommand::ReadChunk(
                location, i,
            )))
            .await
            .unwrap();
        o.await?
    }
    async fn write_chunk(&mut self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor> {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Backend(SyncBackendCommand::WriteChunk(
                chunk, id, i,
            )))
            .await
            .unwrap();
        o.await?
    }
    async fn close(mut self) {
        let (i, o) = oneshot::channel();
        self.channel
            .send(SyncCommand::Backend(SyncBackendCommand::Close(i)))
            .await
            .unwrap();
        o.await.unwrap()
    }
}
