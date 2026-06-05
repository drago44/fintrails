//! `fintrails-indexer` — a reliable on-chain indexer: it watches a chain for
//! confirmed ERC-20 transfers and emits [`ChainEvent`]s.
//!
//! It knows nothing about accounting — it survives reorgs, resumes from a
//! checkpoint, and leaves what to do with the events to its consumer.

mod event;
pub use event::ChainEvent;

mod source;
pub use source::{
    BlockSource, DEFAULT_MAX_BLOCK_SPAN, RpcBlockSource, fetch_transfers, fetch_transfers_chunked,
    fetch_transfers_with, http_provider,
};

mod tracker;
pub use tracker::{BlockRef, Tracker, TrackerUpdate};

mod poller;
pub use poller::{IndexerMessage, Poller};

mod checkpoint;
pub use checkpoint::{Checkpoint, InMemoryCheckpoint, NoCheckpoint};

mod retry;
pub use retry::{RetryPolicy, RetryingSource, with_retry};

mod error;
pub use error::{CheckpointError, PollerError, SourceError};
