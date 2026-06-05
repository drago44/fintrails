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

    /// Builds the reversing (storno) transaction: the exact negation of every
    /// posting. Appending it undoes `self` without mutating the journal, keeping
    /// it append-only. The result nets to zero by construction — negating a
    /// balanced set stays balanced — so it skips re-validation through `new`.
    ///
    /// Errors with [`LedgerError::Overflow`] only if an amount is `i128::MIN`,
    /// which has no positive counterpart to negate.
    pub fn reverse(&self) -> Result<Transaction, LedgerError> {
        let postings = self
            .postings
            .iter()
            .map(|p| {
                p.amount
                    .checked_neg()
                    .map(|amount| Posting {
                        amount,
                        ..p.clone()
                    })
                    .ok_or_else(|| LedgerError::Overflow {
                        account: p.account.clone(),
                        asset: p.asset.clone(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { postings })
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

    #[test]
    fn reverse_negates_every_posting() {
        let tx = Transaction::new(vec![
            posting("card", "UAH", -100),
            posting("cash", "UAH", 100),
        ])
        .unwrap();
        let storno = tx.reverse().unwrap();

        assert_eq!(
            storno.postings(),
            &[posting("card", "UAH", 100), posting("cash", "UAH", -100)]
        );
    }

    #[test]
    fn original_plus_reversal_nets_to_zero() {
        use crate::balance::balance_of;

        let tx = Transaction::new(vec![
            posting("card", "UAH", -100),
            posting("cash", "UAH", 100),
        ])
        .unwrap();
        let journal = [tx.clone(), tx.reverse().unwrap()];

        let uah = Asset("UAH".into());
        assert_eq!(
            balance_of(&journal, &AccountId("card".into()), &uah).unwrap(),
            0
        );
        assert_eq!(
            balance_of(&journal, &AccountId("cash".into()), &uah).unwrap(),
            0
        );
    }

    #[test]
    fn double_reversal_restores_the_original() {
        let tx = Transaction::new(vec![
            posting("card", "UAH", -100),
            posting("cash", "UAH", 100),
        ])
        .unwrap();
        assert_eq!(tx.reverse().unwrap().reverse().unwrap(), tx);
    }
}
