use alloy::eips::BlockNumberOrTag;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;

use crate::error::SourceError;
use crate::event::ChainEvent;
use crate::tracker::BlockRef;

sol! {
    // Standard ERC-20 Transfer; alloy derives the decoder and signature hash.
    event Transfer(address indexed from, address indexed to, uint256 value);
}

/// Reads block metadata and `Transfer` logs from a single chain, for one token.
///
/// This is the boundary the [`Poller`](crate::poller::Poller) depends on. The
/// production implementation is [`RpcBlockSource`]; tests substitute an
/// in-memory fake, so the poller's reorg logic is exercisable without a node.
#[allow(async_fn_in_trait)] // single-crate use; we never box these futures.
pub trait BlockSource {
    /// The current chain head's block number.
    async fn head_number(&self) -> Result<u64, SourceError>;

    /// The block at `number`, as a [`BlockRef`] (number, hash, parent hash).
    async fn block_ref(&self, number: u64) -> Result<BlockRef, SourceError>;

    /// `Transfer` events for the token over the inclusive range `[from, to]`.
    async fn transfers(&self, from: u64, to: u64) -> Result<Vec<ChainEvent>, SourceError>;
}

/// A [`BlockSource`] backed by a live JSON-RPC provider, scoped to one token.
pub struct RpcBlockSource<P> {
    provider: P,
    token: Address,
}

impl<P: Provider> RpcBlockSource<P> {
    /// Wraps an already-built provider. Keeping the provider external means one
    /// connection is reused across every poll, rather than reconnecting per call.
    pub fn new(provider: P, token: Address) -> Self {
        Self { provider, token }
    }
}

impl<P: Provider> BlockSource for RpcBlockSource<P> {
    async fn head_number(&self) -> Result<u64, SourceError> {
        self.provider
            .get_block_number()
            .await
            .map_err(|e| SourceError::Rpc(format!("{e}")))
    }

    async fn block_ref(&self, number: u64) -> Result<BlockRef, SourceError> {
        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Number(number))
            .await
            .map_err(|e| SourceError::Rpc(format!("{e}")))?
            .ok_or(SourceError::IncompleteLog("block"))?;

        Ok(BlockRef {
            number: block.header.number,
            hash: block.header.hash,
            parent_hash: block.header.parent_hash,
        })
    }

    async fn transfers(&self, from: u64, to: u64) -> Result<Vec<ChainEvent>, SourceError> {
        fetch_transfers_with(&self.provider, self.token, from, to).await
    }
}

/// Builds an HTTP JSON-RPC provider from `rpc_url`, ready to hand to
/// [`RpcBlockSource::new`] or [`fetch_transfers_with`].
pub fn http_provider(rpc_url: &str) -> Result<impl Provider, SourceError> {
    let url = rpc_url
        .parse()
        .map_err(|e| SourceError::InvalidUrl(format!("{e}")))?;
    Ok(ProviderBuilder::new().connect_http(url))
}

/// Fetches ERC-20 `Transfer` logs for `token` over the inclusive block range
/// `[from_block, to_block]` and decodes them into [`ChainEvent`]s.
///
/// Backfill only: a single `eth_getLogs` call. No range chunking, retries,
/// confirmation waiting or reorg handling yet — those are later steps.
pub async fn fetch_transfers(
    rpc_url: &str,
    token: Address,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<ChainEvent>, SourceError> {
    let provider = http_provider(rpc_url)?;
    fetch_transfers_with(&provider, token, from_block, to_block).await
}

/// Core of [`fetch_transfers`] over a caller-supplied provider, so a single
/// connection can be reused across many calls (e.g. by the poller).
pub async fn fetch_transfers_with<P: Provider>(
    provider: &P,
    token: Address,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<ChainEvent>, SourceError> {
    let filter = Filter::new()
        .address(token)
        .event_signature(Transfer::SIGNATURE_HASH)
        .from_block(from_block)
        .to_block(to_block);

    let logs = provider
        .get_logs(&filter)
        .await
        .map_err(|e| SourceError::Rpc(format!("{e}")))?;

    logs.iter().map(log_to_event).collect()
}

/// Pure decode of one fetched log into a [`ChainEvent`]. No network — split out
/// from [`fetch_transfers`] so the decode logic is unit-testable on its own.
fn log_to_event(log: &alloy::rpc::types::Log) -> Result<ChainEvent, SourceError> {
    let decoded =
        Transfer::decode_log(&log.inner).map_err(|e| SourceError::Decode(format!("{e}")))?;

    Ok(ChainEvent {
        token: log.inner.address,
        from: decoded.data.from,
        to: decoded.data.to,
        value: decoded.data.value,
        block_number: log
            .block_number
            .ok_or(SourceError::IncompleteLog("block_number"))?,
        tx_hash: log
            .transaction_hash
            .ok_or(SourceError::IncompleteLog("tx_hash"))?,
        log_index: log
            .log_index
            .ok_or(SourceError::IncompleteLog("log_index"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{B256, U256};

    fn sample_log(block_number: Option<u64>) -> alloy::rpc::types::Log {
        let transfer = Transfer {
            from: Address::repeat_byte(0x11),
            to: Address::repeat_byte(0x22),
            value: U256::from(1000u64),
        };
        alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: Address::repeat_byte(0xaa),
                data: transfer.encode_log_data(),
            },
            block_number,
            transaction_hash: Some(B256::repeat_byte(0xbb)),
            log_index: Some(7),
            ..Default::default()
        }
    }

    #[test]
    fn decodes_transfer_log_into_event() {
        let event = log_to_event(&sample_log(Some(42))).unwrap();
        assert_eq!(event.token, Address::repeat_byte(0xaa));
        assert_eq!(event.from, Address::repeat_byte(0x11));
        assert_eq!(event.to, Address::repeat_byte(0x22));
        assert_eq!(event.value, U256::from(1000u64));
        assert_eq!(event.block_number, 42);
        assert_eq!(event.tx_hash, B256::repeat_byte(0xbb));
        assert_eq!(event.log_index, 7);
    }

    #[test]
    fn missing_block_number_is_rejected() {
        let result = log_to_event(&sample_log(None));
        assert!(matches!(
            result,
            Err(SourceError::IncompleteLog("block_number"))
        ));
    }
}
