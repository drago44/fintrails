use crate::error::CheckpointError;

/// Durable record of how far the poller has safely processed: the highest
/// confirmed block height whose events were already emitted.
///
/// Only that one number is persisted. The unconfirmed tail is *not* — on resume
/// it is rebuilt by re-fetching recent blocks, since anything at or below the
/// confirmed height is final (`N` confirmations deep, beyond reorg reach).
///
/// Storage-agnostic, like the ledger's `LedgerStore`: an in-memory impl for
/// tests now, a Postgres impl later. Async so the Postgres impl fits without
/// changing callers.
#[allow(async_fn_in_trait)] // single-crate use; we never box these futures.
pub trait Checkpoint {
    /// The height to resume past, or `None` on a cold start (never ran before).
    async fn load(&self) -> Result<Option<u64>, CheckpointError>;

    /// Records `height` as confirmed-and-emitted. Called only when the height
    /// changes, so redundant writes are avoided.
    async fn save(&mut self, height: u64) -> Result<(), CheckpointError>;
}

/// In-memory [`Checkpoint`] for tests. Holds the height in a field; lost on drop.
#[derive(Debug, Default, Clone)]
pub struct InMemoryCheckpoint {
    height: Option<u64>,
}

impl InMemoryCheckpoint {
    /// A cold checkpoint: nothing processed yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A warm checkpoint, as if a prior run had confirmed through `height`.
    /// Handy for exercising resume without an actual previous run.
    pub fn at(height: u64) -> Self {
        Self {
            height: Some(height),
        }
    }
}

impl Checkpoint for InMemoryCheckpoint {
    async fn load(&self) -> Result<Option<u64>, CheckpointError> {
        Ok(self.height)
    }

    async fn save(&mut self, height: u64) -> Result<(), CheckpointError> {
        self.height = Some(height);
        Ok(())
    }
}

/// A [`Checkpoint`] that persists nothing: always cold, every save a no-op.
/// The default for a [`Poller`](crate::poller::Poller) built without resume.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCheckpoint;

impl Checkpoint for NoCheckpoint {
    async fn load(&self) -> Result<Option<u64>, CheckpointError> {
        Ok(None)
    }

    async fn save(&mut self, _height: u64) -> Result<(), CheckpointError> {
        Ok(())
    }
}
