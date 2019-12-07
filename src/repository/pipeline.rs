use futures::channel;
use futures::executor::ThreadPool;
use futures::sink::SinkExt;
use futures::stream::StreamExt;

use crate::repository::{Chunk, ChunkID, Compression, Encryption, Key, HMAC};

struct Message {
    compression: Compression,
    encryption: Encryption,
    hmac: HMAC,
    key: Key,
    ret_chunk: channel::oneshot::Sender<Chunk>,
    ret_id: Option<channel::oneshot::Sender<ChunkID>>,
}

#[derive(Clone)]
pub struct Pipeline {
    pool: ThreadPool,
    input: channel::mpsc::Sender<(Vec<u8>, Message)>,
}

impl Pipeline {
    pub fn new(pool: ThreadPool) -> Pipeline {
        let (input, mut id_rx) = channel::mpsc::channel(100);
        let (mut id_tx, mut compress_rx) = channel::mpsc::channel(100);
        let (mut compress_tx, mut enc_rx) = channel::mpsc::channel(100);
        let (mut enc_tx, mut mac_rx) = channel::mpsc::channel(100);
        // ID stage
        pool.spawn_ok(async move {
            while let Some(input) = id_rx.next().await {
                let (data, mut message): (Vec<u8>, Message) = input;
                let id = message.hmac.id(&data[..], &message.key);
                let cid = ChunkID::new(&id[..]);
                message.ret_id.take().unwrap().send(cid).unwrap();
                id_tx.send((cid, data, message)).await.unwrap();
            }
        });

        // Compression stage
        pool.spawn_ok(async move {
            while let Some(input) = compress_rx.next().await {
                let (cid, data, message) = input;
                let cdata = message.compression.compress(data);
                compress_tx.send((cid, cdata, message)).await.unwrap();
            }
        });

        // Encryption stage
        pool.spawn_ok(async move {
            while let Some(input) = enc_rx.next().await {
                let (cid, data, message) = input;
                let edata = message.encryption.encrypt(&data[..], &message.key);
                enc_tx.send((cid, edata, message)).await.unwrap();
            }
        });

        // Mac stage
        pool.spawn_ok(async move {
            while let Some(input) = mac_rx.next().await {
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

        Pipeline { pool, input }
    }

    pub async fn process(
        &self,
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> (
        channel::oneshot::Receiver<ChunkID>,
        channel::oneshot::Receiver<Chunk>,
    ) {
        let (c_tx, c_rx) = channel::oneshot::channel();
        let (id_tx, id_rx) = channel::oneshot::channel();
        let message = Message {
            compression,
            encryption,
            hmac,
            key,
            ret_chunk: c_tx,
            ret_id: Some(id_tx),
        };
        let mut input = self.input.clone();
        input.send((data, message)).await.unwrap();
        (id_rx, c_rx)
    }
}
