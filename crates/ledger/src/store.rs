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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::posting::Posting;

    fn acc(s: &str) -> AccountId {
        AccountId(s.into())
    }
    fn asset(s: &str) -> Asset {
        Asset(s.into())
    }
    fn usd(account: &str, amount: i128) -> Posting {
        Posting {
            account: acc(account),
            asset: asset("USD"),
            amount,
        }
    }
    fn transfer() -> Transaction {
        Transaction::new(vec![usd("card", -100), usd("cash", 100)]).unwrap()
    }

    #[test]
    fn balances_reflect_appended_transactions() {
        let mut store = InMemoryStore::new();
        store.append("tx-1", transfer()).unwrap();

        assert_eq!(store.balance(&acc("card"), &asset("USD")).unwrap(), -100);
        assert_eq!(store.balance(&acc("cash"), &asset("USD")).unwrap(), 100);
    }

    #[test]
    fn append_is_idempotent_on_repeated_key() {
        let mut store = InMemoryStore::new();
        store.append("tx-1", transfer()).unwrap();
        store.append("tx-1", transfer()).unwrap(); // same key, must be ignored

        assert_eq!(store.balance(&acc("cash"), &asset("USD")).unwrap(), 100);
    }

    #[test]
    fn append_rejects_same_key_with_different_body() {
        let mut store = InMemoryStore::new();
        store.append("tx-1", transfer()).unwrap();

        // Same key, different body: must be rejected, not silently ignored.
        let other = Transaction::new(vec![usd("card", -50), usd("cash", 50)]).unwrap();
        assert!(matches!(
            store.append("tx-1", other),
            Err(LedgerError::IdempotencyConflict { .. })
        ));

        // The conflicting transaction must not have touched balances.
        assert_eq!(store.balance(&acc("cash"), &asset("USD")).unwrap(), 100);
    }

    #[test]
    fn balance_accumulates_across_transactions() {
        let mut store = InMemoryStore::new();
        store.append("a", transfer()).unwrap();
        store
            .append(
                "b",
                Transaction::new(vec![usd("card", -50), usd("cash", 50)]).unwrap(),
            )
            .unwrap();

        assert_eq!(store.balance(&acc("cash"), &asset("USD")).unwrap(), 150);
        assert_eq!(store.balance(&acc("card"), &asset("USD")).unwrap(), -150);
    }

    #[test]
    fn unknown_account_has_zero_balance() {
        let store = InMemoryStore::new();
        assert_eq!(store.balance(&acc("ghost"), &asset("USD")).unwrap(), 0);
    }
}
