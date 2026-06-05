use thiserror::Error;

/// Anything that can go wrong while sourcing events from the chain.
#[derive(Debug, Error)]
pub enum SourceError {
    /// The RPC URL could not be parsed.
    #[error("invalid RPC URL: {0}")]
    InvalidUrl(String),

    /// The RPC call (e.g. `eth_getLogs`) failed.
    #[error("RPC request failed: {0}")]
    Rpc(String),

    /// A returned log could not be decoded as a `Transfer`.
    #[error("failed to decode Transfer log: {0}")]
    Decode(String),

    /// A confirmed log is missing a field we require (block number, tx hash, ...).
    #[error("log is missing required field: {0}")]
    IncompleteLog(&'static str),
}

impl SourceError {
    /// Whether retrying the same call could plausibly succeed. Only transient
    /// transport failures (`Rpc`) qualify; a bad URL or undecodable/incomplete
    /// log is deterministic and retrying it just burns attempts.
    pub fn is_retryable(&self) -> bool {
        matches!(self, SourceError::Rpc(_))
    }
}

/// Anything that can go wrong while loading or persisting the resume cursor.
#[derive(Debug, Error)]
pub enum CheckpointError {
    /// The backing store (file, Postgres, ...) failed to read or write.
    #[error("checkpoint store failed: {0}")]
    Store(String),
}

/// The poller's combined failure domain: sourcing blocks from the chain, or
/// persisting how far it got. Each underlying error converts in with `?`.
#[derive(Debug, Error)]
pub enum PollerError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
}
