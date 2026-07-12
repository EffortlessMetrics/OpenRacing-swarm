//! Device abstraction and virtual device implementation

use crate::RTResult;
use crate::prelude::MutexExt;
pub use openracing_errors::RTError;
use racing_wheel_schemas::prelude::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Telemetry data from device
#[derive(Debug, Clone)]
pub struct TelemetryData {
    /// Current wheel angle in degrees (positive = clockwise)
    pub wheel_angle_deg: f32,
    /// Current wheel rotational speed in radians per second
    pub wheel_speed_rad_s: f32,
    /// Device temperature in degrees Celsius
    pub temperature_c: u8,
    /// Bitmask of active device faults (see [`crate::protocol::fault_flags`])
    pub fault_flags: u8,
    /// Whether hands-on-wheel detection is active
    pub hands_on: bool,
    /// Timestamp when this telemetry snapshot was captured
    pub timestamp: Instant,
}

/// Generic non-RT control-surface snapshot used by input pipeline and diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceInputs {
    /// Monotonic tick counter from the device firmware
    pub tick: u32,
    /// Raw button state bytes (up to 128 buttons, 1 bit each)
    pub buttons: [u8; 16],
    /// Hat switch / D-pad position (0–8; 0 = center)
    pub hat: u8,
    /// Absolute steering axis value (0–65535)
    pub steering: Option<u16>,
    /// Absolute throttle axis value (0–65535)
    pub throttle: Option<u16>,
    /// Absolute brake axis value (0–65535)
    pub brake: Option<u16>,
    /// Left clutch paddle axis value (0–65535)
    pub clutch_left: Option<u16>,
    /// Right clutch paddle axis value (0–65535)
    pub clutch_right: Option<u16>,
    /// Combined clutch axis value (0–65535)
    pub clutch_combined: Option<u16>,
    /// Left clutch paddle digital button state
    pub clutch_left_button: Option<bool>,
    /// Right clutch paddle digital button state
    pub clutch_right_button: Option<bool>,
    /// Handbrake axis value (0–65535)
    pub handbrake: Option<u16>,
    /// Rotary encoder deltas (up to 8 encoders)
    pub rotaries: [i16; 8],
}

/// Device info for enumeration and management
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Unique device identifier
    pub id: DeviceId,
    /// Human-readable device name (e.g. "Moza R9 V2")
    pub name: String,
    /// USB vendor ID
    pub vendor_id: u16,
    /// USB product ID
    pub product_id: u16,
    /// Device serial number (if reported by firmware)
    pub serial_number: Option<String>,
    /// Manufacturer string (if reported by firmware)
    pub manufacturer: Option<String>,
    /// OS-level device path used to open the HID handle
    pub path: String,
    /// Negotiated device capabilities
    pub capabilities: DeviceCapabilities,
    /// Whether the device is currently connected
    pub is_connected: bool,
}

// HidDevice and HidPort traits are now defined in ports.rs
use crate::ports::{DeviceHealthStatus, HidDevice, HidPort};

/// Device events for monitoring
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// A new device was detected and enumerated
    Connected(DeviceInfo),
    /// A previously known device was physically disconnected
    Disconnected(DeviceInfo),
}

/// Virtual device implementation for testing
pub struct VirtualDevice {
    info: DeviceInfo,
    capabilities: DeviceCapabilities,
    state: Arc<Mutex<VirtualDeviceState>>,
    connected: Arc<AtomicBool>,
}

#[derive(Debug)]
struct VirtualDeviceState {
    wheel_angle_deg: f32,
    wheel_speed_rad_s: f32,
    temperature_c: u8,
    fault_flags: u8,
    hands_on: bool,
    last_torque_nm: f32,
    last_seq: u16,
    last_update: Instant,
    /// Fixed-size ring buffer for torque history to avoid RT allocations
    torque_history: [f32; 1024],
    torque_history_idx: usize,
    torque_history_len: usize,
}

