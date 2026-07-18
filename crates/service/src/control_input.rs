//! Continuous, single-owner, non-real-time control-input collection (issue #177).
//!
//! [`ControlInputService`] feeds the vendor-neutral [`ControlProjector`]
//! (issue #169) continuously from the existing device/input owner, turning
//! decoded [`DeviceInputs`](racing_wheel_engine::DeviceInputs) snapshots into
//! ordered [`ControlStreamItem`]s and pushing them — tagged with their source
//! [`DeviceIdentity`] — into a **bounded internal producer channel**.
//!
//! Scope and boundaries (external-control-stream plan, work item
//! `non-rt-input-service`, issue #177):
//!
//! * **Single owner.** The collector opens each input-capable device **once**
//!   (via the shared [`HidPort`]) and holds that handle for the lifetime of the
//!   connection. It does not open a second HID handle per device and does not
//!   create a second [`HidPort`].
//! * **Non-RT only.** Collection runs on a Tokio worker on a polling cadence; it
//!   never touches the 1 kHz FFB path and performs no FFB output.
//! * **Bounded producer.** Items flow into a bounded [`mpsc`] channel. On
//!   producer overflow (a lagging consumer) the collector forces an explicit
//!   [`ResetReason::Overflow`] reset and a fresh baseline rather than silently
//!   dropping events unnoticed.
//! * **No public subscriber/gRPC API here.** Multi-subscriber broadcasting is a
//!   separate work item (#178); this module only exposes the single internal
//!   receiver returned by [`ControlInputService::new`].
//!
//! The collection *logic* lives in [`ControlInputCollector`], driven
//! deterministically by explicit `on_connected` / `on_disconnected` /
//! `poll_once` calls; [`ControlInputService`] wraps it in a lifecycle-managed
//! worker. Keeping the two separate makes projection behaviour testable without
//! timers or sleeps.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use openracing_device_types::{ControlProjector, ControlStreamItem, DeviceIdentity, ResetReason};
use racing_wheel_engine::{DeviceEvent, DeviceInfo, HidDevice, HidPort};
use racing_wheel_schemas::prelude::DeviceId;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Mapping/contract version the collector stamps onto projected surfaces.
///
/// Bumped when the projected control contract changes in a way consumers must
/// notice. Kept at 1 for the initial input-only scope.
pub const CONTROL_MAPPING_VERSION: u32 = 1;

/// Default bound for the internal producer channel.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Default non-RT polling cadence for reading decoded input snapshots.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(8);

/// A control-stream item tagged with the identity of the device that produced
/// it, so a shared producer channel carrying multiple devices stays
/// unambiguous.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectedControlItem {
    /// Stable identity of the source device (instance survives reconnect).
    pub device: DeviceIdentity,
    /// The projected stream item.
    pub item: ControlStreamItem,
}

/// Configuration for a [`ControlInputService`].
#[derive(Debug, Clone)]
pub struct ControlInputConfig {
    /// Bound on the internal producer channel.
    pub channel_capacity: usize,
    /// Non-RT polling cadence.
    pub poll_interval: Duration,
}

