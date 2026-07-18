//! Non-real-time collection of decoded device inputs.
//!
//! [`ControlInputCollector`] owns one shared HID session per connected device.
//! It reads the already-decoded [`DeviceInputs`] snapshot on a bounded,
//! non-real-time cadence and feeds the transport-neutral projector. The
//! collector deliberately stops before subscriber fan-out; bounded broadcast,
//! lag, and metrics are the follow-up seam in issue #178.

use anyhow::{Context, Result};
use openracing_device_types::{ControlProjector, ControlStreamItem, DeviceIdentity, ResetReason};
use racing_wheel_engine::{DeviceEvent, DeviceInfo, HidDevice, HidPort};
use racing_wheel_schemas::prelude::DeviceId;
use std::collections::HashMap;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};
use tracing::{debug, warn};

/// Default capacity for the collector-to-broadcaster handoff.
pub const DEFAULT_CONTROL_INPUT_CAPACITY: usize = 256;

/// Default non-real-time input polling cadence.
pub const DEFAULT_CONTROL_INPUT_POLL_INTERVAL: Duration = Duration::from_millis(5);

type SharedHidDevice = Arc<Mutex<Box<dyn HidDevice>>>;

/// A bounded, service-owned collector for decoded control input.
pub struct ControlInputCollector {
    hid_port: Arc<dyn HidPort>,
    handles: Arc<Mutex<HashMap<DeviceId, SharedHidDevice>>>,
    output_sender: mpsc::Sender<ControlStreamItem>,
    output_receiver: Mutex<Option<mpsc::Receiver<ControlStreamItem>>>,
    task: Mutex<Option<JoinHandle<()>>>,
    stop_sender: Mutex<Option<watch::Sender<bool>>>,
    poll_interval: Duration,
    next_instance: Arc<AtomicU64>,
    dropped_items: Arc<AtomicU64>,
}

impl ControlInputCollector {
    /// Create a collector backed by the existing HID port.
    #[must_use]
    pub fn new(hid_port: Arc<dyn HidPort>) -> Self {
        Self::with_options(
            hid_port,
            DEFAULT_CONTROL_INPUT_CAPACITY,
            DEFAULT_CONTROL_INPUT_POLL_INTERVAL,
        )
    }

