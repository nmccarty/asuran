use futures_intrusive::channel::shared::*;
use num_cpus;
use tokio::task;

use crate::repository::{Chunk, ChunkID, Compression, Encryption, Key, HMAC};

use tracing::instrument;

#[derive(Debug)]
struct Message {
    compression: Compression,
    encryption: Encryption,
    hmac: HMAC,
    key: Key,
    ret_chunk: OneshotSender<Chunk>,
    ret_id: Option<OneshotSender<ChunkID>>,
}

#[derive(Clone)]
pub struct Pipeline {
    input: Sender<(Vec<u8>, Message)>,
    input_id: Sender<(ChunkID, Vec<u8>, Message)>,
}

impl Pipeline {
    /// Spawns a new pipeline and populates it with a number of tasks
    pub fn new() -> Pipeline {
        let base_threads = num_cpus::get();

        let (input, rx) = channel(50);
        let (input_id, id_rx) = channel(50);

        for _ in 0..base_threads {
            let rx = rx.clone();
            task::spawn(async move {
                while let Some(input) = rx.receive().await {
                    let (chunk, message): (Vec<u8>, Message) = input;
                    task::block_in_place(|| {
                        let c = Chunk::pack(
                            chunk,
                            message.compression,
                            message.encryption,
                            message.hmac,
                            &message.key,
                        );
                        if let Some(ret_id) = message.ret_id {
                            ret_id.send(c.get_id()).unwrap();
                        }
                        message.ret_chunk.send(c).unwrap();
                    });
                }
            });
        }

        for _ in 0..base_threads {
            let id_rx = id_rx.clone();
            task::spawn(async move {
                while let Some(input) = id_rx.receive().await {
                    let (id, chunk, message): (ChunkID, Vec<u8>, Message) = input;
                    task::block_in_place(|| {
                        let c = Chunk::pack_with_id(
                            chunk,
                            message.compression,
                            message.encryption,
                            message.hmac,
                            &message.key,
                            id,
                        );
                        if let Some(ret_id) = message.ret_id {
                            ret_id.send(c.get_id()).unwrap();
                        }
                        message.ret_chunk.send(c).unwrap();
                    });
                }
            });
        }

        Pipeline { input, input_id }
    }

    #[instrument(skip(self, data))]
    pub async fn process(
        &self,
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> (ChunkID, Chunk) {
        let (c_tx, c_rx) = oneshot_channel();
        let (id_tx, id_rx) = oneshot_channel();
        let message = Message {
            compression,
            encryption,
            hmac,
            key,
            ret_chunk: c_tx,
            ret_id: Some(id_tx),
        };
        let input = self.input.clone();
        input.send((data, message)).await.unwrap();
        (
            id_rx.receive().await.unwrap(),
            c_rx.receive().await.unwrap(),
        )
    }

    #[instrument(skip(self, data))]
    pub async fn process_with_id(
        &self,
        data: Vec<u8>,
        id: ChunkID,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> Chunk {
        let (c_tx, c_rx) = oneshot_channel();
        let message = Message {
            compression,
            encryption,
            hmac,
            key,
            ret_chunk: c_tx,
            ret_id: None,
        };
        let input = self.input_id.clone();
        input.send((id, data, message)).await.unwrap();
        c_rx.receive().await.unwrap()
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
