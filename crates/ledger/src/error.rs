use thiserror::Error;

use crate::account::{AccountId, Asset};

/// Anything that can go wrong inside the ledger core. Grows as operations are added.
#[derive(Debug, Error)]
pub enum LedgerError {
    /// A transaction's postings do not sum to zero for this asset.
    #[error("transaction does not balance for asset {0:?}")]
    NotBalanced(Asset),

    /// A transaction was created with no postings.
    #[error("transaction has no postings")]
    EmptyTransaction,

    /// Accumulating an account balance exceeded the `i128` range.
    #[error("balance overflow for account {account:?} in asset {asset:?}")]
    Overflow { account: AccountId, asset: Asset },
}
