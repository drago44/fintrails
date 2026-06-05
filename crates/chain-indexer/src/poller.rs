use std::time::Duration;

use tokio::sync::mpsc;

use crate::error::SourceError;
use crate::event::ChainEvent;
use crate::source::BlockSource;
use crate::tracker::{BlockRef, Tracker};

/// What the poller hands downstream as the chain advances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexerMessage {
    /// A `Transfer` in a block that has now reached the confirmation depth.
    /// Safe to act on; only retracted if a later [`Reorged`](Self::Reorged)
    /// names its block.
    Confirmed(ChainEvent),
    /// Blocks orphaned by a reorg, ascending. Any [`Confirmed`](Self::Confirmed)
    /// event whose `block_number` is among these must be retracted downstream.
    Reorged { orphaned: Vec<BlockRef> },
}

/// Live-tails a chain: repeatedly fetches new blocks, feeds them to a
/// [`Tracker`], and emits confirmed transfers and reorgs.
///
/// The poller owns the *fetching* (and the backward walk that re-pulls a forked
/// branch); the tracker owns the *bookkeeping* (confirmation depth, rollback).
/// Generic over [`BlockSource`] so the network is swappable for a fake in tests.
pub struct Poller<S> {
    source: S,
    tracker: Tracker,
    /// First block to index on a cold start (before the tracker holds anything).
    start_block: u64,
    poll_interval: Duration,
}

impl<S: BlockSource> Poller<S> {
    pub fn new(source: S, tracker: Tracker, start_block: u64, poll_interval: Duration) -> Self {
        Self {
            source,
            tracker,
            start_block,
            poll_interval,
        }
    }

    /// Polls once: pulls whatever is new since last time and returns the
    /// resulting messages. The unit of behaviour — [`run`](Self::run) is just
    /// this in a loop, so tests drive `advance` directly without a clock.
    pub async fn advance(&mut self) -> Result<Vec<IndexerMessage>, SourceError> {
        let head_num = self.source.head_number().await?;
        let mut msgs = Vec::new();

        match self.tracker.head().map(|h| h.number) {
            // Cold start: index forward from the configured floor.
            None => {
                for n in self.start_block..=head_num {
                    let block = self.source.block_ref(n).await?;
                    self.feed(block, &mut msgs).await?;
                }
            }
            // Height grew: feed each block above our head. `feed` reconciles any
            // reorg that also touched blocks at or below the old head.
            Some(th) if head_num > th => {
                for n in (th + 1)..=head_num {
                    let block = self.source.block_ref(n).await?;
                    self.feed(block, &mut msgs).await?;
                }
            }
            // Height did not grow. A reorg can still have replaced the head at
            // the same (or lower) height — detect it by a changed hash. If the
            // head block is identical, the whole ancestry is too: nothing new.
            Some(_) => {
                let head = self.source.block_ref(head_num).await?;
                let unchanged = self
                    .tracker
                    .block_at(head_num)
                    .is_some_and(|b| b.hash == head.hash);
                if !unchanged {
                    self.feed(head, &mut msgs).await?;
                }
            }
        }

        Ok(msgs)
    }

