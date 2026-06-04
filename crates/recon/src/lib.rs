//! `fintrails-recon` — the glue that reconciles on-chain events against a ledger.
//!
//! It depends on both `fintrails-indexer` (source of [`ChainEvent`]s) and
//! `fintrails-ledger` (the double-entry journal), and is the *only* place where
//! the two meet: on-chain `U256` base units are converted to ledger `i128`
//! minor units here and nowhere else (see CLAUDE.md §8).
//!
//! [`ChainEvent`]: fintrails_indexer::event::ChainEvent

pub mod error;
pub mod reconcile;