impl VirtualDevice {
    /// Create a new virtual device
    #[allow(clippy::expect_used)]
    pub fn new(id: DeviceId, name: String) -> Self {
        let capabilities = DeviceCapabilities::new(
            false, // supports_pid
            true,  // supports_raw_torque_1khz
            true,  // supports_health_stream
            true,  // supports_led_bus
            // SAFETY: 25.0 is within the valid range for TorqueNm
            unsafe { TorqueNm::new_unchecked(25.0) },
            10000, // encoder_cpr
            1000,  // min_report_period_us (1ms = 1kHz)
        );

        let info = DeviceInfo {
            id: id.clone(),
            name,
            vendor_id: 0x1234,  // Mock vendor ID
            product_id: 0x5678, // Mock product ID
            serial_number: Some("VIRTUAL001".to_string()),
            manufacturer: Some("Virtual Racing".to_string()),
            path: format!("virtual://{}", id.as_str()),
            capabilities: capabilities.clone(),
            is_connected: true,
        };

        let state = VirtualDeviceState {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 35,
            fault_flags: 0,
            hands_on: true,
            last_torque_nm: 0.0,
            last_seq: 0,
            last_update: Instant::now(),
            torque_history: [0.0; 1024],
            torque_history_idx: 0,
            torque_history_len: 0,
        };

        Self {
            info,
            capabilities,
            state: Arc::new(Mutex::new(state)),
            connected: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Simulate device physics (for testing)
    pub fn simulate_physics(&mut self, dt: Duration) {
        let mut state = self.state.lock_or_panic();

        // Simple physics simulation
        let dt_s = dt.as_secs_f32();

        // Apply torque to wheel dynamics
        let inertia = 0.1; // kg*m^2
        let friction = 0.05;
        let damping = 0.02;

        let torque_total = state.last_torque_nm
            - friction * state.wheel_speed_rad_s.signum()
            - damping * state.wheel_speed_rad_s;

        let acceleration = torque_total / inertia;
        state.wheel_speed_rad_s += acceleration * dt_s;
        state.wheel_angle_deg += state.wheel_speed_rad_s.to_degrees() * dt_s;

        // Keep angle in reasonable range
        if state.wheel_angle_deg > 1080.0 {
            state.wheel_angle_deg = 1080.0;
            state.wheel_speed_rad_s = 0.0;
        } else if state.wheel_angle_deg < -1080.0 {
            state.wheel_angle_deg = -1080.0;
            state.wheel_speed_rad_s = 0.0;
        }

        // Simulate temperature based on torque
        let torque_heating = state.last_torque_nm.abs() * 0.1;
        let ambient_cooling = (state.temperature_c as f32 - 25.0) * 0.01;
        let temp_change = (torque_heating - ambient_cooling) * dt_s;
        state.temperature_c = ((state.temperature_c as f32 + temp_change).clamp(20.0, 100.0)) as u8;

        // Hands-on detection based on torque variance in the ring buffer
        if state.torque_history_len > 10 {
            let mut sum_diff = 0.0;
            for i in 0..state.torque_history_len.min(100) {
                let curr_idx = (state.torque_history_idx + 1024 - i) % 1024;
                let prev_idx = (state.torque_history_idx + 1024 - i - 1) % 1024;
                sum_diff += (state.torque_history[curr_idx] - state.torque_history[prev_idx]).abs();
            }
            let torque_variance = sum_diff / state.torque_history_len.min(100) as f32;
            state.hands_on = torque_variance > 0.1;
        }

        state.last_update = Instant::now();
    }

    /// Inject a fault for testing
    pub fn inject_fault(&mut self, fault_type: u8) {
        let mut state = self.state.lock_or_panic();
        state.fault_flags |= fault_type;
    }

    /// Clear faults
    pub fn clear_faults(&mut self) {
        let mut state = self.state.lock_or_panic();
        state.fault_flags = 0;
    }

    /// Disconnect the device (for testing)
    pub fn disconnect(&mut self) {
        self.connected.store(false, Ordering::Release);
    }

    /// Reconnect the device (for testing)
    pub fn reconnect(&mut self) {
        self.connected.store(true, Ordering::Release);
    }
}

impl HidDevice for VirtualDevice {
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult {
        if !self.connected.load(Ordering::Acquire) {
            return Err(RTError::DeviceDisconnected);
        }

        // Reject non-finite values before torque limit check
        if !torque_nm.is_finite() {
            return Err(RTError::InvalidConfig);
        }

        let mut state = self.state.lock().map_err(|_| RTError::PipelineFault)?;

        // Validate torque is within device limits
        let max_torque = self.capabilities.max_torque.value();
        if torque_nm.abs() > max_torque {
            return Err(RTError::TorqueLimit);
        }

        state.last_torque_nm = torque_nm;
        state.last_seq = seq;

        // Push to ring buffer (no allocations)
        let idx = (state.torque_history_idx + 1) % 1024;
        state.torque_history_idx = idx;
        state.torque_history[idx] = torque_nm;
        if state.torque_history_len < 1024 {
            state.torque_history_len += 1;
        }

        Ok(())
    }

    fn read_telemetry(&mut self) -> Option<TelemetryData> {
        if !self.connected.load(Ordering::Acquire) {
            return None;
        }

        let state = self.state.lock().ok()?;

        Some(TelemetryData {
            wheel_angle_deg: state.wheel_angle_deg,
            wheel_speed_rad_s: state.wheel_speed_rad_s,
            temperature_c: state.temperature_c,
            fault_flags: state.fault_flags,
            hands_on: state.hands_on,
            timestamp: Instant::now(),
        })
    }

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }

    fn device_info(&self) -> &DeviceInfo {
        &self.info
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn health_status(&self) -> DeviceHealthStatus {
        let state = self.state.lock_or_panic();
        DeviceHealthStatus {
            temperature_c: state.temperature_c,
            fault_flags: state.fault_flags,
            hands_on: state.hands_on,
            last_communication: state.last_update,
            communication_errors: 0,
        }
    }
}

/// Virtual HID port for testing
pub struct VirtualHidPort {
    devices: Arc<Mutex<Vec<VirtualDevice>>>,
    event_tx: Arc<Mutex<Option<mpsc::Sender<DeviceEvent>>>>,
}

impl VirtualHidPort {
    /// Create a new virtual HID port
    pub fn new() -> Self {
        Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            event_tx: Arc::new(Mutex::new(None)),
        }
    }

    fn emit_event(&self, event: DeviceEvent) {
        if let Ok(event_tx) = self.event_tx.lock()
            && let Some(tx) = event_tx.as_ref()
        {
            let _ = tx.try_send(event);
        }
    }

    /// Add a virtual device to the port
    pub fn add_device(&mut self, device: VirtualDevice) -> Result<(), Box<dyn std::error::Error>> {
        let mut device = device;
        device.info.is_connected = true;
        device.connected.store(true, Ordering::Release);
        let device_info = device.device_info().clone();

        {
            let mut devices = self.devices.lock_or_panic();
            devices.push(device);
        }

        // Send connect event if monitoring
        self.emit_event(DeviceEvent::Connected(device_info));

        Ok(())
    }

    /// Remove a device by ID
    pub fn remove_device(&mut self, id: &DeviceId) -> Result<(), Box<dyn std::error::Error>> {
        let mut devices = self.devices.lock_or_panic();
        let device_info = devices.iter_mut().find(|d| d.info.id == *id).map(|d| {
            d.connected.store(false, Ordering::Release);
            let mut info = d.info.clone();
            info.is_connected = false;
            info
        });

        devices.retain(|d| d.info.id != *id);

        // Send disconnect event if monitoring
        if let Some(info) = device_info {
            self.emit_event(DeviceEvent::Disconnected(info));
        }

        Ok(())
    }

    /// Get mutable reference to device for testing
    pub fn get_device_mut(&mut self, _id: &DeviceId) -> Option<&mut VirtualDevice> {
        // This is a bit tricky with Arc<Mutex<Vec<_>>>
        // For testing purposes, we'll provide a different approach
        // The caller should use the device reference returned from open_device
        None
    }

    /// Simulate physics for all devices
    pub fn simulate_physics(&mut self, dt: Duration) {
        let mut devices = self.devices.lock_or_panic();
        for device in devices.iter_mut() {
            device.simulate_physics(dt);
        }
    }
}

#[async_trait::async_trait]
impl HidPort for VirtualHidPort {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
        let devices = self.devices.lock_or_panic();
        Ok(devices
            .iter()
            .map(|d| {
                let mut info = d.device_info().clone();
                info.is_connected = d.is_connected();
                info
            })
            .collect())
    }

