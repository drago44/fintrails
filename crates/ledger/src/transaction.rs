use std::collections::HashMap;

use serde::Serialize;

use crate::account::Asset;
use crate::error::LedgerError;
use crate::posting::Posting;

/// A set of postings treated as one atomic unit.
///
/// The only way to build one is [`Transaction::new`], which rejects anything
/// that is empty or does not balance to zero per asset. Holding a `Transaction`
/// therefore proves it is balanced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Transaction {
    postings: Vec<Posting>,
}

impl Transaction {
    /// Builds a transaction, enforcing the ledger invariants: the set must be
    /// non-empty and must net to zero per asset. Returns [`LedgerError`] otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use fintrails_ledger::account::{AccountId, Asset};
    /// use fintrails_ledger::posting::Posting;
    /// use fintrails_ledger::transaction::Transaction;
    ///
    /// let usd = Asset("USD".into());
    /// let ok = Transaction::new(vec![
    ///     Posting { account: AccountId("a".into()), asset: usd.clone(), amount: -50 },
    ///     Posting { account: AccountId("b".into()), asset: usd.clone(), amount: 50 },
    /// ]);
    /// assert!(ok.is_ok());
    ///
    /// // Does not balance: rejected.
    /// let bad = Transaction::new(vec![
    ///     Posting { account: AccountId("a".into()), asset: usd.clone(), amount: -50 },
    ///     Posting { account: AccountId("b".into()), asset: usd.clone(), amount: 40 },
    /// ]);
    /// assert!(bad.is_err());
    /// ```
    pub fn new(postings: Vec<Posting>) -> Result<Self, LedgerError> {
        if postings.is_empty() {
            return Err(LedgerError::EmptyTransaction);
        }

        // Net every asset independently; each must sum to exactly zero.
        let mut sums: HashMap<&Asset, i128> = HashMap::new();
        for p in &postings {
            *sums.entry(&p.asset).or_insert(0) += p.amount;
        }
        for (asset, sum) in sums {
            if sum != 0 {
                return Err(LedgerError::NotBalanced(asset.clone()));
            }
        }

        Ok(Self { postings })
    }

    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountId;

    fn posting(account: &str, asset: &str, amount: i128) -> Posting {
        Posting {
            account: AccountId(account.into()),
            asset: Asset(asset.into()),
            amount,
        }
    }

    #[test]
    fn balanced_transaction_is_accepted() {
        let tx = Transaction::new(vec![
            posting("card", "UAH", -100),
            posting("cash", "UAH", 100),
        ]);
        assert!(tx.is_ok());
    }

    #[test]
    fn unbalanced_transaction_is_rejected() {
        let tx = Transaction::new(vec![
            posting("card", "UAH", -100),
            posting("cash", "UAH", 90),
        ]);
        assert!(matches!(tx, Err(LedgerError::NotBalanced(_))));
    }

    #[test]
    fn empty_transaction_is_rejected() {
        let tx = Transaction::new(vec![]);
        assert!(matches!(tx, Err(LedgerError::EmptyTransaction)));
    }

    #[test]
    fn balancing_is_checked_per_asset() {
        // Nets to zero overall, but not within each asset.
        let tx = Transaction::new(vec![posting("a", "USDC", 100), posting("b", "USDT", -100)]);
        assert!(matches!(tx, Err(LedgerError::NotBalanced(_))));
    }
}
