use std::collections::HashMap;

use crate::account::{AccountId, Asset};
use crate::balance::balance_of;
use crate::error::LedgerError;
use crate::transaction::Transaction;

/// Storage contract for the ledger: append-only journal, derived balances.
pub trait LedgerStore {
    /// Append a transaction under an idempotency key. Repeating the same key
    /// with the same body is a no-op, turning at-least-once delivery into
    /// exactly-once. Reusing a key with a *different* body is rejected with
    /// [`LedgerError::IdempotencyConflict`].
    fn append(&mut self, key: &str, tx: Transaction) -> Result<(), LedgerError>;

    /// Current balance of an account in an asset.
    fn balance(&self, account: &AccountId, asset: &Asset) -> Result<i128, LedgerError>;
}

/// In-memory store for tests and examples. The journal is the source of truth.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    journal: Vec<Transaction>,
    /// Idempotency key → index of its transaction in `journal`. Storing the
    /// index (not the body) keeps the journal the single source of truth.
    keys: HashMap<String, usize>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LedgerStore for InMemoryStore {
    fn append(&mut self, key: &str, tx: Transaction) -> Result<(), LedgerError> {
        if let Some(&idx) = self.keys.get(key) {
            // Known key: a replay is a no-op, a different body is a conflict.
            return if self.journal[idx] == tx {
                Ok(())
            } else {
                Err(LedgerError::IdempotencyConflict {
                    key: key.to_string(),
                })
            };
        }
        self.keys.insert(key.to_string(), self.journal.len());
        self.journal.push(tx);
        Ok(())
    }

    fn balance(&self, account: &AccountId, asset: &Asset) -> Result<i128, LedgerError> {
        balance_of(&self.journal, account, asset)
    }
}
