use std::collections::HashMap;

use bytes::Bytes;
use tokio::sync::mpsc;
use uuid::Uuid;

pub type BodyChunk = Result<Bytes, std::io::Error>;
pub type BodySender = mpsc::Sender<BodyChunk>;
pub type BodyReceiver = mpsc::Receiver<BodyChunk>;

#[derive(Default)]
pub struct Inflight {
    map: HashMap<Uuid, BodySender>,
}

impl Inflight {
    pub fn insert(&mut self, request_id: Uuid, tx: BodySender) {
        self.map.insert(request_id, tx);
    }

    pub fn get(&self, request_id: &Uuid) -> Option<BodySender> {
        self.map.get(request_id).cloned()
    }

    pub fn remove(&mut self, request_id: &Uuid) {
        self.map.remove(request_id);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}