    /// Runs the poll loop forever, sending each message on `tx`. Returns `Ok`
    /// when the consumer drops the receiver; returns `Err` on the first RPC
    /// failure (retry/backoff is a later step).
    pub async fn run(mut self, tx: mpsc::Sender<IndexerMessage>) -> Result<(), SourceError> {
        loop {
            for msg in self.advance().await? {
                if tx.send(msg).await.is_err() {
                    return Ok(());
                }
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    /// Feeds one fetched block into the tracker, first walking back to the fork
    /// point if it does not attach to what we hold (a reorg). Confirmed and
    /// orphaned blocks from the resulting `observe`s become messages.
    async fn feed(
        &mut self,
        block: BlockRef,
        msgs: &mut Vec<IndexerMessage>,
    ) -> Result<(), SourceError> {
        // Walk back from `block`, fetching ancestors, until we reach one whose
        // parent the tracker already agrees with by hash. On the happy path this
        // stops immediately (the parent is our current head).
        let mut branch = vec![block];
        loop {
            let tip = branch.last().expect("branch is never empty");
            let Some(parent_num) = tip.number.checked_sub(1) else {
                break; // reached genesis
            };
            match self.tracker.block_at(parent_num) {
                // Tracker agrees on the parent: the branch connects here.
                Some(known) if known.hash == tip.parent_hash => break,
                // Different block at this height: the fork is deeper, keep going.
                Some(_) => {
                    let parent = self.source.block_ref(parent_num).await?;
                    branch.push(parent);
                }
                // Parent not retained (first block, or fork older than the
                // window): feed what we have; the tracker drains safely.
                None => break,
            }
        }

        // Feed fork-point-first so the tracker rolls back orphans on the first
        // observe and re-confirms as the new branch extends back to the head.
        for b in branch.into_iter().rev() {
            let update = self.tracker.observe(b);
            if !update.reorged.is_empty() {
                msgs.push(IndexerMessage::Reorged {
                    orphaned: update.reorged,
                });
            }
            // Confirmed blocks from one observe are a contiguous range; fetch
            // their transfers in a single call.
            if let (Some(first), Some(last)) = (update.confirmed.first(), update.confirmed.last()) {
                let events = self.source.transfers(first.number, last.number).await?;
                msgs.extend(events.into_iter().map(IndexerMessage::Confirmed));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{Address, B256, U256};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Linear canonical block: hash = number, parent_hash = number - 1.
    fn block(number: u64) -> BlockRef {
        BlockRef {
            number,
            hash: B256::from(U256::from(number)),
            parent_hash: B256::from(U256::from(number.saturating_sub(1))),
        }
    }

    /// Alternative-branch block: a distinct hash (0xee filler + number), linked
    /// to an explicit `parent_hash`.
    fn alt(number: u64, parent_hash: B256) -> BlockRef {
        let mut bytes = [0xee_u8; 32];
        bytes[..8].copy_from_slice(&number.to_be_bytes());
        BlockRef {
            number,
            hash: B256::from(bytes),
            parent_hash,
        }
    }

    fn transfer(block_number: u64, value: u64) -> ChainEvent {
        ChainEvent {
            token: Address::repeat_byte(0xaa),
            from: Address::repeat_byte(0x11),
            to: Address::repeat_byte(0x22),
            value: U256::from(value),
            block_number,
            tx_hash: B256::repeat_byte(0xbb),
            log_index: 0,
        }
    }

    /// In-memory chain the test mutates between `advance` calls. Shared via `Rc`
    /// so the test keeps a handle after handing a clone to the `Poller`.
    #[derive(Clone, Default)]
    struct FakeSource {
        inner: Rc<RefCell<State>>,
    }

    #[derive(Default)]
    struct State {
        /// Canonical blocks indexed by number (slot `n` is the block at height `n`).
        blocks: Vec<BlockRef>,
        events: Vec<ChainEvent>,
    }

    impl FakeSource {
        fn set_block(&self, b: BlockRef) {
            let mut s = self.inner.borrow_mut();
            let n = b.number as usize;
            if s.blocks.len() <= n {
                s.blocks.resize(n + 1, b.clone());
            }
            s.blocks[n] = b;
        }
        fn add_event(&self, e: ChainEvent) {
            self.inner.borrow_mut().events.push(e);
        }
    }

    impl BlockSource for FakeSource {
        async fn head_number(&self) -> Result<u64, SourceError> {
            Ok(self.inner.borrow().blocks.len() as u64 - 1)
        }
        async fn block_ref(&self, number: u64) -> Result<BlockRef, SourceError> {
            Ok(self.inner.borrow().blocks[number as usize].clone())
        }
        async fn transfers(&self, from: u64, to: u64) -> Result<Vec<ChainEvent>, SourceError> {
            Ok(self
                .inner
                .borrow()
                .events
                .iter()
                .filter(|e| (from..=to).contains(&e.block_number))
                .cloned()
                .collect())
        }
    }

    fn confirmed_events(msgs: &[IndexerMessage]) -> Vec<&ChainEvent> {
        msgs.iter()
            .filter_map(|m| match m {
                IndexerMessage::Confirmed(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn live_tail_emits_confirmed_transfers() {
        let src = FakeSource::default();
        for n in 0..=3 {
            src.set_block(block(n));
        }
        src.add_event(transfer(1, 500));

        let mut poller = Poller::new(src, Tracker::new(1), 0, Duration::ZERO);
        let msgs = poller.advance().await.unwrap();

        // Head 3 with N=1 confirms blocks 0..=2, so block 1's transfer surfaces.
        let events = confirmed_events(&msgs);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, U256::from(500u64));
        assert_eq!(events[0].block_number, 1);
    }

    #[tokio::test]
    async fn second_advance_only_emits_new_blocks() {
        let src = FakeSource::default();
        for n in 0..=2 {
            src.set_block(block(n));
        }
        let mut poller = Poller::new(src.clone(), Tracker::new(1), 0, Duration::ZERO);
        poller.advance().await.unwrap();

        // Extend the chain and place a transfer in the new block.
        src.set_block(block(3));
        src.add_event(transfer(2, 700));
        let msgs = poller.advance().await.unwrap();

        // Only block 2 newly confirmed (head 2 -> 3 with N=1), not a re-emit.
        let events = confirmed_events(&msgs);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].block_number, 2);
    }

    #[tokio::test]
    async fn same_height_hash_change_is_reported_as_reorg() {
        let src = FakeSource::default();
        for n in 0..=3 {
            src.set_block(block(n));
        }
        let mut poller = Poller::new(src.clone(), Tracker::new(2), 0, Duration::ZERO);
        poller.advance().await.unwrap();

        // Replace the head (block 3) with a competing block; height stays 3.
        src.set_block(alt(3, block(2).hash));
        let msgs = poller.advance().await.unwrap();

        assert!(msgs.contains(&IndexerMessage::Reorged {
            orphaned: vec![block(3)],
        }));
    }

    #[tokio::test]
    async fn deep_reorg_walks_back_and_rolls_orphans() {
        let src = FakeSource::default();
        for n in 0..=3 {
            src.set_block(block(n));
        }
        let mut poller = Poller::new(src.clone(), Tracker::new(1), 0, Duration::ZERO);
        poller.advance().await.unwrap(); // confirms 0,1,2

        // Reorg forks at block 1: blocks 2,3 are replaced and the branch extends
        // to a new head 4. Only the head's height growth is visible up front;
        // the poller must walk back to discover blocks 2' and 3'.
        let alt2 = alt(2, block(1).hash);
        let alt3 = alt(3, alt2.hash);
        let alt4 = alt(4, alt3.hash);
        src.set_block(alt2.clone());
        src.set_block(alt3);
        src.set_block(alt4);

        let msgs = poller.advance().await.unwrap();

        assert!(msgs.contains(&IndexerMessage::Reorged {
            orphaned: vec![block(2), block(3)],
        }));
    }
}