    /// Create a collector with deterministic test/runtime options.
    #[must_use]
    pub fn with_options(
        hid_port: Arc<dyn HidPort>,
        capacity: usize,
        poll_interval: Duration,
    ) -> Self {
        let (output_sender, output_receiver) = mpsc::channel(capacity.max(1));
        Self {
            hid_port,
            handles: Arc::new(Mutex::new(HashMap::new())),
            output_sender,
            output_receiver: Mutex::new(Some(output_receiver)),
            task: Mutex::new(None),
            stop_sender: Mutex::new(None),
            poll_interval: poll_interval.max(Duration::from_millis(1)),
            next_instance: Arc::new(AtomicU64::new(1)),
            dropped_items: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Take the bounded producer stream before starting the collector.
    pub async fn take_stream(&self) -> Result<mpsc::Receiver<ControlStreamItem>> {
        self.output_receiver
            .lock()
            .await
            .take()
            .context("control input stream receiver already taken")
    }

    /// Start collection. Starting an already running collector is harmless.
    pub async fn start(&self) -> Result<()> {
        let mut task_guard = self.task.lock().await;
        if task_guard.is_some() {
            return Ok(());
        }

        let monitor = self
            .hid_port
            .monitor_devices()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .context("failed to start device monitor for control input")?;
        let (stop_sender, stop_receiver) = watch::channel(false);
        *self.stop_sender.lock().await = Some(stop_sender);

        let hid_port = Arc::clone(&self.hid_port);
        let handles = Arc::clone(&self.handles);
        let output_sender = self.output_sender.clone();
        let poll_interval = self.poll_interval;
        let next_instance = Arc::clone(&self.next_instance);
        let dropped_items = Arc::clone(&self.dropped_items);

        let task = tokio::spawn(run_collector(
            hid_port,
            handles,
            output_sender,
            monitor,
            stop_receiver,
            poll_interval,
            next_instance,
            dropped_items,
        ));
        *task_guard = Some(task);
        Ok(())
    }

    /// Stop collection and release all collector-owned device sessions.
    pub async fn stop(&self) -> Result<()> {
        let task = {
            let mut task_guard = self.task.lock().await;
            if let Some(sender) = self.stop_sender.lock().await.take() {
                let _ = sender.send(true);
            }
            task_guard.take()
        };

        if let Some(task) = task {
            task.await.context("control input collector task failed")?;
        }

        self.handles.lock().await.clear();
        Ok(())
    }

    /// Open or return the one shared session for a device.
    pub(crate) async fn open_or_get_device(&self, id: &DeviceId) -> Result<SharedHidDevice> {
        open_shared_device(&self.hid_port, &self.handles, id).await
    }

    /// Number of items rejected because the bounded handoff was full.
    #[must_use]
    pub fn dropped_items(&self) -> u64 {
        self.dropped_items.load(Ordering::Relaxed)
    }
}

struct Session {
    info: DeviceInfo,
    handle: SharedHidDevice,
    projector: ControlProjector,
    last_tick: Option<u32>,
}

async fn run_collector(
    hid_port: Arc<dyn HidPort>,
    handles: Arc<Mutex<HashMap<DeviceId, SharedHidDevice>>>,
    output_sender: mpsc::Sender<ControlStreamItem>,
    mut monitor: mpsc::Receiver<DeviceEvent>,
    mut stop_receiver: watch::Receiver<bool>,
    poll_interval: Duration,
    next_instance: Arc<AtomicU64>,
    dropped_items: Arc<AtomicU64>,
) {
    let mut sessions = HashMap::new();

    let devices = hid_port
        .list_devices()
        .await
        .map_err(|error| error.to_string());
    match devices {
        Ok(devices) => {
            for info in devices.into_iter().filter(|info| info.is_connected) {
                register_session(
                    &hid_port,
                    &handles,
                    &output_sender,
                    &mut sessions,
                    info,
                    &next_instance,
                    &dropped_items,
                )
                .await;
            }
        }
        Err(error) => warn!(error = %error, "initial control input enumeration failed"),
    }

    let mut ticker = time::interval(poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            changed = stop_receiver.changed() => {
                if changed.is_err() || *stop_receiver.borrow() {
                    break;
                }
            }
            event = monitor.recv() => {
                match event {
                    Some(DeviceEvent::Connected(info)) => register_session(
                        &hid_port,
                        &handles,
                        &output_sender,
                        &mut sessions,
                        info,
                        &next_instance,
                        &dropped_items,
                    ).await,
                    Some(DeviceEvent::Disconnected(info)) => {
                        disconnect_session(
                            &handles,
                            &output_sender,
                            &mut sessions,
                            &info.id,
                            &dropped_items,
                        ).await;
                    }
                    None => break,
                }
            }
            _ = ticker.tick() => poll_sessions(&mut sessions, &output_sender, &dropped_items).await,
        }
    }

    for (_, mut session) in sessions {
        let item = session
            .projector
            .reset(ResetReason::Disconnect, timestamp_ns());
        send_item(&output_sender, item, &dropped_items);
    }
    handles.lock().await.clear();
}

async fn register_session(
    hid_port: &Arc<dyn HidPort>,
    handles: &Arc<Mutex<HashMap<DeviceId, SharedHidDevice>>>,
    output_sender: &mpsc::Sender<ControlStreamItem>,
    sessions: &mut HashMap<DeviceId, Session>,
    info: DeviceInfo,
    next_instance: &AtomicU64,
    dropped_items: &AtomicU64,
) {
    if sessions.contains_key(&info.id) || !info.is_connected {
        return;
    }

    let handle = match open_shared_device(hid_port, handles, &info.id).await {
        Ok(handle) => handle,
        Err(error) => {
            warn!(device_id = %info.id, error = %error, "failed to open control input device");
            return;
        }
    };

    let identity = DeviceIdentity {
        vendor_id: info.vendor_id,
        product_id: info.product_id,
        serial: info.serial_number.clone(),
        instance: next_instance.fetch_add(1, Ordering::Relaxed),
    };
    let mut projector = ControlProjector::new(identity, 1);
    send_item(
        output_sender,
        projector.descriptor(timestamp_ns()),
        dropped_items,
    );
    sessions.insert(
        info.id.clone(),
        Session {
            info,
            handle,
            projector,
            last_tick: None,
        },
    );
}

async fn disconnect_session(
    handles: &Arc<Mutex<HashMap<DeviceId, SharedHidDevice>>>,
    output_sender: &mpsc::Sender<ControlStreamItem>,
    sessions: &mut HashMap<DeviceId, Session>,
    id: &DeviceId,
    dropped_items: &AtomicU64,
) {
    if let Some(mut session) = sessions.remove(id) {
        debug!(device_id = %session.info.id, "control input device disconnected");
        send_item(
            output_sender,
            session
                .projector
                .reset(ResetReason::Disconnect, timestamp_ns()),
            dropped_items,
        );
    }
    handles.lock().await.remove(id);
}

async fn open_shared_device(
    hid_port: &Arc<dyn HidPort>,
    handles: &Arc<Mutex<HashMap<DeviceId, SharedHidDevice>>>,
    id: &DeviceId,
) -> Result<SharedHidDevice> {
    let mut handles = handles.lock().await;
    if let Some(handle) = handles.get(id).cloned() {
        return Ok(handle);
    }

    let opened = hid_port
        .open_device(id)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .with_context(|| format!("failed to open input device {id}"))?;
    let handle = Arc::new(Mutex::new(opened));
    handles.insert(id.clone(), Arc::clone(&handle));
    Ok(handle)
}

async fn poll_sessions(
    sessions: &mut HashMap<DeviceId, Session>,
    output_sender: &mpsc::Sender<ControlStreamItem>,
    dropped_items: &AtomicU64,
) {
    for session in sessions.values_mut() {
        let inputs = session.handle.lock().await.read_inputs();
        let Some(inputs) = inputs else {
            continue;
        };

        if let Some(last_tick) = session.last_tick {
            if inputs.tick == last_tick {
                continue;
            }
            if inputs.tick < last_tick {
                send_item(
                    output_sender,
                    session
                        .projector
                        .reset(ResetReason::EpochChange, timestamp_ns()),
                    dropped_items,
                );
                session.last_tick = None;
            }
        }

        session.last_tick = Some(inputs.tick);
        for item in session.projector.observe(&inputs, timestamp_ns()) {
            send_item(output_sender, item, dropped_items);
        }
    }
}

fn send_item(
    output_sender: &mpsc::Sender<ControlStreamItem>,
    item: ControlStreamItem,
    dropped_items: &AtomicU64,
) {
    if output_sender.try_send(item).is_err() {
        dropped_items.fetch_add(1, Ordering::Relaxed);
    }
}

fn timestamp_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_nanos();
    u64::try_from(elapsed).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openracing_device_types::DeviceInputs;
    use racing_wheel_engine::{VirtualDevice, VirtualHidPort};
    use tokio::time::{Duration, timeout};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    async fn next_item(
        stream: &mut mpsc::Receiver<ControlStreamItem>,
    ) -> Result<ControlStreamItem, Box<dyn std::error::Error>> {
        timeout(Duration::from_secs(1), stream.recv())
            .await?
            .ok_or_else(|| "control input stream closed".into())
    }

