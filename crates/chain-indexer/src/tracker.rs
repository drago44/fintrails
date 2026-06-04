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
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TrackerUpdate {
    /// Blocks that reached `N` confirmations on this observation, ascending.
    /// Their events are safe to emit downstream.
    pub confirmed: Vec<BlockRef>,
    /// Blocks orphaned by a reorg on this observation, ascending. Their events
    /// must be retracted downstream. Empty when no reorg occurred.
    pub reorged: Vec<BlockRef>,
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

    /// Feeds the next canonical block. Contract: the poller feeds blocks by
    /// ascending number and, on a reorg, re-feeds from the fork point. A block
    /// that does not extend the head triggers rollback of the orphaned tail.
    ///
    /// Returns the blocks newly confirmed and any orphaned by a reorg.
    pub fn observe(&mut self, block: BlockRef) -> TrackerUpdate {
        // Roll back conflicting blocks until `block` attaches to its parent
        // (or the chain empties). A non-empty `reorged` means a reorg happened.
        let mut reorged = Vec::new();
        while let Some(top) = self.chain.last() {
            let attaches = top.number + 1 == block.number && top.hash == block.parent_hash;
            if attaches {
                break;
            }
            reorged.push(self.chain.pop().expect("last() was Some"));
        }
        reorged.reverse(); // ascending by number, like `confirmed`
        self.chain.push(block);

        // If the rollback orphaned blocks at or below the watermark, they were
        // wrongly confirmed: pull the watermark back below the lowest of them.
        if let Some(lowest) = reorged.first().map(|b| b.number) {
            self.confirmed_through = match (self.confirmed_through, lowest.checked_sub(1)) {
                (Some(w), Some(max_allowed)) if w > max_allowed => Some(max_allowed),
                (Some(_), None) => None, // reorg reached block 0
                (current, _) => current,
            };
        }

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

        TrackerUpdate { confirmed, reorged }
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

    /// A block on an alternative branch: a hash distinct from `block(number)`
    /// (0xee filler + number) but linked to an explicit `parent_hash`.
    fn alt(number: u64, parent_hash: B256) -> BlockRef {
        let mut bytes = [0xee_u8; 32];
        bytes[..8].copy_from_slice(&number.to_be_bytes());
        BlockRef {
            number,
            hash: B256::from(bytes),
            parent_hash,
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

    #[test]
    fn shallow_reorg_replaces_the_head_and_reports_it() {
        let mut tracker = Tracker::new(2);
        for n in 0..=3 {
            tracker.observe(block(n));
        }
        assert_eq!(tracker.confirmed_height(), Some(1));

        // Block 3 is replaced by a competing block on a fork off block 2.
        let update = tracker.observe(alt(3, block(2).hash));
        assert_eq!(update.reorged, vec![block(3)]);
        assert!(update.confirmed.is_empty());
        // The reorg was above the confirmed height, so it stays put.
        assert_eq!(tracker.confirmed_height(), Some(1));
    }

    #[test]
    fn deep_reorg_rolls_back_several_blocks_then_extends_cleanly() {
        let mut tracker = Tracker::new(1);
        for n in 0..=3 {
            tracker.observe(block(n));
        }
        assert_eq!(tracker.confirmed_height(), Some(2));

        // Fork at block 1: blocks 2 and 3 are orphaned.
        let alt2 = alt(2, block(1).hash);
        let update = tracker.observe(alt2.clone());
        assert_eq!(update.reorged, vec![block(2), block(3)]);
        // Block 2 was confirmed; the watermark is pulled back below it.
        assert_eq!(tracker.confirmed_height(), Some(1));

        // The new branch then extends and confirms normally.
        let alt3 = alt(3, alt2.hash);
        let update = tracker.observe(alt3);
        assert_eq!(update.confirmed, vec![alt2]);
        assert_eq!(tracker.confirmed_height(), Some(2));
    }

    #[test]
    fn reorg_shallower_than_n_never_unconfirms() {
        // The whole point of N confirmations: a reorg shallower than N must not
        // touch already-confirmed blocks.
        let mut tracker = Tracker::new(3);
        for n in 0..=5 {
            tracker.observe(block(n));
        }
        assert_eq!(tracker.confirmed_height(), Some(2));

        // Depth-2 reorg (blocks 4, 5) — shallower than N=3.
        let update = tracker.observe(alt(4, block(3).hash));
        assert_eq!(update.reorged, vec![block(4), block(5)]);
        assert!(update.confirmed.is_empty());
        assert_eq!(tracker.confirmed_height(), Some(2)); // untouched
    }
}
