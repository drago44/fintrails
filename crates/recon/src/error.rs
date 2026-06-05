use thiserror::Error;

use fintrails_ledger::LedgerError;

/// Anything that can go wrong while turning a chain event into a ledger entry.
#[derive(Debug, Error)]
pub enum ReconError {
    /// The on-chain `U256` amount does not fit into the ledger's `i128`.
    #[error("transfer value does not fit into i128: {0}")]
    AmountOverflow(String),

    /// The derived transaction was rejected by the ledger (e.g. unbalanced).
    #[error("ledger rejected the transaction: {0}")]
    Ledger(#[from] LedgerError),
}
