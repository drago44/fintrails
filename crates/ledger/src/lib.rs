//! `fintrails-ledger` — a pure, storage-agnostic double-entry ledger.
//!
//! Core invariant: every transaction balances to zero per asset
//! (`Σ debits == Σ credits`). No async, no network, no blockchain here.
//!
//! # Example
//!
//! ```
//! use fintrails_ledger::{AccountId, Asset, InMemoryStore, LedgerStore, Posting, Transaction};
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
//! assert_eq!(store.balance(&AccountId("cash".into()), &usd).unwrap(), 100);
//! assert_eq!(store.balance(&AccountId("card".into()), &usd).unwrap(), -100);
//! ```

mod account;
pub use account::{AccountId, Asset};

mod posting;
pub use posting::Posting;

mod transaction;
pub use transaction::Transaction;

mod balance;
pub use balance::balance_of;

mod store;
pub use store::{InMemoryStore, LedgerStore};

mod error;
pub use error::LedgerError;
