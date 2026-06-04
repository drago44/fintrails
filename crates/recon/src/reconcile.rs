use alloy::primitives::{Address, U256};

use fintrails_indexer::event::ChainEvent;
use fintrails_ledger::account::{AccountId, Asset};
use fintrails_ledger::posting::Posting;
use fintrails_ledger::transaction::Transaction;

use crate::error::ReconError;

/// Turns one confirmed on-chain `Transfer` into a balanced ledger transaction.
///
/// The event moves `value` of `token` from `from` to `to`; we mirror that as a
/// two-leg double entry so the ledger's `Σ == 0` invariant holds by construction:
/// the sender account is debited, the receiver credited.
///
/// This is the only place where on-chain `U256` base units cross into the
/// ledger's `i128` minor units (see the boundary rules in CLAUDE.md §8).
pub fn event_to_transaction(event: &ChainEvent) -> Result<Transaction, ReconError> {
    let amount = u256_to_i128(event.value)?;
    let asset = asset_of(event.token);

    let tx = Transaction::new(vec![
        Posting {
            account: account_of(event.from),
            asset: asset.clone(),
            amount: -amount,
        },
        Posting {
            account: account_of(event.to),
            asset,
            amount,
        },
    ])?;
    Ok(tx)
}

/// Idempotency key for an event: a log is unique per `(tx_hash, log_index)`.
/// Feeding this to the ledger's idempotent `append` makes the pipeline
/// exactly-once even under at-least-once delivery.
pub fn idempotency_key(event: &ChainEvent) -> String {
    format!("{:#x}:{}", event.tx_hash, event.log_index)
}

/// Converts an on-chain amount to the ledger's signed minor units, erroring if
/// it overflows `i128` rather than silently wrapping.
fn u256_to_i128(value: U256) -> Result<i128, ReconError> {
    i128::try_from(value).map_err(|_| ReconError::AmountOverflow(value.to_string()))
}

/// Maps a chain address to a ledger account id. Thin slice: raw address under an
/// `onchain:` namespace. Invoice/merchant accounts come later (recon Phase 1).
fn account_of(addr: Address) -> AccountId {
    AccountId(format!("onchain:{addr:#x}"))
}

/// Maps a token contract to a ledger asset. Thin slice: the raw token address.
/// Symbol/decimals resolution comes later (recon Phase 1).
fn asset_of(token: Address) -> Asset {
    Asset(format!("{token:#x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::B256;

    fn event(value: U256) -> ChainEvent {
        ChainEvent {
            token: Address::repeat_byte(0xaa),
            from: Address::repeat_byte(0x11),
            to: Address::repeat_byte(0x22),
            value,
            block_number: 100,
            tx_hash: B256::repeat_byte(0xbb),
            log_index: 3,
        }
    }

    #[test]
    fn transfer_becomes_a_balanced_mirror_entry() {
        let tx = event_to_transaction(&event(U256::from(1000u64))).unwrap();
        let postings = tx.postings();

        // Sender debited, receiver credited, summing to zero.
        let from = &postings[0];
        let to = &postings[1];
        assert_eq!(from.account, account_of(Address::repeat_byte(0x11)));
        assert_eq!(from.amount, -1000);
        assert_eq!(to.account, account_of(Address::repeat_byte(0x22)));
        assert_eq!(to.amount, 1000);
        assert_eq!(from.amount + to.amount, 0);
    }

    #[test]
    fn value_above_i128_max_is_rejected() {
        let too_big = U256::from(i128::MAX) + U256::from(1u64);
        let result = event_to_transaction(&event(too_big));
        assert!(matches!(result, Err(ReconError::AmountOverflow(_))));
    }

    #[test]
    fn idempotency_key_combines_tx_hash_and_log_index() {
        let key = idempotency_key(&event(U256::from(1u64)));
        assert!(key.ends_with(":3"));
    }
}
