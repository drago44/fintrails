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
