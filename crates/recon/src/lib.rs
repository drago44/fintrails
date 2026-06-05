//! `fintrails-recon` — the glue that reconciles on-chain events against a ledger.
//!
//! It depends on both `fintrails-indexer` (source of [`ChainEvent`]s) and
//! `fintrails-ledger` (the double-entry journal), and is the *only* place where
//! the two meet: on-chain `U256` base units are converted to ledger `i128`
//! minor units here and nowhere else (see CLAUDE.md §8).
//!
//! [`ChainEvent`]: fintrails_indexer::ChainEvent

mod reconcile;
pub use reconcile::{account_of, asset_of, event_to_transaction, idempotency_key};

mod error;
pub use error::ReconError;
