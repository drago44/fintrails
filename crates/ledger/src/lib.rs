//! `fintrails-ledger` — a pure, storage-agnostic double-entry ledger.
//!
//! Core invariant: every transaction balances to zero per asset
//! (`Σ debits == Σ credits`). No async, no network, no blockchain here.
//!
//! # Example
//!
//! ```
//! use fintrails_ledger::account::{AccountId, Asset};
//! use fintrails_ledger::posting::Posting;
//! use fintrails_ledger::store::{InMemoryStore, LedgerStore};
//! use fintrails_ledger::transaction::Transaction;
//!
//! let usd = Asset("USD".into());
//!
//! // Move 100 minor units from "card" to "cash" (Σ == 0).
//! let tx = Transaction::new(vec![
//!     Posting { account: AccountId("card".into()), asset: usd.clone(), amount: -100 },
//!     Posting { account: AccountId("cash".into()), asset: usd.clone(), amount: 100 },
//! ])
//! .expect("transaction balances");
//!
//! let mut store = InMemoryStore::new();
//! store.append("tx-1", tx).expect("appended");
//!
//! assert_eq!(store.balance(&AccountId("cash".into()), &usd), 100);
//! assert_eq!(store.balance(&AccountId("card".into()), &usd), -100);
//! ```

pub mod account;
pub mod balance;
pub mod error;
pub mod posting;
pub mod store;
pub mod transaction;
