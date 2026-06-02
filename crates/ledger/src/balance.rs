use crate::account::{AccountId, Asset};
use crate::transaction::Transaction;

/// Balance of an account in an asset: the sum of every matching posting.
///
/// Pure fold over the journal — the balance is always derived, never stored.
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
/// assert_eq!(balance_of(&journal, &AccountId("cash".into()), &usd), 100);
/// ```
pub fn balance_of(txs: &[Transaction], account: &AccountId, asset: &Asset) -> i128 {
    txs.iter()
        .flat_map(|tx| tx.postings())
        .filter(|p| &p.account == account && &p.asset == asset)
        .map(|p| p.amount)
        .sum()
}
