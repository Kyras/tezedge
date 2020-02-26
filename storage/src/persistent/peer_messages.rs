use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Instant;
use crossbeam::crossbeam_channel::{unbounded, Sender, Receiver};
use failure::Error;

use crate::persistent::{
    KeyValueStoreWithSchema, KeyValueSchema,
    database::IteratorWithSchema,
};
use tezos_messages::p2p::encoding::peer::PeerMessage;
use std::{
    mem::swap,
    thread::{JoinHandle, spawn},
};
use crate::IteratorMode;

pub type MessageDatabase = dyn KeyValueStoreWithSchema<PeerMessages> + Sync + Send;

/// Trait for recording messages
pub trait Recorder<K, V, E> {
    fn record(&mut self, msg: V) -> Result<Option<K>, E>;
}

/// Trait for replaying messages
/// TODO: Find out to resolve bad lifetime
pub trait Replayer<T> {
    type Iterator: Iterator<Item=T>;
    fn replay(&self) -> Result<Self::Iterator, Error>;
}

/// Storage responsible for storing network messages in precise
/// time sequence.
pub struct PeerMessages {
    db: Arc<MessageDatabase>,
    sender: Arc<Sender<(Nsec, PeerMessage)>>,
    sequencer: TimeLineSequencer,
    locked: AtomicBool,
    handle: Option<JoinHandle<()>>,
}

impl PeerMessages {
    /// Create new Peer Messages database
    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        let (sender, receiver) = unbounded();
        let tmp = db.clone();
        let handle = spawn(move || message_recording(tmp, receiver));

        Self {
            db,
            sender: Arc::new(sender),
            sequencer: TimeLineSequencer::new(),
            locked: AtomicBool::new(false),
            handle: Some(handle),
        }
    }

    /// Temporary solution to database lifetime problem
    pub fn replay(&self) -> Result<IteratorWithSchema<Self>, Error> {
        self.db.iterator(IteratorMode::<Self>::Start).map_err(Error::from)
    }
}

impl Recorder<Nsec, PeerMessage, Error> for PeerMessages {
    fn record(&mut self, msg: PeerMessage) -> Result<Option<Nsec>, Error> {
        if !self.locked.load(Ordering::SeqCst) {
            let ts = self.sequencer.new_ts();
            self.sender.send((ts, msg))
                .map_err(Error::from)?;
            Ok(Some(ts))
        } else {
            Ok(None)
        }
    }
}


/// Manual Drop to make sure, all data that potentially caused the failure of node are
/// correctly recorded.
impl Drop for PeerMessages {
    fn drop(&mut self) {
        if self.locked.compare_and_swap(false, true, Ordering::SeqCst) {
            while !self.sender.is_empty() { /*Wait for all remaining messages to be send to the writer thread*/ }
            let handle: Option<JoinHandle<()>> = None;
            swap(&mut self.handle, &mut None);
            if let Some(handle) = handle {
                let _ = handle.join(); // Wait for database to properly write the messages
            }
        }
    }
}

/// Recording thread to ensure as little latency as possible
fn message_recording(db: Arc<MessageDatabase>, recv: Receiver<(Nsec, PeerMessage)>) {
    for (k, v) in recv {
        let _ = db.put(&k, &v);
    }
}

/// Message storage schema, store message associated with nanosecond after start of the node.
impl KeyValueSchema for PeerMessages {
    type Key = Nsec;
    type Value = PeerMessage;

    fn name() -> &'static str {
        "peer_messages"
    }
}

/// Represents number of nanoseconds passed from some time event.
/// Is able to represent up to 584 years of time.
pub type Nsec = u64;

/// Struct responsible for managing as timestamp generation relative to some time point
#[derive(Debug, Copy, Clone, Hash)]
pub struct TimeLineSequencer {
    start: Instant,
}

impl TimeLineSequencer {
    /// Create new TimeLineSequencer
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }

    /// Generate new time-point relative to the creation of this sequencer in nanoseconds.
    pub fn new_ts(&self) -> Nsec {
        self.start.elapsed().as_nanos() as Nsec
    }
}