    async fn open_device(
        &self,
        id: &DeviceId,
    ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.lock_or_panic();

        for device in devices.iter() {
            if device.info.id == *id {
                // Create a new instance that shares the same state
                let virtual_device = VirtualDevice {
                    info: {
                        let mut info = device.info.clone();
                        info.is_connected = device.is_connected();
                        info
                    },
                    capabilities: device.capabilities.clone(),
                    state: Arc::clone(&device.state),
                    connected: Arc::clone(&device.connected),
                };

                return Ok(Box::new(virtual_device));
            }
        }

        Err(format!("Device not found: {}", id).into())
    }

    async fn monitor_devices(
        &self,
    ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(100);
        let mut event_tx = self
            .event_tx
            .lock()
            .map_err(|_| std::io::Error::other("virtual device event monitor lock poisoned"))?;
        *event_tx = Some(tx);
        Ok(rx)
    }

    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
        // For virtual devices, this is a no-op
        Ok(())
    }
}

impl Default for VirtualHidPort {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn test_virtual_device_creation() -> Result<()> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let device = VirtualDevice::new(device_id, "Test Wheel".to_string());

        assert_eq!(device.device_info().id.as_str(), "test-device");
        assert_eq!(device.device_info().name, "Test Wheel");
        assert!(device.is_connected());
        assert_eq!(device.capabilities().max_torque.value(), 25.0);
        Ok(())
    }

    #[test]
    fn test_virtual_device_torque_write() -> Result<()> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());

        // Test normal torque write
        let result = device.write_ffb_report(10.0, 1);
        assert!(result.is_ok());

        // Test torque limit
        let result = device.write_ffb_report(30.0, 2); // Exceeds 25Nm limit
        assert_eq!(result, Err(RTError::TorqueLimit));

        // Test disconnected device
        device.disconnect();
        let result = device.write_ffb_report(5.0, 3);
        assert_eq!(result, Err(RTError::DeviceDisconnected));
        Ok(())
    }

    #[test]
    fn test_virtual_device_telemetry() -> Result<()> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());

        // Write some torque
        device.write_ffb_report(5.0, 1)?;

        // Simulate physics
        device.simulate_physics(Duration::from_millis(10));

        // Read telemetry
        let telemetry = device.read_telemetry().ok_or("telemetry missing")?;
        assert!(telemetry.temperature_c >= 35);
        Ok(())
    }

    #[tokio::test]
    async fn test_virtual_hid_port() -> Result<()> {
        let mut port = VirtualHidPort::new();

        // Add a device
        let device_id = "test-device".parse::<DeviceId>()?;
        let device = VirtualDevice::new(device_id.clone(), "Test Wheel".to_string());
        port.add_device(device)?;

        // List devices
        let devices = port.list_devices().await?;
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id.as_str(), "test-device");

        // Open device
        let mut opened_device = port.open_device(&device_id).await?;
        assert!(opened_device.is_connected());

        // Test device operations
        let result = opened_device.write_ffb_report(5.0, 1);
        assert!(result.is_ok());

        let telemetry = opened_device.read_telemetry();
        assert!(telemetry.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_virtual_hid_port_reports_reenumeration_and_invalidates_open_handle() -> Result<()>
    {
        let mut port = VirtualHidPort::new();
        let mut events = port.monitor_devices().await?;
        let device_id = "reconnect-device".parse::<DeviceId>()?;

        port.add_device(VirtualDevice::new(
            device_id.clone(),
            "Reconnect Wheel".to_string(),
        ))?;

        let connected = events.recv().await.ok_or("missing connect event")?;
        match connected {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.id, device_id);
                assert!(info.is_connected);
            }
            DeviceEvent::Disconnected(_) => return Err("unexpected disconnect event".into()),
        }

        let mut stale_handle = port.open_device(&device_id).await?;
        assert!(stale_handle.is_connected());

        port.remove_device(&device_id)?;

        let disconnected = events.recv().await.ok_or("missing disconnect event")?;
        match disconnected {
            DeviceEvent::Disconnected(info) => {
                assert_eq!(info.id, device_id);
                assert!(!info.is_connected);
            }
            DeviceEvent::Connected(_) => return Err("unexpected connect event".into()),
        }
        assert!(!stale_handle.is_connected());
        assert_eq!(
            stale_handle.write_ffb_report(2.0, 1),
            Err(RTError::DeviceDisconnected)
        );
        assert!(stale_handle.read_telemetry().is_none());

        port.add_device(VirtualDevice::new(
            device_id.clone(),
            "Reconnect Wheel".to_string(),
        ))?;
        let reconnected = events.recv().await.ok_or("missing re-connect event")?;
        match reconnected {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.id, device_id);
                assert!(info.is_connected);
            }
            DeviceEvent::Disconnected(_) => return Err("unexpected second disconnect".into()),
        }

        let mut fresh_handle = port.open_device(&device_id).await?;
        assert!(fresh_handle.is_connected());
        fresh_handle.write_ffb_report(2.0, 2)?;
        assert!(fresh_handle.read_telemetry().is_some());
        Ok(())
    }

    #[test]
    fn test_virtual_device_connection_state_is_shared_with_open_handle() -> Result<()> {
        let device_id = "shared-connection-device".parse::<DeviceId>()?;
        let mut original = VirtualDevice::new(device_id, "Shared Wheel".to_string());
        let mut opened = VirtualDevice {
            info: original.info.clone(),
            capabilities: original.capabilities.clone(),
            state: Arc::clone(&original.state),
            connected: Arc::clone(&original.connected),
        };

        original.disconnect();
        assert!(!opened.is_connected());
        assert_eq!(
            opened.write_ffb_report(1.0, 1),
            Err(RTError::DeviceDisconnected)
        );

        original.reconnect();
        assert!(opened.is_connected());
        opened.write_ffb_report(1.0, 2)?;
        Ok(())
    }

    #[test]
    fn test_virtual_device_physics_simulation() -> Result<()> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());

        // Apply constant torque
        device.write_ffb_report(10.0, 1)?;

        // Simulate for 100ms
        for _ in 0..10 {
            device.simulate_physics(Duration::from_millis(10));
        }

        let telemetry = device.read_telemetry().ok_or("telemetry missing")?;

        // Wheel should have moved and gained speed
        assert!(telemetry.wheel_angle_deg.abs() > 0.0);
        assert!(telemetry.wheel_speed_rad_s.abs() > 0.0);

        // Temperature should have increased slightly (or at least stayed at baseline)
        assert!(telemetry.temperature_c >= 35);
        Ok(())
    }

    #[test]
    fn test_fault_injection() -> Result<()> {
        let device_id = "test-device".parse::<DeviceId>()?;
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());

        // Initially no faults
        let telemetry = device.read_telemetry().ok_or("telemetry missing")?;
        assert_eq!(telemetry.fault_flags, 0);

        // Inject thermal fault
        device.inject_fault(0x04); // Thermal fault bit

        let telemetry = device.read_telemetry().ok_or("telemetry missing")?;
        assert_eq!(telemetry.fault_flags, 0x04);

        // Clear faults
        device.clear_faults();

        let telemetry = device.read_telemetry().ok_or("telemetry missing")?;
        assert_eq!(telemetry.fault_flags, 0);
        Ok(())
    }
}
