use alloy::primitives::B256;

/// Minimal header of a block the tracker has seen.
///
/// `parent_hash` is stored from the start but unused here in the linear-only
/// step; reorg detection (next step) is what links each block to its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRef {
    pub number: u64,
    pub hash: B256,
    pub parent_hash: B256,
}

/// What changed after a call to [`Tracker::observe`].
///
/// For now it only reports blocks that just crossed the confirmation depth.
/// A `reorged` field will join it once rollback is implemented.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TrackerUpdate {
    /// Blocks that reached `N` confirmations on this observation, ascending.
    /// Their events are safe to emit downstream.
    pub confirmed: Vec<BlockRef>,
}

/// Tracks the chain head and decides when a block is confirmed.
///
/// A block at height `H` is confirmed once the head reaches `H + N`, i.e. `N`
/// blocks are built on top of it. Pure and synchronous: no network, no async —
/// the RPC that feeds it lives in a higher layer.
#[derive(Debug)]
pub struct Tracker {
    confirmations: u64,
    /// Seen blocks in ascending order. This step assumes linear growth and does
    /// not trim; windowing arrives with reorg handling.
    chain: Vec<BlockRef>,
    /// Highest block number already reported as confirmed (the watermark).
    confirmed_through: Option<u64>,
}

impl Tracker {
    /// Creates a tracker requiring `confirmations` blocks on top before a block
    /// is considered confirmed.
    pub fn new(confirmations: u64) -> Self {
        Self {
            confirmations,
            chain: Vec::new(),
            confirmed_through: None,
        }
    }

    /// Feeds the next block. This step assumes `block` linearly extends the head;
    /// reorg handling is added next. Returns the blocks newly confirmed by this
    /// observation.
    pub fn observe(&mut self, block: BlockRef) -> TrackerUpdate {
        self.chain.push(block);
        let head = self.chain.last().expect("just pushed a block").number;

        let mut confirmed = Vec::new();
        if let Some(confirmed_max) = head.checked_sub(self.confirmations) {
            // Emit only blocks past the watermark, up to the new confirmed height.
            let start = self.confirmed_through.map_or(0, |w| w + 1);
            for b in &self.chain {
                if (start..=confirmed_max).contains(&b.number) {
                    confirmed.push(b.clone());
                }
            }
            if self.confirmed_through.is_none_or(|w| confirmed_max > w) {
                self.confirmed_through = Some(confirmed_max);
            }
        }

        TrackerUpdate { confirmed }
    }

    /// Highest confirmed block number so far, or `None` if nothing is confirmed.
    pub fn confirmed_height(&self) -> Option<u64> {
        self.confirmed_through
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a linear block: hash = number, parent_hash = number - 1.
    fn block(number: u64) -> BlockRef {
        BlockRef {
            number,
            hash: B256::from(alloy::primitives::U256::from(number)),
            parent_hash: B256::from(alloy::primitives::U256::from(number.saturating_sub(1))),
        }
    }

    #[test]
    fn block_confirms_once_n_blocks_sit_on_top() {
        let mut tracker = Tracker::new(2);

        // Blocks 0 and 1: head too low, nothing confirmed yet.
        assert!(tracker.observe(block(0)).confirmed.is_empty());
        assert!(tracker.observe(block(1)).confirmed.is_empty());
        assert_eq!(tracker.confirmed_height(), None);

        // Block 2 arrives: head=2, so block 0 (head - N) is now confirmed.
        let update = tracker.observe(block(2));
        assert_eq!(update.confirmed, vec![block(0)]);
        assert_eq!(tracker.confirmed_height(), Some(0));
    }

    #[test]
    fn each_block_is_confirmed_once_and_in_order() {
        let mut tracker = Tracker::new(1);

        tracker.observe(block(0));
        assert_eq!(tracker.observe(block(1)).confirmed, vec![block(0)]);
        assert_eq!(tracker.observe(block(2)).confirmed, vec![block(1)]);
        assert_eq!(tracker.observe(block(3)).confirmed, vec![block(2)]);
        assert_eq!(tracker.confirmed_height(), Some(2));
    }

    #[test]
    fn deeper_confirmation_depth_lags_further_behind_the_head() {
        let mut tracker = Tracker::new(3);

        // Need head >= 3 before block 0 confirms.
        for n in 0..3 {
            assert!(tracker.observe(block(n)).confirmed.is_empty());
        }
        assert_eq!(tracker.observe(block(3)).confirmed, vec![block(0)]);
        assert_eq!(tracker.observe(block(4)).confirmed, vec![block(1)]);
        assert_eq!(tracker.confirmed_height(), Some(1));
    }

    #[test]
    fn zero_confirmations_confirms_immediately() {
        let mut tracker = Tracker::new(0);
        assert_eq!(tracker.observe(block(5)).confirmed, vec![block(5)]);
        assert_eq!(tracker.confirmed_height(), Some(5));
    }
}