    fn seeded_port() -> Result<(Arc<VirtualHidPort>, DeviceId), Box<dyn std::error::Error>> {
        let port = VirtualHidPort::new();
        let id = "collector-device".parse::<DeviceId>()?;
        port.add_device(VirtualDevice::new(
            id.clone(),
            "Collector Wheel".to_string(),
        ))?;
        Ok((Arc::new(port), id))
    }

    #[tokio::test]
    async fn collects_descriptor_baseline_and_button_edge() -> TestResult {
        let (port, id) = seeded_port()?;
        let collector =
            ControlInputCollector::with_options(port.clone(), 32, Duration::from_millis(1));
        let mut stream = collector.take_stream().await?;
        collector.start().await?;

        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::Descriptor { .. }
        ));
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::InitialSnapshot { .. }
        ));

        let mut inputs = DeviceInputs {
            tick: 1,
            ..DeviceInputs::default()
        };
        inputs.set_button(7, true);
        port.set_device_inputs(&id, inputs)?;

        let item = next_item(&mut stream).await?;
        assert!(matches!(item, ControlStreamItem::Event { .. }));
        assert!(item.is_actionable());

        collector.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn disconnect_emits_reset_and_stop_is_deterministic() -> TestResult {
        let (port, id) = seeded_port()?;
        let collector =
            ControlInputCollector::with_options(port.clone(), 32, Duration::from_millis(1));
        let mut stream = collector.take_stream().await?;
        collector.start().await?;
        let _ = next_item(&mut stream).await?;
        let _ = next_item(&mut stream).await?;

        port.remove_device(&id)?;
        let item = next_item(&mut stream).await?;
        assert!(matches!(
            item,
            ControlStreamItem::Reset {
                reason: ResetReason::Disconnect,
                ..
            }
        ));

        collector.stop().await?;
        collector.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn reconnect_reannounces_surface_and_epoch_reset_rebaselines() -> TestResult {
        let (port, id) = seeded_port()?;
        let collector =
            ControlInputCollector::with_options(port.clone(), 32, Duration::from_millis(1));
        let mut stream = collector.take_stream().await?;
        collector.start().await?;

        let _ = next_item(&mut stream).await?;
        let _ = next_item(&mut stream).await?;
        port.remove_device(&id)?;
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::Reset {
                reason: ResetReason::Disconnect,
                ..
            }
        ));

        port.add_device(VirtualDevice::new(
            id.clone(),
            "Collector Wheel".to_string(),
        ))?;
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::Descriptor { .. }
        ));
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::InitialSnapshot { .. }
        ));

        let mut inputs = DeviceInputs {
            tick: 10,
            ..DeviceInputs::default()
        };
        inputs.set_button(2, true);
        port.set_device_inputs(&id, inputs)?;
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::Event { .. }
        ));

        port.set_device_inputs(
            &id,
            DeviceInputs {
                tick: 1,
                ..DeviceInputs::default()
            },
        )?;
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::Reset {
                reason: ResetReason::EpochChange,
                ..
            }
        ));
        assert!(matches!(
            next_item(&mut stream).await?,
            ControlStreamItem::InitialSnapshot { .. }
        ));

        collector.stop().await?;
        Ok(())
    }
}
