//! Bounded multi-subscriber broadcast of the collected control stream (issue #178).
//!
//! [`ControlBroadcaster`] takes the single bounded producer stream of
//! [`ControlStreamItem`]s — as produced by the non-real-time control-input
//! collector from issue #177 — and fans it out to any number of independent,
//! read-only subscribers. Each subscriber owns its own bounded queue.
//!
//! A subscriber that falls behind is **never** silently skipped: it is cut over
//! to an explicit, typed recoverable [`SubscriptionEvent::Lagged`] outcome so it
//! can resubscribe and resynchronise from a fresh descriptor/baseline. This
//! preserves per-subscriber ordering and the producer's sequence/epoch semantics
//! without forging synthetic sequence numbers.
//!
//! This layer adds no protobuf/gRPC, no HID access, and no Runbook/application
//! semantics, and performs no work on the 1 kHz FFB path.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use openracing_device_types::ControlStreamItem;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

/// Default per-subscriber queue capacity.
pub const DEFAULT_SUBSCRIBER_CAPACITY: usize = 256;

/// Outcome of awaiting the next item on a [`ControlSubscription`].
#[derive(Debug)]
pub enum SubscriptionEvent {
    /// The next ordered stream item.
    Item(ControlStreamItem),
    /// The subscriber fell behind and was cut from the broadcast. Any items that
    /// were already buffered are delivered before this outcome; the consumer
    /// should resubscribe to resynchronise from a fresh descriptor/baseline.
    /// This is a *recoverable* condition, not a hard failure.
    Lagged,
    /// The broadcast source closed (e.g. service shutdown). Terminal.
    Closed,
}

/// A point-in-time metrics/health snapshot of a [`ControlBroadcaster`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BroadcastMetrics {
    /// Subscribers currently attached.
    pub active_subscribers: usize,
    /// Total subscribers ever created.
    pub total_subscribers: u64,
    /// Source items fanned out since creation.
    pub items_broadcast: u64,
    /// Subscribers cut over to `Lagged` due to overflow.
    pub subscriber_lags: u64,
    /// Sequence number of the most recently broadcast item.
    pub last_sequence: u64,
    /// Epoch of the most recently broadcast item.
    pub last_epoch: u32,
}

/// A single read-only subscription to the broadcast control stream.
pub struct ControlSubscription {
    rx: mpsc::Receiver<ControlStreamItem>,
    lagged: Arc<AtomicBool>,
}

impl ControlSubscription {
    /// Await the next [`SubscriptionEvent`].
    ///
    /// Returns buffered [`SubscriptionEvent::Item`]s first; once the underlying
    /// channel closes, resolves to [`SubscriptionEvent::Lagged`] if this
    /// subscriber was cut for overflow, otherwise [`SubscriptionEvent::Closed`].
    pub async fn recv(&mut self) -> SubscriptionEvent {
        match self.rx.recv().await {
            Some(item) => SubscriptionEvent::Item(item),
            None => self.terminal_event(),
        }
    }

    /// Non-blocking variant of [`recv`](Self::recv). Returns `None` when no item
    /// is currently buffered and the channel is still open.
    pub fn try_recv(&mut self) -> Option<SubscriptionEvent> {
        match self.rx.try_recv() {
            Ok(item) => Some(SubscriptionEvent::Item(item)),
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => Some(self.terminal_event()),
        }
    }

    fn terminal_event(&self) -> SubscriptionEvent {
        if self.lagged.load(Ordering::Acquire) {
            SubscriptionEvent::Lagged
        } else {
            SubscriptionEvent::Closed
        }
    }
}

struct Subscriber {
    tx: mpsc::Sender<ControlStreamItem>,
    lagged: Arc<AtomicBool>,
}

#[derive(Default)]
struct Metrics {
    total_subscribers: AtomicU64,
    items_broadcast: AtomicU64,
    subscriber_lags: AtomicU64,
    last_sequence: AtomicU64,
    last_epoch: AtomicU32,
}

struct Shared {
    subscribers: Mutex<Vec<Subscriber>>,
    /// Set once the pump has stopped (source closed or explicit shutdown), under
    /// the `subscribers` lock, so a `subscribe` that races termination cannot
    /// register a subscriber that nothing will ever service.
    closed: AtomicBool,
    metrics: Metrics,
    capacity: usize,
}

