//! Integration tests exercising the public ledger API as an external consumer.

use std::collections::HashSet;

use proptest::prelude::*;

use fintrails_ledger::{
    AccountId, Asset, InMemoryStore, LedgerError, LedgerStore, Posting, Transaction, balance_of,
};

// --- shared helpers -------------------------------------------------------

fn acc(s: &str) -> AccountId {
    AccountId(s.into())
}

fn asset(s: &str) -> Asset {
    Asset(s.into())
}

fn posting(account: &str, asset_code: &str, amount: i128) -> Posting {
    Posting {
        account: acc(account),
        asset: asset(asset_code),
        amount,
    }
}

fn usd(account: &str, amount: i128) -> Posting {
    posting(account, "USD", amount)
}

fn transfer() -> Transaction {
    Transaction::new(vec![usd("card", -100), usd("cash", 100)]).unwrap()
}

// --- transaction: Σ=0 invariant ------------------------------------------

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

// --- transaction: reversal (storno) --------------------------------------

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
    let tx = Transaction::new(vec![
        posting("card", "UAH", -100),
        posting("cash", "UAH", 100),
    ])
    .unwrap();
    let journal = [tx.clone(), tx.reverse().unwrap()];

    let uah = asset("UAH");
    assert_eq!(balance_of(&journal, &acc("card"), &uah).unwrap(), 0);
    assert_eq!(balance_of(&journal, &acc("cash"), &uah).unwrap(), 0);
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

// --- store: append, idempotency, balances --------------------------------

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

// --- property tests -------------------------------------------------------

/// A handful of reused account ids so generated transactions overlap.
fn account_strat() -> impl Strategy<Value = AccountId> {
    (0..5usize).prop_map(|i| AccountId(format!("acc{i}")))
}

/// 1..8 legs in a single asset; amounts bounded so sums stay far from
/// the `i128` range (overflow is a separate invariant).
fn legs_strat() -> impl Strategy<Value = Vec<(AccountId, i128)>> {
    prop::collection::vec((account_strat(), -1_000_000_000i128..=1_000_000_000), 1..8)
}

fn usd_posting(account: AccountId, amount: i128) -> Posting {
    Posting {
        account,
        asset: asset("USD"),
        amount,
    }
}

/// A transaction that nets to zero by construction: random legs over a small
/// account pool, plus one balancing leg.
fn balanced_tx() -> impl Strategy<Value = Transaction> {
    legs_strat().prop_map(|legs| {
        let sum: i128 = legs.iter().map(|(_, amount)| amount).sum();
        let mut postings: Vec<Posting> = legs.into_iter().map(|(a, m)| usd_posting(a, m)).collect();
        postings.push(usd_posting(AccountId("balancer".into()), -sum));
        Transaction::new(postings).unwrap()
    })
}

proptest! {
    /// Any set made to net to zero (legs + a balancing leg) is accepted.
    #[test]
    fn balanced_transaction_is_always_accepted(legs in legs_strat()) {
        let sum: i128 = legs.iter().map(|(_, amount)| amount).sum();
        let mut postings: Vec<Posting> =
            legs.into_iter().map(|(a, m)| usd_posting(a, m)).collect();
        postings.push(usd_posting(AccountId("balancer".into()), -sum));

        prop_assert!(Transaction::new(postings).is_ok());
    }

    /// Perturbing the balancing leg by a non-zero delta breaks `Σ=0`.
    #[test]
    fn unbalanced_transaction_is_always_rejected(
        legs in legs_strat(),
        perturb in 1i128..=1_000_000,
    ) {
        let sum: i128 = legs.iter().map(|(_, amount)| amount).sum();
        let mut postings: Vec<Posting> =
            legs.into_iter().map(|(a, m)| usd_posting(a, m)).collect();
        postings.push(usd_posting(AccountId("balancer".into()), -sum + perturb));

        prop_assert!(matches!(
            Transaction::new(postings),
            Err(LedgerError::NotBalanced(_))
        ));
    }

    /// Double-entry's defining property: across every account, a journal of
    /// balanced transactions sums to zero per asset.
    #[test]
    fn journal_balances_sum_to_zero(txs in prop::collection::vec(balanced_tx(), 0..10)) {
        let usd = asset("USD");
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
