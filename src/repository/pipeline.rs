use futures::executor::ThreadPool;
use futures_intrusive::channel::shared::*;

use crate::repository::{Chunk, ChunkID, Compression, Encryption, Key, HMAC};

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
    pool: ThreadPool,
    input: Sender<(Vec<u8>, Message)>,
    input_id: Sender<(ChunkID, Vec<u8>, Message)>,
}

impl Pipeline {
    pub fn new(pool: ThreadPool) -> Pipeline {
        let id_count = 4;
        let compress_count = 4;
        let encryption_count = 6;
        let mac_count = 4;
        let (input, id_rx) = channel(100);
        let (id_tx, compress_rx) = channel(100);
        let input_id = id_tx.clone();
        let (compress_tx, enc_rx) = channel(100);
        let (enc_tx, mac_rx) = channel(100);

        for _ in 0..id_count {
            // ID stage
            let id_rx = id_rx.clone();
            let id_tx = id_tx.clone();
            pool.spawn_ok(async move {
                while let Some(input) = id_rx.receive().await {
                    let (data, mut message): (Vec<u8>, Message) = input;
                    let id = message.hmac.id(&data[..], &message.key);
                    let cid = ChunkID::new(&id[..]);
                    message.ret_id.take().unwrap().send(cid).unwrap();
                    id_tx.send((cid, data, message)).await.unwrap();
                }
            });
        }

        for _ in 0..compress_count {
            let compress_rx = compress_rx.clone();
            let compress_tx = compress_tx.clone();
            // Compression stage
            pool.spawn_ok(async move {
                while let Some(input) = compress_rx.receive().await {
                    let (cid, data, message) = input;
                    let cdata = message.compression.compress(data);
                    compress_tx.send((cid, cdata, message)).await.unwrap();
                }
            });
        }

        // Encryption stage
        for _ in 0..encryption_count {
            let enc_rx = enc_rx.clone();
            let enc_tx = enc_tx.clone();
            pool.spawn_ok(async move {
                while let Some(input) = enc_rx.receive().await {
                    let (cid, data, message) = input;
                    let edata = message.encryption.encrypt(&data[..], &message.key);
                    enc_tx.send((cid, edata, message)).await.unwrap();
                }
            });
        }

        // Mac stage
        for _ in 0..mac_count {
            let mac_rx = mac_rx.clone();
            pool.spawn_ok(async move {
                while let Some(input) = mac_rx.receive().await {
                    let (cid, data, message) = input;
                    let mac = message.hmac.mac(&data, &message.key);
                    let chunk = Chunk::from_parts(
                        data,
                        message.compression,
                        message.encryption,
                        message.hmac,
                        mac,
                        cid,
                    );
                    message.ret_chunk.send(chunk).unwrap();
                }
            });
        }

        Pipeline {
            pool,
            input,
            input_id,
        }
    }

    pub async fn process(
        &self,
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> (OneshotReceiver<ChunkID>, OneshotReceiver<Chunk>) {
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
        (id_rx, c_rx)
    }

    pub async fn process_with_id(
        &self,
        data: Vec<u8>,
        id: ChunkID,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> OneshotReceiver<Chunk> {
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
        c_rx
    }
}
