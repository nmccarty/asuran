use crate::repository::{Chunk, Compression, Encryption, Key, HMAC};

use futures::channel::oneshot;
use smol::block_on;
use std::thread;
use tracing::instrument;

#[derive(Debug)]
struct Message {
    compression: Compression,
    encryption: Encryption,
    hmac: HMAC,
    key: Key,
    ret_chunk: oneshot::Sender<Chunk>,
}

#[derive(Clone)]
pub struct Pipeline {
    input: async_channel::Sender<(Vec<u8>, Message)>,
}

impl Pipeline {
    /// Spawns a new pipeline and populates it with a number of tasks
    pub fn new(task_count: usize) -> Pipeline {
        // A hacky approximation for the depth of the queue used
        // roughly 1.5 times the number of tasks used, plus one extra to make sure its not zero
        let queue_depth = (task_count * 3) / 2 + 1;
        let (input, rx) = async_channel::bounded(queue_depth);

        for _ in 0..task_count {
            let rx = rx.clone();
            thread::spawn(move || {
                while let Ok(input) = block_on(rx.recv()) {
                    let (chunk, message): (Vec<u8>, Message) = input;
                    let c = Chunk::pack(
                        chunk,
                        message.compression,
                        message.encryption,
                        message.hmac,
                        &message.key,
                    );
                    // If sending to this channel fails, we have no way to communicate to
                    // the outside anymore. Just let this task die.
                    message.ret_chunk.send(c).unwrap();
                }
            });
        }
        Pipeline { input }
    }

    #[instrument(skip(self, data))]
    pub async fn process(
        &self,
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: Key,
    ) -> Chunk {
        let (c_tx, c_rx) = oneshot::channel();
        let message = Message {
            compression,
            encryption,
            hmac,
            key,
            ret_chunk: c_tx,
        };
        let input = self.input.clone();
        input
            .send((data, message))
            .await
            .expect("Sending to processing thread failed");

        c_rx.await
            .expect("Not able to communicate with processing tasks. Unable to recover.")
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(num_cpus::get_physical())
    }
}
