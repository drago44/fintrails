//! End-to-end pass over the public APIs of all three crates:
//! A (indexer) produces a `ChainEvent` → C (recon) maps it to a balanced
//! transaction → B (ledger) stores it and derives balances.
//!
//! The chain itself (A's RPC) is the only stubbed part: events are built by
//! hand here, standing in for what `fetch_transfers` would return. Real
//! on-chain e2e via `anvil` is a later (Phase 5) concern.

use alloy::primitives::{Address, B256, U256};

use fintrails_indexer::event::ChainEvent;
use fintrails_ledger::store::{InMemoryStore, LedgerStore};
use fintrails_recon::reconcile::{account_of, asset_of, event_to_transaction, idempotency_key};

/// Builds a `Transfer` event as the indexer would hand it over. `log_index`
/// keeps each event's idempotency key distinct within a tx.
fn transfer(from: Address, to: Address, token: Address, value: u64, log_index: u64) -> ChainEvent {
    ChainEvent {
        token,
        from,
        to,
        value: U256::from(value),
        block_number: 1_000,
        tx_hash: B256::repeat_byte(0xab),
        log_index,
    }
}

/// Feeds one event through recon into the ledger, the way recon's binary will.
fn ingest(store: &mut InMemoryStore, event: &ChainEvent) {
    let tx = event_to_transaction(event).expect("event maps to a balanced transaction");
    store
        .append(&idempotency_key(event), tx)
        .expect("ledger accepts the transaction");
}

#[test]
fn single_transfer_credits_receiver_and_debits_sender() {
    let token = Address::repeat_byte(0x01);
    let payer = Address::repeat_byte(0xf1);
    let payee = Address::repeat_byte(0xf2);

    let mut store = InMemoryStore::new();
    ingest(&mut store, &transfer(payer, payee, token, 100, 0));

    let asset = asset_of(token);
    assert_eq!(store.balance(&account_of(payee), &asset).unwrap(), 100);
    assert_eq!(store.balance(&account_of(payer), &asset).unwrap(), -100);
}

#[test]
fn replaying_the_same_event_does_not_double_count() {
    let token = Address::repeat_byte(0x01);
    let payer = Address::repeat_byte(0xf1);
    let payee = Address::repeat_byte(0xf2);
    let event = transfer(payer, payee, token, 100, 0);

    let mut store = InMemoryStore::new();
    ingest(&mut store, &event);
    ingest(&mut store, &event); // at-least-once delivery: same (tx_hash, log_index)

    // Idempotent end to end: the second pass is a no-op, not a double credit.
    assert_eq!(
        store.balance(&account_of(payee), &asset_of(token)).unwrap(),
        100
    );
}

#[test]
fn create2_sweep_split_flow_composes_balances() {
    // CLAUDE.md §9: funds land on a CREATE2 child address, then get swept to
    // the merchant payout and split to treasury. Three independent Transfer
    // events, one token (USDC, base units at 6 decimals).
    let usdc = Address::repeat_byte(0x01);
    let cex = Address::repeat_byte(0xc0);
    let child = Address::repeat_byte(0xcd);
    let payout = Address::repeat_byte(0xa0);
    let treasury = Address::repeat_byte(0x77);

    let mut store = InMemoryStore::new();
    ingest(&mut store, &transfer(cex, child, usdc, 100_000_000, 0)); // inbound 100
    ingest(&mut store, &transfer(child, payout, usdc, 99_500_000, 1)); // sweep 99.5
    ingest(&mut store, &transfer(child, treasury, usdc, 500_000, 2)); // split 0.5

    let asset = asset_of(usdc);
    // The child address is fully drained: it nets to zero after sweep + split.
    assert_eq!(store.balance(&account_of(child), &asset).unwrap(), 0);
    assert_eq!(
        store.balance(&account_of(payout), &asset).unwrap(),
        99_500_000
    );
    assert_eq!(
        store.balance(&account_of(treasury), &asset).unwrap(),
        500_000
    );
    assert_eq!(
        store.balance(&account_of(cex), &asset).unwrap(),
        -100_000_000
    );
}
