use crate::account::{AccountId, Asset};
use crate::transaction::Transaction;

/// Balance of an account in an asset: the sum of every matching posting.
///
/// Pure fold over the journal — the balance is always derived, never stored.
pub fn balance_of(txs: &[Transaction], account: &AccountId, asset: &Asset) -> i128 {
    txs.iter()
        .flat_map(|tx| tx.postings())
        .filter(|p| &p.account == account && &p.asset == asset)
        .map(|p| p.amount)
        .sum()
}