/// Fans a bounded control-item source out to many bounded subscribers.
pub struct ControlBroadcaster {
    shared: Arc<Shared>,
    pump: Mutex<Option<JoinHandle<()>>>,
}

impl ControlBroadcaster {
    /// Create a broadcaster that pumps `source` to subscribers, each with the
    /// default per-subscriber capacity.
    #[must_use]
    pub fn new(source: mpsc::Receiver<ControlStreamItem>) -> Self {
        Self::with_capacity(source, DEFAULT_SUBSCRIBER_CAPACITY)
    }

    /// Create a broadcaster with an explicit per-subscriber queue capacity.
    #[must_use]
    pub fn with_capacity(source: mpsc::Receiver<ControlStreamItem>, capacity: usize) -> Self {
        let shared = Arc::new(Shared {
            subscribers: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            metrics: Metrics::default(),
            capacity: capacity.max(1),
        });
        let pump = tokio::spawn(run_pump(Arc::clone(&shared), source));
        Self {
            shared,
            pump: Mutex::new(Some(pump)),
        }
    }

    /// Attach a new read-only subscriber. Subscribers only receive items
    /// broadcast after they attach.
    ///
    /// If the broadcast has already ended (source closed or [`shutdown`](Self::shutdown)),
    /// the returned subscription resolves immediately to
    /// [`SubscriptionEvent::Closed`] rather than blocking forever.
    pub async fn subscribe(&self) -> ControlSubscription {
        let (tx, rx) = mpsc::channel(self.shared.capacity);
        let lagged = Arc::new(AtomicBool::new(false));
        {
            // Register (and count) under the same lock the termination paths use,
            // so this either lands before termination (and is cleared by it) or
            // observes `closed` and drops `tx` immediately — never a stuck sender.
            let mut subs = self.shared.subscribers.lock().await;
            self.shared
                .metrics
                .total_subscribers
                .fetch_add(1, Ordering::Relaxed);
            if !self.shared.closed.load(Ordering::Acquire) {
                subs.push(Subscriber {
                    tx,
                    lagged: Arc::clone(&lagged),
                });
            }
            // else: drop `tx` (by not storing it); `rx` then closes -> Closed.
        }
        ControlSubscription { rx, lagged }
    }

    /// Take a metrics/health snapshot.
    pub async fn metrics(&self) -> BroadcastMetrics {
        // Prune subscribers whose receivers have been dropped so the reported
        // count reflects genuinely attached consumers even while the source is
        // idle (a dropped subscription is otherwise only reaped on the next item).
        let active = {
            let mut subs = self.shared.subscribers.lock().await;
            subs.retain(|s| !s.tx.is_closed());
            subs.len()
        };
        let m = &self.shared.metrics;
        // Load the count first with Acquire so the paired Release increment in
        // the pump guarantees the last_sequence/last_epoch stores for that item
        // are visible — the snapshot never reports N items with item N-1's meta.
        let items_broadcast = m.items_broadcast.load(Ordering::Acquire);
        BroadcastMetrics {
            active_subscribers: active,
            total_subscribers: m.total_subscribers.load(Ordering::Relaxed),
            items_broadcast,
            subscriber_lags: m.subscriber_lags.load(Ordering::Relaxed),
            last_sequence: m.last_sequence.load(Ordering::Relaxed),
            last_epoch: m.last_epoch.load(Ordering::Relaxed),
        }
    }

    /// Stop pumping and drop all subscribers (which then observe
    /// [`SubscriptionEvent::Closed`]). Idempotent.
    pub async fn shutdown(&self) {
        if let Some(pump) = self.pump.lock().await.take() {
            pump.abort();
            let _ = pump.await;
        }
        let mut subs = self.shared.subscribers.lock().await;
        self.shared.closed.store(true, Ordering::Release);
        subs.clear();
    }
}

