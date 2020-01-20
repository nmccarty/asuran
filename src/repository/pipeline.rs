use futures_intrusive::channel::shared::*;
use num_cpus;
use tokio::task;

use crate::repository::{Chunk, ChunkID, Compression, Encryption, Key, HMAC};

#[derive(Debug)]
struct Message {
    compression: Compression,
    encryption: Encryption,
    hmac: HMAC,
    key: Key,
    ret_chunk: OneshotSender<Vec<Chunk>>,
    ret_id: Option<OneshotSender<Vec<ChunkID>>>,
}

#[derive(Clone)]
pub struct Pipeline {
    input: Sender<(Vec<Vec<u8>>, Message)>,
    input_id: Sender<(Vec<ChunkID>, Vec<Vec<u8>>, Message)>,
}

impl Pipeline {
    /// Spawns a new pipeline and populates it with a number of tasks
    ///
    /// Gets a pass on too_many lines for now
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Pipeline {
        let base_threads = match num_cpus::get() / 2 {
            0 => 1,
            x => x,
        };
        let heavy_count = base_threads;
        let light_count = base_threads;

        let (input, id_rx) = channel(50);
        let (id_tx, compress_rx) = channel(50);
        let input_id = id_tx.clone();
        let (compress_tx, enc_rx) = channel(50);
        let (enc_tx, mac_rx) = channel(50);

        for _ in 0..light_count {
            // ID stage
            let id_rx = id_rx.clone();
            let id_tx = id_tx.clone();
            let compress_tx = compress_tx.clone();
            let enc_tx = enc_tx.clone();
            task::spawn(async move {
                while let Some(input) = id_rx.receive().await {
                    let (data, mut message): (Vec<Vec<u8>>, Message) = input;
                    let mut cids = Vec::new();
                    for chunk in &data {
                        let id = message.hmac.id(&chunk[..], &message.key);
                        cids.push(ChunkID::new(&id[..]));
                    }
                    // Go ahead and send the chunkIDs
                    message.ret_id.take().unwrap().send(cids.clone()).unwrap();
                    let compression = message.compression;
                    let encryption = message.encryption;
                    let next = (cids, data, message);
                    // Skip to the appropiate stage if compression is disabled
                    if compression == Compression::NoCompression {
                        // If encryption is also disabled, skip straight to HMAC
                        if encryption == Encryption::NoEncryption {
                            enc_tx.send(next).await.unwrap();
                        } else {
                            // Otherwise just skip compression
                            compress_tx.send(next).await.unwrap();
                        }
                    } else {
                        id_tx.send(next).await.unwrap();
                    }
                }
            });
        }

        for _ in 0..heavy_count {
            let compress_rx = compress_rx.clone();
            let compress_tx = compress_tx.clone();
            let enc_tx = enc_tx.clone();
            // Compression stage
            task::spawn(async move {
                while let Some(input) = compress_rx.receive().await {
                    let (cids, data, message) = input;
                    let mut cdatas = Vec::new();
                    for chunk in data {
                        let cdata = message.compression.compress(chunk);
                        cdatas.push(cdata);
                    }
                    let encryption = message.encryption;
                    let next = (cids, cdatas, message);
                    // If encryption is disabled, skip that stage
                    if encryption == Encryption::NoEncryption {
                        enc_tx.send(next).await.unwrap();
                    } else {
                        compress_tx.send(next).await.unwrap();
                    }
                }
            });
        }

        // Encryption stage
        for _ in 0..heavy_count {
            let enc_rx = enc_rx.clone();
            let enc_tx = enc_tx.clone();
            task::spawn(async move {
                while let Some(input) = enc_rx.receive().await {
                    let (cids, data, message) = input;
                    let mut edatas = Vec::new();
                    for chunk in data {
                        let edata = message.encryption.encrypt(&chunk[..], &message.key);
                        edatas.push(edata);
                    }
                    enc_tx.send((cids, edatas, message)).await.unwrap();
                }
            });
        }

        // Mac stage
        for _ in 0..light_count {
            let mac_rx = mac_rx.clone();
            task::spawn(async move {
                while let Some(input) = mac_rx.receive().await {
                    let (cids, data, message) = input;
                    let mut chunks = Vec::new();
                    for (index, chunk) in data.into_iter().enumerate() {
                        let mac = message.hmac.mac(&chunk, &message.key);
                        let chunk = Chunk::from_parts(
                            chunk,
                            message.compression,
                            message.encryption,
                            message.hmac,
                            mac,
                            cids[index],
                        );
                        chunks.push(chunk)
                    }
                    message.ret_chunk.send(chunks).unwrap();
                }
            });
        }

        Pipeline { input, input_id }
    }

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
        input.send((vec![data], message)).await.unwrap();
        (
            id_rx.receive().await.unwrap()[0],
            c_rx.receive().await.unwrap().into_iter().next().unwrap(),
        )
    }

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
        input.send((vec![id], vec![data], message)).await.unwrap();
        c_rx.receive().await.unwrap().into_iter().next().unwrap()
    }

    pub async fn process_multiple(
        &self,
        data: Vec<Vec<u8>>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> Vec<Chunk> {
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
        id_rx.receive().await;
        c_rx.receive().await.unwrap()
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