impl Default for ControlInputConfig {
    fn default() -> Self {
        Self {
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

/// Health/diagnostic counters for the collector.
///
/// All counters are monotonic since construction and safe to read from any
/// thread, so a lagging or resetting stream is observable without a subscriber
/// attached.
#[derive(Debug, Default)]
pub struct ControlInputMetrics {
    /// Number of devices currently being collected from.
    pub active_devices: AtomicUsize,
    /// Total stream items successfully pushed to the producer channel.
    pub items_emitted: AtomicU64,
    /// Total items dropped because the bounded channel was full.
    pub dropped_items: AtomicU64,
    /// Total reset items emitted (disconnect, overflow, ...).
    pub resets: AtomicU64,
    /// Total polls where a device returned `None` from `read_inputs()`.
    pub none_reads: AtomicU64,
    /// Last sequence number pushed for any device.
    pub last_seq: AtomicU64,
}

/// Per-device collection state: the single owned handle plus its projector.
struct ActiveDevice {
    handle: Box<dyn HidDevice>,
    projector: ControlProjector,
    /// Set when a channel-full condition forced us to drop items; the next
    /// successful send emits a [`ResetReason::Overflow`] so the consumer learns
    /// a gap occurred and re-baselines.
    pending_overflow: bool,
}

/// Deterministic core of the collector: owns one handle + projector per active
/// device and pushes tagged items into the producer channel.
///
/// This type performs no timing of its own; a caller (production worker or a
/// test) drives it with explicit `on_connected` / `on_disconnected` /
/// `poll_once` calls.
pub struct ControlInputCollector {
    hid_port: Arc<dyn HidPort>,
    item_tx: mpsc::Sender<CollectedControlItem>,
    /// Currently connected devices (each owns its HID handle).
    active: HashMap<DeviceId, ActiveDevice>,
    /// Projectors for disconnected devices, retained so a reconnect preserves
    /// the device instance and continues the monotonic epoch sequence.
    dormant: HashMap<DeviceId, ControlProjector>,
    /// Monotonic source of new logical instance ids for first-seen devices.
    next_instance: u64,
    metrics: Arc<ControlInputMetrics>,
}

impl ControlInputCollector {
    /// Create a collector that opens handles via `hid_port` and pushes items to
    /// `item_tx`.
    pub fn new(
        hid_port: Arc<dyn HidPort>,
        item_tx: mpsc::Sender<CollectedControlItem>,
        metrics: Arc<ControlInputMetrics>,
    ) -> Self {
        Self {
            hid_port,
            item_tx,
            active: HashMap::new(),
            dormant: HashMap::new(),
            next_instance: 0,
            metrics,
        }
    }

    /// Number of devices currently being collected from.
    pub fn active_device_count(&self) -> usize {
        self.active.len()
    }

    /// Register a connected device: open its handle **once** and attach a
    /// projector.
    ///
    /// A device reconnecting after a disconnect reuses its retained projector,
    /// preserving its [`DeviceIdentity::instance`] and continuing the monotonic
    /// epoch sequence. A first-seen device gets a fresh instance. The
    /// non-actionable baseline is emitted on the first [`Self::poll_once`], not
    /// here, so a connect never synthesizes actions.
    ///
    /// Idempotent: an already-active device is left untouched.
    pub async fn on_connected(&mut self, info: DeviceInfo) {
        if self.active.contains_key(&info.id) {
            debug!(device = %info.id, "control-input: device already collected; ignoring connect");
            return;
        }

        let handle = match self.hid_port.open_device(&info.id).await {
            Ok(handle) => handle,
            Err(err) => {
                warn!(device = %info.id, error = %err, "control-input: failed to open device");
                return;
            }
        };

        // Reuse a retained projector on reconnect; otherwise start a fresh one.
        // A retained projector was already reset on disconnect, so its next
        // observe re-baselines in the post-disconnect epoch.
        let projector = match self.dormant.remove(&info.id) {
            Some(projector) => projector,
            None => {
                let instance = self.next_instance;
                self.next_instance += 1;
                let identity = DeviceIdentity {
                    vendor_id: info.vendor_id,
                    product_id: info.product_id,
                    serial: info.serial_number.clone(),
                    instance,
                };
                ControlProjector::new(identity, CONTROL_MAPPING_VERSION)
            }
        };

        self.active.insert(
            info.id.clone(),
            ActiveDevice {
                handle,
                projector,
                pending_overflow: false,
            },
        );
        self.metrics
            .active_devices
            .store(self.active.len(), Ordering::Relaxed);
        debug!(device = %info.id, "control-input: now collecting");
    }

    /// Handle a disconnect: emit a [`ResetReason::Disconnect`] reset, drop the
    /// owned handle, and retain the projector so a later reconnect continues the
    /// same device instance and epoch sequence.
    pub fn on_disconnected(&mut self, id: &DeviceId, timestamp_ns: u64) {
        if let Some(mut active) = self.active.remove(id) {
            let device = active.projector.device().clone();
            let reset = active
                .projector
                .reset(ResetReason::Disconnect, timestamp_ns);
            self.push(device, reset);
            self.dormant.insert(id.clone(), active.projector);
            self.metrics
                .active_devices
                .store(self.active.len(), Ordering::Relaxed);
            debug!(device = %id, "control-input: device disconnected, reset emitted");
        }
    }

    /// Poll every active device once, projecting any changes into the producer
    /// channel. Devices that return `None` from `read_inputs()` are skipped
    /// without busy-looping or crashing.
    pub fn poll_once(&mut self, timestamp_ns: u64) {
        // Borrow disjoint fields so the send logic can run while iterating the
        // active-device map mutably.
        let tx = &self.item_tx;
        let metrics: &ControlInputMetrics = &self.metrics;

        for active in self.active.values_mut() {
            let device = active.projector.device().clone();

            // Clear a prior overflow first: emit an explicit gap/reset once the
            // channel has room again, then re-baseline on the next observe.
            if active.pending_overflow {
                match tx.try_reserve() {
                    Ok(permit) => {
                        let reset = active.projector.reset(ResetReason::Overflow, timestamp_ns);
                        record_push(metrics, &reset);
                        permit.send(CollectedControlItem {
                            device: device.clone(),
                            item: reset,
                        });
                        active.pending_overflow = false;
                    }
                    Err(_) => continue, // still full; retry next poll
                }
            }

            let inputs = match active.handle.read_inputs() {
                Some(inputs) => inputs,
                None => {
                    metrics.none_reads.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };

            for item in active.projector.observe(&inputs, timestamp_ns) {
                match tx.try_reserve() {
                    Ok(permit) => {
                        record_push(metrics, &item);
                        permit.send(CollectedControlItem {
                            device: device.clone(),
                            item,
                        });
                    }
                    Err(mpsc::error::TrySendError::Full(())) => {
                        metrics.dropped_items.fetch_add(1, Ordering::Relaxed);
                        active.pending_overflow = true;
                        break;
                    }
                    Err(mpsc::error::TrySendError::Closed(())) => return,
                }
            }
        }
    }

    /// Best-effort tagged push of a single item (used for disconnect resets).
    fn push(&self, device: DeviceIdentity, item: ControlStreamItem) {
        let is_reset = matches!(item, ControlStreamItem::Reset { .. });
        let seq = item.meta().seq;
        match self.item_tx.try_send(CollectedControlItem { device, item }) {
            Ok(()) => {
                self.metrics.items_emitted.fetch_add(1, Ordering::Relaxed);
                self.metrics.last_seq.store(seq, Ordering::Relaxed);
                if is_reset {
                    self.metrics.resets.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.metrics.dropped_items.fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }
}

/// Update counters for a successfully pushed item.
fn record_push(metrics: &ControlInputMetrics, item: &ControlStreamItem) {
    metrics.items_emitted.fetch_add(1, Ordering::Relaxed);
    metrics.last_seq.store(item.meta().seq, Ordering::Relaxed);
    if matches!(item, ControlStreamItem::Reset { .. }) {
        metrics.resets.fetch_add(1, Ordering::Relaxed);
    }
}

/// Lifecycle-managed control-input collection service.
///
/// [`Self::new`] returns the service together with the receiving end of the
/// bounded producer channel; the caller (or, later, the #178 broadcaster) owns
/// that receiver. [`Self::start`] spawns a non-RT worker that reacts to device
/// connect/disconnect events and polls decoded inputs; [`Self::stop`]
/// deterministically cancels and joins the worker.
pub struct ControlInputService {
    hid_port: Arc<dyn HidPort>,
    config: ControlInputConfig,
    item_tx: Option<mpsc::Sender<CollectedControlItem>>,
    metrics: Arc<ControlInputMetrics>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl ControlInputService {
    /// Create a service and its bounded producer receiver.
    pub fn new(
        hid_port: Arc<dyn HidPort>,
        config: ControlInputConfig,
    ) -> (Self, mpsc::Receiver<CollectedControlItem>) {
        let (item_tx, item_rx) = mpsc::channel(config.channel_capacity.max(1));
        let service = Self {
            hid_port,
            config,
            item_tx: Some(item_tx),
            metrics: Arc::new(ControlInputMetrics::default()),
            shutdown_tx: None,
            worker: None,
        };
        (service, item_rx)
    }

    /// Shared health/diagnostic counters.
    pub fn metrics(&self) -> Arc<ControlInputMetrics> {
        self.metrics.clone()
    }

    /// Whether the collection worker is currently running.
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    /// Start the non-RT collection worker.
    ///
    /// Subscribes to device connect/disconnect events, seeds any
    /// already-connected devices, and polls decoded inputs on the configured
    /// cadence until [`Self::stop`] is called or the producer consumer is
    /// dropped. Calling `start` while already running is a no-op.
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker.is_some() {
            return Ok(());
        }
        let item_tx = match self.item_tx.take() {
            Some(tx) => tx,
            None => return Err("control-input producer channel already consumed".into()),
        };

        let mut events = self.hid_port.monitor_devices().await?;
        let initial = self.hid_port.list_devices().await.unwrap_or_default();

        let mut collector =
            ControlInputCollector::new(self.hid_port.clone(), item_tx, self.metrics.clone());
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let poll_interval = self.config.poll_interval;

        let worker = tokio::spawn(async move {
            for info in initial {
                collector.on_connected(info).await;
            }

            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => collector.poll_once(now_ns()),
                    event = events.recv() => {
                        match event {
                            Some(DeviceEvent::Connected(info)) => collector.on_connected(info).await,
                            Some(DeviceEvent::Disconnected(info)) => {
                                collector.on_disconnected(&info.id, now_ns());
                            }
                            None => {
                                // Monitor stream closed; keep polling known devices.
                            }
                        }
                    }
                    _ = &mut shutdown_rx => break,
                }
            }
            debug!("control-input: worker stopped");
        });

        self.shutdown_tx = Some(shutdown_tx);
        self.worker = Some(worker);
        Ok(())
    }

    /// Deterministically stop and join the collection worker.
    pub async fn stop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.await;
        }
    }
}

/// Monotonic source timestamp in nanoseconds for stream ordering.
fn now_ns() -> u64 {
    crate::telemetry::telemetry_now_ns()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use openracing_device_types::{ControlValue, DeviceInputs, RawControlId};
    use racing_wheel_engine::{
        DeviceHealthStatus, RTResult, TelemetryData, VirtualDevice, VirtualHidPort,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Shared, scriptable queue of decoded input snapshots for one device.
    type InputQueue = Arc<Mutex<VecDeque<DeviceInputs>>>;

    /// A `HidDevice` that returns pre-scripted `DeviceInputs` from `read_inputs`.
    struct ScriptedDevice {
        info: DeviceInfo,
        health: DeviceHealthStatus,
        queue: InputQueue,
    }

    impl HidDevice for ScriptedDevice {
        fn write_ffb_report(&mut self, _torque_nm: f32, _seq: u16) -> RTResult {
            Ok(())
        }
        fn read_telemetry(&mut self) -> Option<TelemetryData> {
            None
        }
        fn capabilities(&self) -> &racing_wheel_schemas::prelude::DeviceCapabilities {
            &self.info.capabilities
        }
        fn device_info(&self) -> &DeviceInfo {
            &self.info
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn health_status(&self) -> DeviceHealthStatus {
            self.health.clone()
        }
        fn read_inputs(&self) -> Option<DeviceInputs> {
            match self.queue.lock() {
                Ok(mut q) => q.pop_front(),
                Err(_) => None,
            }
        }
    }

    /// A `HidPort` that opens `ScriptedDevice`s sharing one input queue and
    /// counts opens so tests can assert a single handle per connection.
    struct ScriptedPort {
        info: DeviceInfo,
        queue: InputQueue,
        opens: AtomicUsize,
    }

    impl ScriptedPort {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let id: DeviceId = "scripted-0".parse()?;
            // Borrow a valid DeviceCapabilities/DeviceInfo from a VirtualDevice.
            let vdev = VirtualDevice::new(id, "Scripted Wheel".to_string());
            Ok(Self {
                info: vdev.device_info().clone(),
                queue: Arc::new(Mutex::new(VecDeque::new())),
                opens: AtomicUsize::new(0),
            })
        }

        fn push_inputs(&self, inputs: DeviceInputs) {
            if let Ok(mut q) = self.queue.lock() {
                q.push_back(inputs);
            }
        }

        fn open_count(&self) -> usize {
            self.opens.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl HidPort for ScriptedPort {
        async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
            Ok(vec![self.info.clone()])
        }
        async fn open_device(
            &self,
            _id: &DeviceId,
        ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
            self.opens.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(ScriptedDevice {
                info: self.info.clone(),
                health: DeviceHealthStatus {
                    temperature_c: 30,
                    fault_flags: 0,
                    hands_on: false,
                    last_communication: std::time::Instant::now(),
                    communication_errors: 0,
                },
                queue: self.queue.clone(),
            }))
        }
        async fn monitor_devices(
            &self,
        ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
            let (_tx, rx) = mpsc::channel(1);
            Ok(rx)
        }
        async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    fn collector_with(
        port: Arc<ScriptedPort>,
        capacity: usize,
    ) -> (ControlInputCollector, mpsc::Receiver<CollectedControlItem>) {
        let (tx, rx) = mpsc::channel(capacity);
        let metrics = Arc::new(ControlInputMetrics::default());
        (ControlInputCollector::new(port, tx, metrics), rx)
    }

    fn drain(rx: &mut mpsc::Receiver<CollectedControlItem>) -> Vec<CollectedControlItem> {
        let mut out = Vec::new();
        while let Ok(item) = rx.try_recv() {
            out.push(item);
        }
        out
    }

    fn button_events(items: &[CollectedControlItem]) -> Vec<(RawControlId, ControlValue)> {
        items
            .iter()
            .filter_map(|c| match &c.item {
                ControlStreamItem::Event { event, .. } => Some((event.raw_id, event.value)),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn connect_then_poll_emits_baseline_then_edges() -> Result<(), Box<dyn std::error::Error>>
    {
        let port = Arc::new(ScriptedPort::new()?);
        let (mut collector, mut rx) = collector_with(port.clone(), 64);

        collector.on_connected(port.info.clone()).await;
        assert_eq!(port.open_count(), 1, "device opened exactly once");
        assert_eq!(collector.active_device_count(), 1);

        // A real device returns its current snapshot immediately; the mock is
        // seeded so the first poll baselines from it.
        port.push_inputs(DeviceInputs::default());
        collector.poll_once(1_000);
        let first = drain(&mut rx);
        assert_eq!(first.len(), 1, "first poll emits one baseline");
        assert!(matches!(
            first[0].item,
            ControlStreamItem::InitialSnapshot { .. }
        ));

        // A subsequent snapshot with a button pressed projects one edge.
        let mut pressed = DeviceInputs::default();
        pressed.set_button(5, true);
        port.push_inputs(pressed);
        collector.poll_once(2_000);
        let edges = button_events(&drain(&mut rx));
        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0],
            (RawControlId::button(5), ControlValue::Button(true))
        );

        // The handle is reused: still exactly one open.
        assert_eq!(port.open_count(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn none_read_is_handled_without_crash_or_event() -> Result<(), Box<dyn std::error::Error>>
    {
        let port = Arc::new(ScriptedPort::new()?);
        let (mut collector, mut rx) = collector_with(port.clone(), 64);
        collector.on_connected(port.info.clone()).await;

        // Empty queue => read_inputs returns None; polls must be no-ops.
        collector.poll_once(1);
        collector.poll_once(2);
        assert!(drain(&mut rx).is_empty(), "None reads produce no events");
        assert!(collector.metrics.none_reads.load(Ordering::Relaxed) >= 2);
        Ok(())
    }

    #[tokio::test]
    async fn disconnect_emits_reset_and_drops_handle() -> Result<(), Box<dyn std::error::Error>> {
        let port = Arc::new(ScriptedPort::new()?);
        let (mut collector, mut rx) = collector_with(port.clone(), 64);
        collector.on_connected(port.info.clone()).await;
        port.push_inputs(DeviceInputs::default());
        collector.poll_once(1); // baseline
        let _ = drain(&mut rx);

        collector.on_disconnected(&port.info.id, 10);
        assert_eq!(collector.active_device_count(), 0);
        let items = drain(&mut rx);
        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0].item,
            ControlStreamItem::Reset {
                reason: ResetReason::Disconnect,
                ..
            }
        ));
        Ok(())
    }

    #[tokio::test]
    async fn reconnect_preserves_instance_and_advances_epoch()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = Arc::new(ScriptedPort::new()?);
        let (mut collector, mut rx) = collector_with(port.clone(), 64);

        collector.on_connected(port.info.clone()).await;
        port.push_inputs(DeviceInputs::default());
        collector.poll_once(1);
        let baseline0 = drain(&mut rx);
        let instance0 = baseline0[0].device.instance;
        let epoch0 = baseline0[0].item.meta().epoch;

        // Disconnect (emits reset, bumps epoch, retains projector).
        collector.on_disconnected(&port.info.id, 2);
        let _ = drain(&mut rx);

        // Reconnect: re-open the handle, reuse the retained projector.
        collector.on_connected(port.info.clone()).await;
        assert_eq!(port.open_count(), 2, "reconnect re-opens the handle");
        port.push_inputs(DeviceInputs::default());
        collector.poll_once(3);
        let baseline1 = drain(&mut rx);
        assert!(matches!(
            baseline1[0].item,
            ControlStreamItem::InitialSnapshot { .. }
        ));
        assert_eq!(
            baseline1[0].device.instance, instance0,
            "device instance survives reconnect"
        );
        assert!(
            baseline1[0].item.meta().epoch > epoch0,
            "reconnect continues a later epoch"
        );
        Ok(())
    }

    #[tokio::test]
    async fn channel_overflow_forces_explicit_reset() -> Result<(), Box<dyn std::error::Error>> {
        // Capacity 1: the baseline fills the channel, so the next poll's event
        // cannot be sent and must become an explicit overflow reset.
        let port = Arc::new(ScriptedPort::new()?);
        let (mut collector, mut rx) = collector_with(port.clone(), 1);
        collector.on_connected(port.info.clone()).await;

        port.push_inputs(DeviceInputs::default());
        collector.poll_once(1); // baseline occupies the single slot

        let mut pressed = DeviceInputs::default();
        pressed.set_button(1, true);
        port.push_inputs(pressed);
        collector.poll_once(2); // edge cannot be sent -> dropped, overflow flagged
        assert!(collector.metrics.dropped_items.load(Ordering::Relaxed) >= 1);

        // Free the slot by draining the baseline, then poll again: an explicit
        // Overflow reset must be emitted before anything else.
        let baseline = rx.try_recv();
        assert!(matches!(
            baseline.map(|c| c.item),
            Ok(ControlStreamItem::InitialSnapshot { .. })
        ));
        collector.poll_once(3);
        let items = drain(&mut rx);
        assert!(
            items.iter().any(|c| matches!(
                c.item,
                ControlStreamItem::Reset {
                    reason: ResetReason::Overflow,
                    ..
                }
            )),
            "overflow must surface as an explicit reset"
        );
        assert!(collector.metrics.resets.load(Ordering::Relaxed) >= 1);
        Ok(())
    }

    #[tokio::test]
    async fn service_start_and_stop_are_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        // Uses the engine VirtualHidPort: the worker must start, seed the
        // virtual device, and stop cleanly on request.
        let mut port = VirtualHidPort::new();
        let id: DeviceId = "virtual-wheel-0".parse()?;
        port.add_device(VirtualDevice::new(id, "Test Wheel".to_string()))?;
        let port: Arc<dyn HidPort> = Arc::new(port);

        let (mut service, _rx) = ControlInputService::new(port, ControlInputConfig::default());
        assert!(!service.is_running());
        service.start().await?;
        assert!(service.is_running());

        // Second start is a no-op, not a panic or a second worker.
        service.start().await?;

        service.stop().await;
        assert!(!service.is_running());
        Ok(())
    }
}