async fn run_pump(shared: Arc<Shared>, mut source: mpsc::Receiver<ControlStreamItem>) {
    while let Some(item) = source.recv().await {
        let meta = *item.meta();
        {
            let mut subs = shared.subscribers.lock().await;
            let mut i = 0;
            while i < subs.len() {
                match subs[i].tx.try_send(item.clone()) {
                    Ok(()) => i += 1,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        // Overflow: cut this subscriber over to a recoverable
                        // `Lagged` outcome rather than silently dropping events.
                        subs[i].lagged.store(true, Ordering::Release);
                        shared
                            .metrics
                            .subscriber_lags
                            .fetch_add(1, Ordering::Relaxed);
                        // Dropping the sender lets the receiver drain its buffer
                        // and then observe the `Lagged` terminal event.
                        subs.swap_remove(i);
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Consumer dropped its subscription: remove silently.
                        subs.swap_remove(i);
                    }
                }
            }
        }
        let m = &shared.metrics;
        // Publish the item's metadata first, then make the count the release
        // point: a reader that observes the incremented count (Acquire) is
        // guaranteed to see this item's last_sequence/last_epoch, so the
        // snapshot is always coherent.
        m.last_sequence.store(meta.seq, Ordering::Relaxed);
        m.last_epoch.store(meta.epoch, Ordering::Relaxed);
        m.items_broadcast.fetch_add(1, Ordering::Release);
    }
    // Source closed: mark closed and drop every subscriber (under the lock) so
    // current consumers observe `Closed` and any later `subscribe` does too.
    let mut subs = shared.subscribers.lock().await;
    shared.closed.store(true, Ordering::Release);
    subs.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use openracing_device_types::{ControlEvent, ControlValue, RawControlId, StreamMeta};
    use std::time::Duration;
    use tokio::time::timeout;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn event_item(seq: u64, epoch: u32) -> ControlStreamItem {
        ControlStreamItem::Event {
            meta: StreamMeta {
                seq,
                timestamp_ns: seq,
                epoch,
            },
            event: ControlEvent {
                raw_id: RawControlId::button(0),
                value: ControlValue::Button(true),
                delta: None,
            },
        }
    }

    async fn recv(
        sub: &mut ControlSubscription,
    ) -> Result<SubscriptionEvent, Box<dyn std::error::Error>> {
        Ok(timeout(Duration::from_secs(1), sub.recv()).await?)
    }

    async fn wait_until<F>(b: &ControlBroadcaster, pred: F) -> BroadcastMetrics
    where
        F: Fn(&BroadcastMetrics) -> bool,
    {
        for _ in 0..400 {
            let m = b.metrics().await;
            if pred(&m) {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        b.metrics().await
    }

    #[tokio::test]
    async fn two_subscribers_receive_independent_ordered_streams() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);
        let mut s1 = b.subscribe().await;
        let mut s2 = b.subscribe().await;

        for seq in 0..3u64 {
            tx.send(event_item(seq, 0)).await?;
        }

        for seq in 0..3u64 {
            for sub in [&mut s1, &mut s2] {
                match recv(sub).await? {
                    SubscriptionEvent::Item(item) => assert_eq!(item.seq(), seq),
                    other => return Err(format!("expected item, got {other:?}").into()),
                }
            }
        }
        b.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn slow_subscriber_is_cut_over_to_lagged_not_silent_loss() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::with_capacity(rx, 2);
        let mut slow = b.subscribe().await;

        // Never drain `slow`; overflow its capacity-2 queue.
        for seq in 0..5u64 {
            tx.send(event_item(seq, 0)).await?;
        }

        // The pump must record exactly one lag and detach the subscriber.
        let m = wait_until(&b, |m| m.subscriber_lags >= 1).await;
        assert_eq!(m.subscriber_lags, 1);
        assert_eq!(m.active_subscribers, 0);

        // The two buffered items are still delivered, then an explicit Lagged.
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Item(_)));
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Item(_)));
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Lagged));

        b.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_lagged_subscriber_does_not_disturb_a_healthy_one() -> TestResult {
        // Per-subscriber capacity is 2. `slow` is never drained (so it will
        // overflow), while `fast` is drained between send batches so its own
        // queue never overflows — proving one subscriber's lag does not disturb
        // another's independent, ordered stream.
        let (tx, rx) = mpsc::channel(64);
        let b = ControlBroadcaster::with_capacity(rx, 2);
        let mut slow = b.subscribe().await;
        let mut fast = b.subscribe().await;

        // Batch 1: fill both queues to capacity, then drain only `fast`.
        tx.send(event_item(0, 0)).await?;
        tx.send(event_item(1, 0)).await?;
        for seq in 0..2u64 {
            match recv(&mut fast).await? {
                SubscriptionEvent::Item(item) => assert_eq!(item.seq(), seq),
                other => return Err(format!("fast expected {seq}, got {other:?}").into()),
            }
        }

        // Batch 2: `fast`'s queue is empty so it keeps up; `slow`'s is still full
        // so item 2 overflows it and cuts it over to Lagged.
        tx.send(event_item(2, 0)).await?;
        tx.send(event_item(3, 0)).await?;
        for seq in 2..4u64 {
            match recv(&mut fast).await? {
                SubscriptionEvent::Item(item) => assert_eq!(item.seq(), seq),
                other => return Err(format!("fast expected {seq}, got {other:?}").into()),
            }
        }

        // `slow` yields its two buffered items, then an explicit Lagged.
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Item(_)));
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Item(_)));
        assert!(matches!(recv(&mut slow).await?, SubscriptionEvent::Lagged));

        let m = b.metrics().await;
        assert_eq!(m.subscriber_lags, 1);
        assert_eq!(m.active_subscribers, 1, "the healthy subscriber remains");
        b.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn late_subscriber_only_receives_items_after_joining() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);

        // Broadcast one item before anyone subscribes; it is simply not delivered.
        tx.send(event_item(0, 0)).await?;
        wait_until(&b, |m| m.items_broadcast >= 1).await;

        let mut late = b.subscribe().await;
        tx.send(event_item(1, 0)).await?;

        match recv(&mut late).await? {
            SubscriptionEvent::Item(item) => assert_eq!(item.seq(), 1),
            other => return Err(format!("expected seq 1, got {other:?}").into()),
        }
        b.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn source_close_yields_closed_after_buffered_items() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);
        let mut sub = b.subscribe().await;

        tx.send(event_item(7, 2)).await?;
        drop(tx); // close the source

        assert!(matches!(recv(&mut sub).await?, SubscriptionEvent::Item(_)));
        assert!(matches!(recv(&mut sub).await?, SubscriptionEvent::Closed));
        Ok(())
    }

    #[tokio::test]
    async fn dropped_subscriber_is_pruned_from_active_count_while_idle() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);
        let s1 = b.subscribe().await;
        let _s2 = b.subscribe().await;
        assert_eq!(b.metrics().await.active_subscribers, 2);

        // Drop a subscription while no items are flowing; the next snapshot must
        // reflect the disconnect without waiting for an item to reap the sender.
        drop(s1);
        assert_eq!(b.metrics().await.active_subscribers, 1);
        drop(tx);
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_after_source_close_resolves_closed_not_hang() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);

        // An early subscriber observes Closed once the pump ends, which also
        // confirms the pump has finished before we subscribe again.
        let mut early = b.subscribe().await;
        drop(tx); // close the source
        assert!(matches!(recv(&mut early).await?, SubscriptionEvent::Closed));

        // A subscription created AFTER the broadcast ended must resolve to
        // Closed rather than blocking forever (the `recv` timeout would trip).
        let mut late = b.subscribe().await;
        assert!(matches!(recv(&mut late).await?, SubscriptionEvent::Closed));
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_after_shutdown_resolves_closed_not_hang() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);
        b.shutdown().await;

        let mut sub = b.subscribe().await;
        assert!(matches!(recv(&mut sub).await?, SubscriptionEvent::Closed));
        drop(tx);
        Ok(())
    }

    #[tokio::test]
    async fn metrics_track_last_sequence_and_epoch() -> TestResult {
        let (tx, rx) = mpsc::channel(16);
        let b = ControlBroadcaster::new(rx);
        let _sub = b.subscribe().await;

        tx.send(event_item(0, 0)).await?;
        tx.send(event_item(1, 0)).await?;
        tx.send(event_item(0, 1)).await?; // new epoch, sequence restarts

        let m = wait_until(&b, |m| m.items_broadcast >= 3).await;
        assert_eq!(m.items_broadcast, 3);
        assert_eq!(m.last_epoch, 1);
        assert_eq!(m.last_sequence, 0);
        assert_eq!(m.total_subscribers, 1);
        b.shutdown().await;
        Ok(())
    }
}
