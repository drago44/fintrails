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
/// use fintrails_ledger::{AccountId, Asset, Posting, Transaction, balance_of};
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
