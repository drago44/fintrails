use crate::account::{AccountId, Asset};
use crate::error::LedgerError;
use crate::transaction::Transaction;

/// Balance of an account in an asset: the sum of every matching posting.
///
/// Pure fold over the journal — the balance is always derived, never stored.
/// Errors with [`LedgerError::Overflow`] instead of panicking or wrapping when
/// the running sum leaves the `i128` range.
///
/// # Examples
///
/// ```
/// use fintrails_ledger::account::{AccountId, Asset};
/// use fintrails_ledger::balance::balance_of;
/// use fintrails_ledger::posting::Posting;
/// use fintrails_ledger::transaction::Transaction;
///
/// let usd = Asset("USD".into());
/// let tx = Transaction::new(vec![
///     Posting { account: AccountId("card".into()), asset: usd.clone(), amount: -100 },
///     Posting { account: AccountId("cash".into()), asset: usd.clone(), amount: 100 },
/// ])
/// .unwrap();
///
/// let journal = [tx];
/// assert_eq!(balance_of(&journal, &AccountId("cash".into()), &usd).unwrap(), 100);
/// ```
pub fn balance_of(
    txs: &[Transaction],
    account: &AccountId,
    asset: &Asset,
) -> Result<i128, LedgerError> {
    txs.iter()
        .flat_map(|tx| tx.postings())
        .filter(|p| &p.account == account && &p.asset == asset)
        .try_fold(0i128, |acc, p| acc.checked_add(p.amount))
        .ok_or_else(|| LedgerError::Overflow {
            account: account.clone(),
            asset: asset.clone(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use super::*;
    use crate::posting::Posting;

    /// A transaction that nets to zero by construction: random legs over a
    /// small account pool, plus one balancing leg.
    fn balanced_tx() -> impl Strategy<Value = Transaction> {
        prop::collection::vec((0..5usize, -1_000_000_000i128..=1_000_000_000), 1..8).prop_map(
            |legs| {
                let sum: i128 = legs.iter().map(|(_, amount)| amount).sum();
                let mut postings: Vec<Posting> = legs
                    .into_iter()
                    .map(|(i, amount)| Posting {
                        account: AccountId(format!("acc{i}")),
                        asset: Asset("USD".into()),
                        amount,
                    })
                    .collect();
                postings.push(Posting {
                    account: AccountId("balancer".into()),
                    asset: Asset("USD".into()),
                    amount: -sum,
                });
                Transaction::new(postings).unwrap()
            },
        )
    }

    proptest! {
        /// Double-entry's defining property: across every account, a journal of
        /// balanced transactions sums to zero per asset.
        #[test]
        fn journal_balances_sum_to_zero(txs in prop::collection::vec(balanced_tx(), 0..10)) {
            let usd = Asset("USD".into());
            let accounts: HashSet<AccountId> = txs
                .iter()
                .flat_map(|tx| tx.postings())
                .map(|p| p.account.clone())
                .collect();

            let total: i128 = accounts
                .iter()
                .map(|a| balance_of(&txs, a, &usd).unwrap())
                .sum();

            prop_assert_eq!(total, 0);
        }
    }
}
