use crate::repository::{Chunk, ChunkID, Compression, Encryption, Key, HMAC};

use futures_intrusive::channel::shared::{channel, oneshot_channel, OneshotSender, Sender};
use tokio::task;
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
}

impl Pipeline {
    /// Spawns a new pipeline and populates it with a number of tasks
    pub fn new(task_count: usize) -> Pipeline {
        // A hacky approximation for the depth of the queue used
        // roughly 1.5 times the number of tasks used, plus one extra to make sure its not zero
        let queue_depth = (task_count * 3) / 2 + 1;
        let (input, rx) = channel(queue_depth);

        for _ in 0..task_count {
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
                            // If sending to this channel fails, we have no way to communicate to
                            // the outside anymore. Just let this task die.
                            ret_id.send(c.get_id()).unwrap();
                        }
                        // If sending to this channel fails, we have no way to communicate to
                        // the outside anymore. Just let this task die.
                        message.ret_chunk.send(c).unwrap();
                    });
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
        input
            .send((data, message))
            .await
            .expect("Not able to communicate with processing tasks. Unable to recover.");
        (
            id_rx
                .receive()
                .await
                .expect("Not able to communicate with processing tasks. Unable to recover."),
            c_rx.receive()
                .await
                .expect("Not able to communicate with processing tasks. Unable to recover."),
        )
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(num_cpus::get_physical())
    }
}
