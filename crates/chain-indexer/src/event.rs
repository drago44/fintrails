use alloy::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// A single confirmed ERC-20 `Transfer`, as the indexer hands it to the outside
/// world. It carries on-chain values verbatim: amounts stay as [`U256`] base
/// units and are converted to ledger minor units only downstream (in `recon`).
///
/// `tx_hash` together with `log_index` uniquely identifies the log within the
/// chain, which is what later makes delivery idempotent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainEvent {
    /// Address of the token contract that emitted the `Transfer`.
    pub token: Address,
    pub from: Address,
    pub to: Address,
    /// Transferred amount in the token's base units.
    pub value: U256,
    pub block_number: u64,
    pub tx_hash: B256,
    /// Index of this log within its block; unique per block alongside `tx_hash`.
    pub log_index: u64,
}
