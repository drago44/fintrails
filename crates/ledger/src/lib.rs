//! `fintrails-ledger` — a pure, storage-agnostic double-entry ledger.
//!
//! Core invariant: every transaction balances to zero per asset
//! (`Σ debits == Σ credits`). No async, no network, no blockchain here.

pub mod account;
pub mod error;
pub mod posting;
