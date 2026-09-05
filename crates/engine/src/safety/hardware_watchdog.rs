//! Hardware watchdog integration for safety-critical torque control
//!
//! This module provides hardware watchdog integration with 100ms timeout
//! for safety-critical force feedback systems. The watchdog ensures that
//! if the RT loop stops feeding the watchdog, torque is immediately zeroed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::FaultType;

/// Hardware watchdog trait for safety-critical torque control
///
/// Implementations must ensure that if `feed()` is not called within
/// the timeout period, the device enters a safe state (zero torque).
pub trait HardwareWatchdog: Send + Sync {
    /// Feed the watchdog (must be called within timeout period)
    ///
    /// This method should be called from the RT loop on every tick
    /// to prevent watchdog timeout.
    fn feed(&mut self) -> Result<(), WatchdogError>;

    /// Get watchdog timeout in milliseconds
    fn timeout_ms(&self) -> u32;

    /// Check if watchdog is armed (active and monitoring)
    fn is_armed(&self) -> bool;

    /// Arm the watchdog (start monitoring)
    fn arm(&mut self) -> Result<(), WatchdogError>;

    /// Disarm the watchdog (stop monitoring)
    fn disarm(&mut self) -> Result<(), WatchdogError>;

    /// Trigger immediate safe state (zero torque)
    fn trigger_safe_state(&mut self) -> Result<(), WatchdogError>;

    /// Check if watchdog has timed out
    fn has_timed_out(&self) -> bool;

    /// Get time since last feed
    fn time_since_last_feed(&self) -> Duration;

    /// Reset the watchdog state
    fn reset(&mut self) -> Result<(), WatchdogError>;
}

/// Watchdog error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogError {
    /// Watchdog is not armed
    NotArmed,
    /// Watchdog is already armed
    AlreadyArmed,
    /// Watchdog has timed out
    TimedOut,
    /// Hardware communication error
    HardwareError(String),
    /// Invalid configuration
    InvalidConfiguration(String),
}

impl std::fmt::Display for WatchdogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchdogError::NotArmed => write!(f, "Watchdog is not armed"),
            WatchdogError::AlreadyArmed => write!(f, "Watchdog is already armed"),
            WatchdogError::TimedOut => write!(f, "Watchdog has timed out"),
            WatchdogError::HardwareError(msg) => write!(f, "Hardware error: {}", msg),
            WatchdogError::InvalidConfiguration(msg) => {
                write!(f, "Invalid configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for WatchdogError {}

/// Software-based hardware watchdog implementation
///
/// This implementation provides a software watchdog that can be used
/// when hardware watchdog is not available, or for testing purposes.
/// For production use with real hardware, a hardware-specific implementation
/// should be used.
pub struct SoftwareWatchdog {
    timeout: Duration,
    last_feed: AtomicU64,
    armed: AtomicBool,
    timed_out: AtomicBool,
    safe_state_triggered: AtomicBool,
    start_time: Instant,
}

impl SoftwareWatchdog {
    /// Create a new software watchdog with the specified timeout
    ///
    /// Default timeout is 100ms as per safety requirements.
    pub fn new(timeout_ms: u32) -> Self {
        let start_time = Instant::now();
        Self {
            timeout: Duration::from_millis(timeout_ms as u64),
            last_feed: AtomicU64::new(0),
            armed: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
            safe_state_triggered: AtomicBool::new(false),
            start_time,
        }
    }

    /// Create a new software watchdog with default 100ms timeout
    pub fn with_default_timeout() -> Self {
        Self::new(100)
    }

    /// Get elapsed time since start as u64 microseconds
    fn elapsed_micros(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Check timeout and update state
    pub fn check_timeout(&self) -> bool {
        if !self.armed.load(Ordering::Acquire) {
            return false;
        }

        let last_feed_micros = self.last_feed.load(Ordering::Acquire);
        let current_micros = self.elapsed_micros();
        let elapsed = Duration::from_micros(current_micros.saturating_sub(last_feed_micros));

        if elapsed > self.timeout {
            self.timed_out.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Check if safe state was triggered
    pub fn is_safe_state_triggered(&self) -> bool {
        self.safe_state_triggered.load(Ordering::Acquire)
    }
}

impl HardwareWatchdog for SoftwareWatchdog {
    fn feed(&mut self) -> Result<(), WatchdogError> {
        if !self.armed.load(Ordering::Acquire) {
            return Err(WatchdogError::NotArmed);
        }

        if self.timed_out.load(Ordering::Acquire) {
            return Err(WatchdogError::TimedOut);
        }

        self.last_feed
            .store(self.elapsed_micros(), Ordering::Release);
        Ok(())
    }

    fn timeout_ms(&self) -> u32 {
        self.timeout.as_millis() as u32
    }

    fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }

    fn arm(&mut self) -> Result<(), WatchdogError> {
        if self.armed.load(Ordering::Acquire) {
            return Err(WatchdogError::AlreadyArmed);
        }

        // Reset state before arming
        self.timed_out.store(false, Ordering::Release);
        self.safe_state_triggered.store(false, Ordering::Release);
        self.last_feed
            .store(self.elapsed_micros(), Ordering::Release);
        self.armed.store(true, Ordering::Release);
        Ok(())
    }

    fn disarm(&mut self) -> Result<(), WatchdogError> {
        if !self.armed.load(Ordering::Acquire) {
            return Err(WatchdogError::NotArmed);
        }

        self.armed.store(false, Ordering::Release);
        Ok(())
    }

    fn trigger_safe_state(&mut self) -> Result<(), WatchdogError> {
        self.safe_state_triggered.store(true, Ordering::Release);
        self.timed_out.store(true, Ordering::Release);
        Ok(())
    }

    fn has_timed_out(&self) -> bool {
        self.check_timeout();
        self.timed_out.load(Ordering::Acquire)
    }

    fn time_since_last_feed(&self) -> Duration {
        let last_feed_micros = self.last_feed.load(Ordering::Acquire);
        let current_micros = self.elapsed_micros();
        Duration::from_micros(current_micros.saturating_sub(last_feed_micros))
    }

    fn reset(&mut self) -> Result<(), WatchdogError> {
        self.armed.store(false, Ordering::Release);
        self.timed_out.store(false, Ordering::Release);
        self.safe_state_triggered.store(false, Ordering::Release);
        self.last_feed.store(0, Ordering::Release);
        Ok(())
    }
}

impl Default for SoftwareWatchdog {
    fn default() -> Self {
        Self::with_default_timeout()
    }
}

/// Thread-safe wrapper for hardware watchdog
pub struct SharedWatchdog {
    inner: Arc<parking_lot::Mutex<Box<dyn HardwareWatchdog>>>,
}

impl SharedWatchdog {
    /// Create a new shared watchdog
    pub fn new(watchdog: Box<dyn HardwareWatchdog>) -> Self {
        Self {
            inner: Arc::new(parking_lot::Mutex::new(watchdog)),
        }
    }

    /// Feed the watchdog
    pub fn feed(&self) -> Result<(), WatchdogError> {
        self.inner.lock().feed()
    }

    /// Check if watchdog has timed out
    pub fn has_timed_out(&self) -> bool {
        self.inner.lock().has_timed_out()
    }

    /// Arm the watchdog
    pub fn arm(&self) -> Result<(), WatchdogError> {
        self.inner.lock().arm()
    }

    /// Disarm the watchdog
    pub fn disarm(&self) -> Result<(), WatchdogError> {
        self.inner.lock().disarm()
    }

    /// Trigger safe state
    pub fn trigger_safe_state(&self) -> Result<(), WatchdogError> {
        self.inner.lock().trigger_safe_state()
    }

    /// Get timeout in milliseconds
    pub fn timeout_ms(&self) -> u32 {
        self.inner.lock().timeout_ms()
    }

    /// Check if armed
    pub fn is_armed(&self) -> bool {
        self.inner.lock().is_armed()
    }
}

impl Clone for SharedWatchdog {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Watchdog timeout response handler
///
/// This struct handles the response to watchdog timeout events,
/// ensuring zero torque is commanded and safe mode is entered.
pub struct WatchdogTimeoutHandler {
    /// Current torque output (will be zeroed on timeout)
    current_torque: f32,
    /// Whether timeout response has been triggered
    timeout_triggered: bool,
    /// Timestamp of timeout trigger
    timeout_timestamp: Option<Instant>,
    /// Maximum response time (should be < 1ms per requirements)
    max_response_time: Duration,
}

impl WatchdogTimeoutHandler {
    /// Create a new timeout handler
    pub fn new() -> Self {
        Self {
            current_torque: 0.0,
            timeout_triggered: false,
            timeout_timestamp: None,
            max_response_time: Duration::from_micros(1000), // 1ms max
        }
    }

    /// Handle watchdog timeout event
    ///
    /// Returns the torque command (always 0.0 on timeout) and the
    /// safety state transition that should occur.
    pub fn handle_timeout(&mut self, current_torque: f32) -> TimeoutResponse {
        let start = Instant::now();

        // Immediately zero torque
        self.current_torque = 0.0;
        self.timeout_triggered = true;
        self.timeout_timestamp = Some(start);

        let response_time = start.elapsed();

        TimeoutResponse {
            torque_command: 0.0,
            previous_torque: current_torque,
            response_time,
            within_budget: response_time <= self.max_response_time,
            fault_type: FaultType::SafetyInterlockViolation,
        }
    }

    /// Check if timeout has been triggered
    pub fn is_timeout_triggered(&self) -> bool {
        self.timeout_triggered
    }

    /// Get the timestamp of timeout trigger
    pub fn timeout_timestamp(&self) -> Option<Instant> {
        self.timeout_timestamp
    }

    /// Reset the handler state
    pub fn reset(&mut self) {
        self.current_torque = 0.0;
        self.timeout_triggered = false;
        self.timeout_timestamp = None;
    }

    /// Get current torque command
    pub fn current_torque(&self) -> f32 {
        self.current_torque
    }
}

impl Default for WatchdogTimeoutHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from watchdog timeout handling
#[derive(Debug, Clone)]
pub struct TimeoutResponse {
    /// Torque command to send (always 0.0 on timeout)
    pub torque_command: f32,
    /// Previous torque before timeout
    pub previous_torque: f32,
    /// Time taken to respond to timeout
    pub response_time: Duration,
    /// Whether response was within the 1ms budget
    pub within_budget: bool,
    /// Fault type to report
    pub fault_type: FaultType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_software_watchdog_creation() -> Result<(), WatchdogError> {
        let watchdog = SoftwareWatchdog::new(100);
        assert_eq!(watchdog.timeout_ms(), 100);
        assert!(!watchdog.is_armed());
        assert!(!watchdog.has_timed_out());
        Ok(())
    }

    #[test]
    fn test_software_watchdog_arm_disarm() -> Result<(), WatchdogError> {
        let mut watchdog = SoftwareWatchdog::new(100);

        // Should not be armed initially
        assert!(!watchdog.is_armed());

        // Arm the watchdog
        watchdog.arm()?;
        assert!(watchdog.is_armed());

        // Should not be able to arm again
        assert_eq!(watchdog.arm(), Err(WatchdogError::AlreadyArmed));

        // Disarm the watchdog
        watchdog.disarm()?;
        assert!(!watchdog.is_armed());

        // Should not be able to disarm again
        assert_eq!(watchdog.disarm(), Err(WatchdogError::NotArmed));

        Ok(())
    }

    #[test]
    fn test_software_watchdog_feed() -> Result<(), WatchdogError> {
        let mut watchdog = SoftwareWatchdog::new(100);

        // Should not be able to feed when not armed
        assert_eq!(watchdog.feed(), Err(WatchdogError::NotArmed));

        // Arm and feed
        watchdog.arm()?;
        watchdog.feed()?;

        // Time since last feed should be very small
        assert!(watchdog.time_since_last_feed() < Duration::from_millis(10));

        Ok(())
    }

    #[test]
    fn test_software_watchdog_timeout() -> Result<(), WatchdogError> {
        let mut watchdog = SoftwareWatchdog::new(10); // 10ms timeout for faster test

        watchdog.arm()?;
        watchdog.feed()?;

        // Should not be timed out immediately
        assert!(!watchdog.has_timed_out());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(15));

        // Should be timed out now
        assert!(watchdog.has_timed_out());

        // Should not be able to feed after timeout
        assert_eq!(watchdog.feed(), Err(WatchdogError::TimedOut));

        Ok(())
    }

    #[test]
    fn test_software_watchdog_reset() -> Result<(), WatchdogError> {
        let mut watchdog = SoftwareWatchdog::new(10);

        watchdog.arm()?;
        std::thread::sleep(Duration::from_millis(15));
        assert!(watchdog.has_timed_out());

        // Reset should clear timeout state
        watchdog.reset()?;
        assert!(!watchdog.is_armed());
        assert!(!watchdog.has_timed_out());

        Ok(())
    }

    #[test]
    fn test_software_watchdog_trigger_safe_state() -> Result<(), WatchdogError> {
        let mut watchdog = SoftwareWatchdog::new(100);

        watchdog.arm()?;
        watchdog.trigger_safe_state()?;

        assert!(watchdog.is_safe_state_triggered());
        assert!(watchdog.has_timed_out());

        Ok(())
    }

    #[test]
    fn test_timeout_handler() {
        let mut handler = WatchdogTimeoutHandler::new();

        assert!(!handler.is_timeout_triggered());
        assert_eq!(handler.current_torque(), 0.0);

        let response = handler.handle_timeout(10.0);

        assert!(handler.is_timeout_triggered());
        assert_eq!(handler.current_torque(), 0.0);
        assert_eq!(response.torque_command, 0.0);
        assert_eq!(response.previous_torque, 10.0);
        assert!(response.within_budget);
        assert!(handler.timeout_timestamp().is_some());
    }

    #[test]
    fn test_timeout_handler_reset() {
        let mut handler = WatchdogTimeoutHandler::new();

        handler.handle_timeout(10.0);
        assert!(handler.is_timeout_triggered());

        handler.reset();
        assert!(!handler.is_timeout_triggered());
        assert!(handler.timeout_timestamp().is_none());
    }

    #[test]
    fn test_shared_watchdog() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(100));
        let shared = SharedWatchdog::new(watchdog);

        assert!(!shared.is_armed());
        shared.arm()?;
        assert!(shared.is_armed());
        shared.feed()?;
        assert!(!shared.has_timed_out());

        Ok(())
    }

    #[test]
    fn test_default_timeout_is_100ms() {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert_eq!(watchdog.timeout_ms(), 100);
    }
}

/// Safety interlock system that integrates hardware watchdog with torque control
///
/// This system ensures that:
/// 1. Watchdog timeout immediately commands zero torque
/// 2. System transitions to safe mode on timeout
/// 3. Response time is within 1ms budget
pub struct SafetyInterlockSystem {
    watchdog: Box<dyn HardwareWatchdog>,
    timeout_handler: WatchdogTimeoutHandler,
    safety_state: SafetyInterlockState,
    torque_limit: TorqueLimit,
    fault_log: Vec<FaultLogEntry>,
    max_fault_log_entries: usize,
    fault_log_next_index: usize,
    communication_timeout: Duration,
    last_communication: Option<Instant>,
}

/// Safety interlock state machine
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyInterlockState {
    /// Normal operation
    Normal,
    /// Warning state (degraded but operational)
    Warning { reason: String, since: Instant },
    /// Safe mode (limited torque)
    SafeMode {
        triggered_by: SafetyTrigger,
        since: Instant,
    },
    /// Emergency stop (zero torque)
    EmergencyStop { since: Instant },
}

/// Trigger that caused safe mode entry
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyTrigger {
    /// Watchdog timeout
    WatchdogTimeout,
    /// Communication loss
    CommunicationLoss,
    /// Fault detected
    FaultDetected(FaultType),
    /// Emergency stop command
    EmergencyStopCommand,
    /// Torque limit exceeded
    TorqueLimitExceeded,
}

impl std::fmt::Display for SafetyTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyTrigger::WatchdogTimeout => write!(f, "Watchdog timeout"),
            SafetyTrigger::CommunicationLoss => write!(f, "Communication loss"),
            SafetyTrigger::FaultDetected(fault) => write!(f, "Fault detected: {}", fault),
            SafetyTrigger::EmergencyStopCommand => write!(f, "Emergency stop command"),
            SafetyTrigger::TorqueLimitExceeded => write!(f, "Torque limit exceeded"),
        }
    }
}

/// Torque limit configuration
#[derive(Debug, Clone)]
pub struct TorqueLimit {
    /// Maximum allowed torque in Nm
    pub max_torque_nm: f32,
    /// Safe mode torque limit in Nm
    pub safe_mode_torque_nm: f32,
    /// Whether to log violations
    pub log_violations: bool,
    /// Violation count
    pub violation_count: u64,
}

impl TorqueLimit {
    /// Create new torque limit configuration
    pub fn new(max_torque_nm: f32, safe_mode_torque_nm: f32) -> Self {
        Self {
            max_torque_nm,
            safe_mode_torque_nm,
            log_violations: true,
            violation_count: 0,
        }
    }

    /// Clamp torque to the maximum allowed value
    pub fn clamp(&mut self, torque: f32) -> (f32, bool) {
        let clamped = torque.clamp(-self.max_torque_nm, self.max_torque_nm);
        let was_clamped = (clamped - torque).abs() > f32::EPSILON;
        if was_clamped {
            self.violation_count += 1;
        }
        (clamped, was_clamped)
    }

    /// Get safe mode torque limit
    pub fn safe_mode_limit(&self) -> f32 {
        self.safe_mode_torque_nm
    }
}

impl Default for TorqueLimit {
    fn default() -> Self {
        Self::new(25.0, 5.0) // 25Nm max, 5Nm safe mode
    }
}

/// Fault log entry for black box recording
#[derive(Debug, Clone)]
pub struct FaultLogEntry {
    pub timestamp: Instant,
    pub fault_type: FaultType,
    pub trigger: SafetyTrigger,
    pub torque_at_fault: f32,
    pub response_time: Duration,
    pub description: String,
}

impl SafetyInterlockSystem {
    /// Create a new safety interlock system
    pub fn new(watchdog: Box<dyn HardwareWatchdog>, max_torque_nm: f32) -> Self {
        Self {
            watchdog,
            timeout_handler: WatchdogTimeoutHandler::new(),
            safety_state: SafetyInterlockState::Normal,
            torque_limit: TorqueLimit::new(max_torque_nm, max_torque_nm * 0.2),
            fault_log: Vec::new(),
            max_fault_log_entries: 1000,
            fault_log_next_index: 0,
            communication_timeout: Duration::from_millis(50),
            last_communication: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        watchdog: Box<dyn HardwareWatchdog>,
        torque_limit: TorqueLimit,
        communication_timeout: Duration,
    ) -> Self {
        Self {
            watchdog,
            timeout_handler: WatchdogTimeoutHandler::new(),
            safety_state: SafetyInterlockState::Normal,
            torque_limit,
            fault_log: Vec::new(),
            max_fault_log_entries: 1000,
            fault_log_next_index: 0,
            communication_timeout,
            last_communication: None,
        }
    }

    /// Process a tick of the safety system
    ///
    /// This should be called from the RT loop on every tick.
    /// Returns the safe torque command to send to the device.
    pub fn process_tick(&mut self, requested_torque: f32) -> SafetyTickResult {
        let start = Instant::now();

        // Check watchdog timeout first (highest priority)
        if self.watchdog.has_timed_out() {
            return self.handle_watchdog_timeout(requested_torque, start);
        }

        // Check communication loss
        if self.check_communication_loss() {
            return self.handle_communication_loss(requested_torque, start);
        }

        // Feed the watchdog
        if let Err(e) = self.watchdog.feed() {
            return self.handle_watchdog_error(e, requested_torque, start);
        }

        // Apply torque limits based on current state
        let (safe_torque, was_clamped) = self.apply_torque_limits(requested_torque);

        if was_clamped && self.torque_limit.log_violations {
            self.log_torque_violation(requested_torque, safe_torque);
        }

        SafetyTickResult {
            torque_command: safe_torque,
            state: self.safety_state.clone(),
            response_time: start.elapsed(),
            fault_occurred: false,
            fault_type: None,
        }
    }

    /// Handle watchdog timeout
    fn handle_watchdog_timeout(&mut self, current_torque: f32, start: Instant) -> SafetyTickResult {
        let response = self.timeout_handler.handle_timeout(current_torque);

        // Transition to safe mode
        self.safety_state = SafetyInterlockState::SafeMode {
            triggered_by: SafetyTrigger::WatchdogTimeout,
            since: Instant::now(),
        };

        // Log the fault
        self.log_fault(
            FaultType::SafetyInterlockViolation,
            SafetyTrigger::WatchdogTimeout,
            current_torque,
            start.elapsed(),
            "Watchdog timeout - zero torque commanded".to_string(),
        );

        SafetyTickResult {
            torque_command: 0.0, // Always zero on watchdog timeout
            state: self.safety_state.clone(),
            response_time: response.response_time,
            fault_occurred: true,
            fault_type: Some(response.fault_type),
        }
    }

    /// Handle communication loss
    fn handle_communication_loss(
        &mut self,
        current_torque: f32,
        start: Instant,
    ) -> SafetyTickResult {
        // Transition to safe mode
        self.safety_state = SafetyInterlockState::SafeMode {
            triggered_by: SafetyTrigger::CommunicationLoss,
            since: Instant::now(),
        };

        // Log the fault
        self.log_fault(
            FaultType::UsbStall,
            SafetyTrigger::CommunicationLoss,
            current_torque,
            start.elapsed(),
            "Communication loss - zero torque commanded".to_string(),
        );

        SafetyTickResult {
            torque_command: 0.0, // Zero torque on communication loss
            state: self.safety_state.clone(),
            response_time: start.elapsed(),
            fault_occurred: true,
            fault_type: Some(FaultType::UsbStall),
        }
    }

    /// Handle watchdog error
    fn handle_watchdog_error(
        &mut self,
        error: WatchdogError,
        current_torque: f32,
        start: Instant,
    ) -> SafetyTickResult {
        match error {
            WatchdogError::TimedOut => self.handle_watchdog_timeout(current_torque, start),
            WatchdogError::NotArmed => {
                // Watchdog not armed - continue with limited torque
                let (safe_torque, _) = self.apply_torque_limits(current_torque);
                SafetyTickResult {
                    torque_command: safe_torque,
                    state: self.safety_state.clone(),
                    response_time: start.elapsed(),
                    fault_occurred: false,
                    fault_type: None,
                }
            }
            _ => {
                // Other errors - enter safe mode
                self.safety_state = SafetyInterlockState::SafeMode {
                    triggered_by: SafetyTrigger::FaultDetected(FaultType::SafetyInterlockViolation),
                    since: Instant::now(),
                };

                SafetyTickResult {
                    torque_command: 0.0,
                    state: self.safety_state.clone(),
                    response_time: start.elapsed(),
                    fault_occurred: true,
                    fault_type: Some(FaultType::SafetyInterlockViolation),
                }
            }
        }
    }

    /// Apply torque limits based on current state
    fn apply_torque_limits(&mut self, requested_torque: f32) -> (f32, bool) {
        match &self.safety_state {
            SafetyInterlockState::Normal => self.torque_limit.clamp(requested_torque),
            SafetyInterlockState::Warning { .. } => {
                // In warning state, use safe mode limit
                let limit = self.torque_limit.safe_mode_limit();
                let clamped = requested_torque.clamp(-limit, limit);
                let was_clamped = (clamped - requested_torque).abs() > f32::EPSILON;
                (clamped, was_clamped)
            }
            SafetyInterlockState::SafeMode { .. } => {
                // In safe mode, use safe mode limit
                let limit = self.torque_limit.safe_mode_limit();
                let clamped = requested_torque.clamp(-limit, limit);
                let was_clamped = (clamped - requested_torque).abs() > f32::EPSILON;
                (clamped, was_clamped)
            }
            SafetyInterlockState::EmergencyStop { .. } => {
                // Emergency stop - always zero
                (0.0, requested_torque.abs() > f32::EPSILON)
            }
        }
    }

    /// Check for communication loss
    fn check_communication_loss(&self) -> bool {
        if let Some(last_comm) = self.last_communication {
            last_comm.elapsed() > self.communication_timeout
        } else {
            false
        }
    }

    /// Report successful communication
    pub fn report_communication(&mut self) {
        self.last_communication = Some(Instant::now());
    }

    /// Age the communication timestamp for deterministic timeout tests.
    #[cfg(test)]
    pub(crate) fn age_last_communication(&mut self, duration: Duration) {
        if let Some(last_communication) = self.last_communication {
            let now = Instant::now();
            self.last_communication = Some(
                last_communication
                    .checked_sub(duration)
                    .map_or(now, |aged| aged),
            );
        }
    }

    /// Log a torque violation
    fn log_torque_violation(&mut self, requested: f32, actual: f32) {
        self.log_fault(
            FaultType::SafetyInterlockViolation,
            SafetyTrigger::TorqueLimitExceeded,
            requested,
            Duration::ZERO,
            format!(
                "Torque limit exceeded: requested {:.2}Nm, clamped to {:.2}Nm",
                requested, actual
            ),
        );
    }

    /// Log a fault to the fault log
    fn log_fault(
        &mut self,
        fault_type: FaultType,
        trigger: SafetyTrigger,
        torque: f32,
        response_time: Duration,
        description: String,
    ) {
        if self.max_fault_log_entries == 0 {
            return;
        }

        let entry = FaultLogEntry {
            timestamp: Instant::now(),
            fault_type,
            trigger,
            torque_at_fault: torque,
            response_time,
            description,
        };

        if self.fault_log.len() < self.max_fault_log_entries {
            self.fault_log.push(entry);
            if self.fault_log.len() == self.max_fault_log_entries {
                self.fault_log_next_index = 0;
            }
        } else {
            self.fault_log[self.fault_log_next_index] = entry;
            self.fault_log_next_index =
                (self.fault_log_next_index + 1) % self.max_fault_log_entries;
        }
    }

    /// Emergency stop - immediately zero torque
    pub fn emergency_stop(&mut self) -> SafetyTickResult {
        let start = Instant::now();

        self.safety_state = SafetyInterlockState::EmergencyStop {
            since: Instant::now(),
        };

        self.log_fault(
            FaultType::SafetyInterlockViolation,
            SafetyTrigger::EmergencyStopCommand,
            0.0,
            Duration::ZERO,
            "Emergency stop commanded".to_string(),
        );

        SafetyTickResult {
            torque_command: 0.0,
            state: self.safety_state.clone(),
            response_time: start.elapsed(),
            fault_occurred: true,
            fault_type: Some(FaultType::SafetyInterlockViolation),
        }
    }

    /// Report a fault and enter safe mode
    pub fn report_fault(&mut self, fault_type: FaultType) {
        self.safety_state = SafetyInterlockState::SafeMode {
            triggered_by: SafetyTrigger::FaultDetected(fault_type),
            since: Instant::now(),
        };

        self.log_fault(
            fault_type,
            SafetyTrigger::FaultDetected(fault_type),
            0.0,
            Duration::ZERO,
            format!("Fault reported: {}", fault_type),
        );
    }

    /// Clear fault and return to normal operation
    pub fn clear_fault(&mut self) -> Result<(), String> {
        match &self.safety_state {
            SafetyInterlockState::SafeMode { since, .. } => {
                // Require minimum time in safe mode before clearing
                if since.elapsed() < Duration::from_millis(100) {
                    return Err("Must wait at least 100ms before clearing fault".to_string());
                }
                self.safety_state = SafetyInterlockState::Normal;
                self.timeout_handler.reset();
                Ok(())
            }
            SafetyInterlockState::EmergencyStop { .. } => {
                Err("Cannot clear emergency stop - manual reset required".to_string())
            }
            _ => Err("No fault to clear".to_string()),
        }
    }

    /// Reset the system (requires manual intervention)
    pub fn reset(&mut self) -> Result<(), WatchdogError> {
        self.watchdog.reset()?;
        self.timeout_handler.reset();
        self.safety_state = SafetyInterlockState::Normal;
        self.last_communication = None;
        Ok(())
    }

    /// Arm the watchdog
    pub fn arm(&mut self) -> Result<(), WatchdogError> {
        self.watchdog.arm()
    }

    /// Disarm the watchdog
    pub fn disarm(&mut self) -> Result<(), WatchdogError> {
        self.watchdog.disarm()
    }

    /// Get current safety state
    pub fn state(&self) -> &SafetyInterlockState {
        &self.safety_state
    }

    /// Get fault log
    pub fn fault_log(&self) -> &[FaultLogEntry] {
        &self.fault_log
    }

    /// Get torque limit configuration
    pub fn torque_limit(&self) -> &TorqueLimit {
        &self.torque_limit
    }

    /// Get mutable torque limit configuration
    pub fn torque_limit_mut(&mut self) -> &mut TorqueLimit {
        &mut self.torque_limit
    }

    /// Check if watchdog is armed
    pub fn is_watchdog_armed(&self) -> bool {
        self.watchdog.is_armed()
    }

    /// Get watchdog timeout in milliseconds
    pub fn watchdog_timeout_ms(&self) -> u32 {
        self.watchdog.timeout_ms()
    }
}

/// Result of a safety tick
#[derive(Debug, Clone)]
pub struct SafetyTickResult {
    /// Torque command to send to device
    pub torque_command: f32,
    /// Current safety state
    pub state: SafetyInterlockState,
    /// Time taken to process this tick
    pub response_time: Duration,
    /// Whether a fault occurred
    pub fault_occurred: bool,
    /// Type of fault if one occurred
    pub fault_type: Option<FaultType>,
}

#[cfg(test)]
mod safety_interlock_tests {
    use super::*;

    fn create_test_system() -> SafetyInterlockSystem {
        let watchdog = Box::new(SoftwareWatchdog::new(100));
        SafetyInterlockSystem::new(watchdog, 25.0)
    }

    #[test]
    fn test_safety_interlock_creation() -> Result<(), WatchdogError> {
        let system = create_test_system();
        assert_eq!(*system.state(), SafetyInterlockState::Normal);
        assert!(!system.is_watchdog_armed());
        Ok(())
    }

    #[test]
    fn test_safety_interlock_normal_operation() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        let result = system.process_tick(10.0);
        assert_eq!(result.torque_command, 10.0);
        assert!(!result.fault_occurred);
        assert_eq!(result.state, SafetyInterlockState::Normal);

        Ok(())
    }

    #[test]
    fn test_safety_interlock_torque_clamping() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        // Request torque above limit
        let result = system.process_tick(30.0);
        assert_eq!(result.torque_command, 25.0); // Clamped to max
        assert!(!result.fault_occurred);

        // Request negative torque above limit
        let result = system.process_tick(-30.0);
        assert_eq!(result.torque_command, -25.0); // Clamped to -max

        Ok(())
    }

    #[test]
    fn test_safety_interlock_watchdog_timeout() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(10)); // 10ms timeout
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        // Feed once
        let _ = system.process_tick(10.0);

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(15));

        // Next tick should detect timeout
        let result = system.process_tick(10.0);
        assert_eq!(result.torque_command, 0.0); // Zero torque on timeout
        assert!(result.fault_occurred);
        assert!(matches!(
            result.state,
            SafetyInterlockState::SafeMode {
                triggered_by: SafetyTrigger::WatchdogTimeout,
                ..
            }
        ));

        Ok(())
    }

    #[test]
    fn test_safety_interlock_emergency_stop() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        let result = system.emergency_stop();
        assert_eq!(result.torque_command, 0.0);
        assert!(result.fault_occurred);
        assert!(matches!(
            result.state,
            SafetyInterlockState::EmergencyStop { .. }
        ));

        // Cannot clear emergency stop
        assert!(system.clear_fault().is_err());

        Ok(())
    }

    #[test]
    fn test_safety_interlock_fault_reporting() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        system.report_fault(FaultType::ThermalLimit);

        assert!(matches!(
            system.state(),
            SafetyInterlockState::SafeMode {
                triggered_by: SafetyTrigger::FaultDetected(FaultType::ThermalLimit),
                ..
            }
        ));

        // Torque should be limited in safe mode
        let result = system.process_tick(20.0);
        assert!(result.torque_command <= system.torque_limit().safe_mode_limit());

        Ok(())
    }

    #[test]
    fn test_safety_interlock_fault_clearing() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        system.report_fault(FaultType::ThermalLimit);

        // Cannot clear immediately
        assert!(system.clear_fault().is_err());

        // Wait minimum time
        std::thread::sleep(Duration::from_millis(110));

        // Now can clear
        assert!(system.clear_fault().is_ok());
        assert_eq!(*system.state(), SafetyInterlockState::Normal);

        Ok(())
    }

    #[test]
    fn test_safety_interlock_fault_log() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        system.report_fault(FaultType::ThermalLimit);
        system.report_fault(FaultType::Overcurrent);

        let log = system.fault_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].fault_type, FaultType::ThermalLimit);
        assert_eq!(log[1].fault_type, FaultType::Overcurrent);

        Ok(())
    }

    #[test]
    fn test_safety_interlock_fault_log_stays_bounded_without_shifting() -> Result<(), WatchdogError>
    {
        let mut system = create_test_system();
        system.max_fault_log_entries = 3;
        system.arm()?;

        system.report_fault(FaultType::ThermalLimit);
        system.report_fault(FaultType::Overcurrent);
        system.report_fault(FaultType::UsbStall);
        system.report_fault(FaultType::EncoderNaN);
        system.report_fault(FaultType::PipelineFault);

        let log = system.fault_log();
        assert_eq!(log.len(), 3);
        assert!(
            log.iter()
                .any(|entry| entry.fault_type == FaultType::UsbStall)
        );
        assert!(
            log.iter()
                .any(|entry| entry.fault_type == FaultType::EncoderNaN)
        );
        assert!(
            log.iter()
                .any(|entry| entry.fault_type == FaultType::PipelineFault)
        );

        Ok(())
    }

    #[test]
    fn test_torque_limit_clamping() {
        let mut limit = TorqueLimit::new(20.0, 5.0);

        let (clamped, was_clamped) = limit.clamp(15.0);
        assert_eq!(clamped, 15.0);
        assert!(!was_clamped);

        let (clamped, was_clamped) = limit.clamp(25.0);
        assert_eq!(clamped, 20.0);
        assert!(was_clamped);

        let (clamped, was_clamped) = limit.clamp(-25.0);
        assert_eq!(clamped, -20.0);
        assert!(was_clamped);

        assert_eq!(limit.violation_count, 2);
    }

    #[test]
    fn test_watchdog_timeout_response_within_budget() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(10));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        // Feed once
        let _ = system.process_tick(10.0);

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(15));

        // Check response time is within 1ms budget
        let result = system.process_tick(10.0);
        assert!(
            result.response_time < Duration::from_millis(1),
            "Response time {:?} exceeded 1ms budget",
            result.response_time
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Hardening tests: successive timeouts, recovery, NaN/Inf torque, e-stop
    // -----------------------------------------------------------------------

    #[test]
    fn test_successive_watchdog_timeouts_stay_in_safe_mode() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(10));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        let _ = system.process_tick(10.0);
        std::thread::sleep(Duration::from_millis(15));

        // Multiple ticks after timeout should all yield zero torque
        for _ in 0..5 {
            let result = system.process_tick(20.0);
            assert_eq!(result.torque_command, 0.0);
            assert!(matches!(
                result.state,
                SafetyInterlockState::SafeMode { .. }
            ));
        }

        Ok(())
    }

    #[test]
    fn test_reset_after_timeout_restores_normal_operation() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(10));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        let _ = system.process_tick(10.0);
        std::thread::sleep(Duration::from_millis(15));

        let result = system.process_tick(10.0);
        assert_eq!(result.torque_command, 0.0);

        // Reset fully restores normal state
        system.reset()?;
        assert_eq!(*system.state(), SafetyInterlockState::Normal);

        // Re-arm and normal ticks work again
        system.arm()?;
        let result = system.process_tick(8.0);
        assert_eq!(result.torque_command, 8.0);
        assert!(!result.fault_occurred);

        Ok(())
    }

    #[test]
    fn test_emergency_stop_from_safe_mode() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        // Enter safe mode via fault
        system.report_fault(FaultType::ThermalLimit);
        assert!(matches!(
            system.state(),
            SafetyInterlockState::SafeMode { .. }
        ));

        // Emergency stop from safe mode
        let result = system.emergency_stop();
        assert_eq!(result.torque_command, 0.0);
        assert!(matches!(
            result.state,
            SafetyInterlockState::EmergencyStop { .. }
        ));

        // Cannot clear emergency stop
        assert!(system.clear_fault().is_err());

        Ok(())
    }

    #[test]
    fn test_emergency_stop_always_zero_torque() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        system.emergency_stop();

        // Even large torque requests yield zero
        let result = system.process_tick(100.0);
        assert_eq!(result.torque_command, 0.0);

        let result = system.process_tick(-100.0);
        assert_eq!(result.torque_command, 0.0);

        Ok(())
    }

    #[test]
    fn test_nan_torque_passes_through_interlock() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        let result = system.process_tick(f32::NAN);
        // TorqueLimit::clamp uses f32::clamp which propagates NaN.
        // The higher-level SafetyService handles NaN → 0.0 conversion.
        assert!(
            result.torque_command.is_nan(),
            "NaN should propagate through TorqueLimit (sanitized at SafetyService layer)"
        );

        Ok(())
    }

    #[test]
    fn test_inf_torque_clamped_in_normal_mode() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        let result = system.process_tick(f32::INFINITY);
        assert!(result.torque_command.is_finite());
        assert!(result.torque_command.abs() <= 25.0);

        let result = system.process_tick(f32::NEG_INFINITY);
        assert!(result.torque_command.is_finite());
        assert!(result.torque_command.abs() <= 25.0);

        Ok(())
    }

    #[test]
    fn test_torque_limit_violation_count_increments() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        let initial_count = system.torque_limit().violation_count;
        system.process_tick(30.0); // Above 25Nm limit
        assert!(system.torque_limit().violation_count > initial_count);

        Ok(())
    }

    #[test]
    fn test_communication_loss_followed_by_recovery() -> Result<(), WatchdogError> {
        let watchdog = Box::new(SoftwareWatchdog::new(30_000));
        let torque_limit = TorqueLimit::new(25.0, 5.0);
        let mut system =
            SafetyInterlockSystem::with_config(watchdog, torque_limit, Duration::from_millis(20));
        system.arm()?;

        system.report_communication();
        std::thread::sleep(Duration::from_millis(25));

        let result = system.process_tick(10.0);
        assert_eq!(result.torque_command, 0.0);
        assert!(result.fault_occurred);

        // Wait minimum safe-mode duration, then clear
        std::thread::sleep(Duration::from_millis(110));
        assert!(system.clear_fault().is_ok());
        assert_eq!(*system.state(), SafetyInterlockState::Normal);

        Ok(())
    }

    #[test]
    fn test_safe_mode_limits_torque_to_safe_mode_limit() -> Result<(), WatchdogError> {
        let mut system = create_test_system();
        system.arm()?;

        system.report_fault(FaultType::Overcurrent);
        let safe_limit = system.torque_limit().safe_mode_limit();

        let result = system.process_tick(100.0);
        assert!(
            result.torque_command <= safe_limit,
            "Safe mode torque {} > safe limit {}",
            result.torque_command,
            safe_limit
        );

        Ok(())
    }
}

/// Property-based tests for safety interlocks
///
/// These tests verify the correctness properties defined in the design document
/// for the safety interlock system.
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for generating valid torque values
    fn torque_strategy() -> impl Strategy<Value = f32> {
        prop::num::f32::NORMAL.prop_map(|v| v.abs() % 100.0)
    }

    /// Strategy for generating watchdog timeout values (10-500ms)
    fn timeout_strategy() -> impl Strategy<Value = u32> {
        10u32..500u32
    }

    /// Strategy for generating max torque limits (5-50 Nm)
    fn max_torque_strategy() -> impl Strategy<Value = f32> {
        5.0f32..50.0f32
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: release-roadmap-v1, Property 32: Watchdog Timeout Response
        ///
        /// *For any* watchdog timeout event, the safety system SHALL command
        /// zero torque within 1ms of the timeout.
        ///
        /// **Validates: Requirements 18.2**
        #[test]
        fn prop_watchdog_timeout_commands_zero_torque(
            current_torque in torque_strategy(),
            timeout_ms in timeout_strategy(),
        ) {
            // Create a watchdog with the specified timeout
            let watchdog = Box::new(SoftwareWatchdog::new(timeout_ms));
            let mut system = SafetyInterlockSystem::new(watchdog, 25.0);

            // Arm the watchdog
            system.arm().map_err(|e| TestCaseError::fail(format!("Failed to arm: {:?}", e)))?;

            // Feed once to start the timer
            let _ = system.process_tick(current_torque);

            // Wait for timeout to occur
            std::thread::sleep(Duration::from_millis((timeout_ms + 5) as u64));

            // Process tick after timeout - should command zero torque
            let result = system.process_tick(current_torque);

            // Property: torque command must be zero on timeout
            prop_assert_eq!(
                result.torque_command,
                0.0,
                "Watchdog timeout must command zero torque, got {}",
                result.torque_command
            );

            // Property: fault must be reported
            prop_assert!(
                result.fault_occurred,
                "Watchdog timeout must report a fault"
            );

            // Property: response time must be within 1ms budget
            prop_assert!(
                result.response_time < Duration::from_millis(1),
                "Response time {:?} exceeded 1ms budget",
                result.response_time
            );

            // Property: state must transition to safe mode
            prop_assert!(
                matches!(
                    result.state,
                    SafetyInterlockState::SafeMode {
                        triggered_by: SafetyTrigger::WatchdogTimeout,
                        ..
                    }
                ),
                "State must be SafeMode with WatchdogTimeout trigger, got {:?}",
                result.state
            );
        }

        /// Feature: release-roadmap-v1, Property 32 (continued): Watchdog Timeout Response
        ///
        /// Verify that the timeout handler always produces zero torque regardless
        /// of the input torque value.
        ///
        /// **Validates: Requirements 18.2**
        #[test]
        fn prop_timeout_handler_always_zeros_torque(
            current_torque in torque_strategy(),
        ) {
            let mut handler = WatchdogTimeoutHandler::new();

            let response = handler.handle_timeout(current_torque);

            // Property: torque command is always zero
            prop_assert_eq!(
                response.torque_command,
                0.0,
                "Timeout handler must always command zero torque"
            );

            // Property: previous torque is preserved for logging
            prop_assert!(
                (response.previous_torque - current_torque).abs() < f32::EPSILON,
                "Previous torque should be preserved"
            );

            // Property: response is within budget
            prop_assert!(
                response.within_budget,
                "Response must be within timing budget"
            );
        }

        /// Feature: release-roadmap-v1, Property 33: Torque Limit Enforcement
        ///
        /// *For any* torque command exceeding the device's maximum torque capability,
        /// the safety system SHALL clamp the output to the device maximum.
        ///
        /// **Validates: Requirements 18.3**
        #[test]
        fn prop_torque_limit_enforcement(
            requested_torque in torque_strategy(),
            max_torque in max_torque_strategy(),
        ) {
            let watchdog = Box::new(SoftwareWatchdog::new(100));
            let mut system = SafetyInterlockSystem::new(watchdog, max_torque);

            // Arm the watchdog
            system.arm().map_err(|e| TestCaseError::fail(format!("Failed to arm: {:?}", e)))?;

            let result = system.process_tick(requested_torque);

            // Property: output torque must never exceed max_torque
            prop_assert!(
                result.torque_command.abs() <= max_torque,
                "Torque {} exceeded max limit {}",
                result.torque_command,
                max_torque
            );

            // Property: if requested was within limits, output equals requested
            if requested_torque.abs() <= max_torque {
                prop_assert!(
                    (result.torque_command - requested_torque).abs() < f32::EPSILON,
                    "Torque within limits should pass through unchanged"
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 33 (continued): Torque Limit Enforcement
        ///
        /// Verify that torque violations are logged when they occur.
        ///
        /// **Validates: Requirements 18.3**
        #[test]
        fn prop_torque_violations_are_logged(
            requested_torque in 30.0f32..100.0f32, // Always above typical max
            max_torque in 5.0f32..25.0f32,
        ) {
            let mut limit = TorqueLimit::new(max_torque, max_torque * 0.2);
            limit.log_violations = true;

            let initial_count = limit.violation_count;
            let (clamped, was_clamped) = limit.clamp(requested_torque);

            // Property: clamped value is at max
            prop_assert!(
                (clamped - max_torque).abs() < f32::EPSILON,
                "Clamped value should equal max_torque"
            );

            // Property: violation was detected
            prop_assert!(was_clamped, "Violation should be detected");

            // Property: violation count increased
            prop_assert_eq!(
                limit.violation_count,
                initial_count + 1,
                "Violation count should increase"
            );
        }

        /// Feature: release-roadmap-v1, Property 34: Fault Detection Response
        ///
        /// *For any* detected fault condition, the safety system SHALL transition
        /// to safe mode and log the fault with timestamp and fault code.
        ///
        /// **Validates: Requirements 18.4**
        #[test]
        fn prop_fault_detection_enters_safe_mode(
            fault_type_idx in 0usize..9usize,
            current_torque in torque_strategy(),
        ) {
            let fault_types = [
                FaultType::UsbStall,
                FaultType::EncoderNaN,
                FaultType::ThermalLimit,
                FaultType::Overcurrent,
                FaultType::PluginOverrun,
                FaultType::TimingViolation,
                FaultType::SafetyInterlockViolation,
                FaultType::HandsOffTimeout,
                FaultType::PipelineFault,
            ];

            let fault_type = fault_types[fault_type_idx];

            let watchdog = Box::new(SoftwareWatchdog::new(100));
            let mut system = SafetyInterlockSystem::new(watchdog, 25.0);

            // Arm the watchdog
            system.arm().map_err(|e| TestCaseError::fail(format!("Failed to arm: {:?}", e)))?;

            // Report the fault
            system.report_fault(fault_type);

            // Property: state must be SafeMode
            prop_assert!(
                matches!(
                    system.state(),
                    SafetyInterlockState::SafeMode {
                        triggered_by: SafetyTrigger::FaultDetected(_),
                        ..
                    }
                ),
                "State must be SafeMode after fault, got {:?}",
                system.state()
            );

            // Property: fault must be logged
            let log = system.fault_log();
            prop_assert!(
                !log.is_empty(),
                "Fault log must not be empty after fault"
            );

            // Property: logged fault type matches reported fault
            let last_entry = log.last().ok_or_else(|| TestCaseError::fail("No log entry"))?;
            prop_assert_eq!(
                last_entry.fault_type,
                fault_type,
                "Logged fault type must match reported fault"
            );

            // Property: torque is limited in safe mode
            let result = system.process_tick(current_torque);
            let safe_limit = system.torque_limit().safe_mode_limit();
            prop_assert!(
                result.torque_command.abs() <= safe_limit,
                "Torque {} exceeded safe mode limit {}",
                result.torque_command,
                safe_limit
            );
        }

        /// Feature: release-roadmap-v1, Property 35: Communication Loss Response
        ///
        /// *For any* communication loss event, the safety system SHALL reach
        /// safe state (zero torque) within 50ms.
        ///
        /// **Validates: Requirements 18.6**
        #[test]
        fn prop_communication_loss_response(
            current_torque in torque_strategy(),
            comm_timeout_ms in 10u64..50u64,
        ) {
            // Use a large watchdog timeout so it cannot interfere with the
            // communication-loss detection test, even under heavy CI load where
            // thread::sleep may significantly overshoot the requested duration.
            let watchdog = Box::new(SoftwareWatchdog::new(30_000));
            let torque_limit = TorqueLimit::new(25.0, 5.0);
            let mut system = SafetyInterlockSystem::with_config(
                watchdog,
                torque_limit,
                Duration::from_millis(comm_timeout_ms),
            );

            // Arm the watchdog
            system.arm().map_err(|e| TestCaseError::fail(format!("Failed to arm: {:?}", e)))?;

            // Report initial communication
            system.report_communication();

            // Process a tick (should be normal)
            let result = system.process_tick(current_torque);
            prop_assert!(
                !result.fault_occurred,
                "Should not fault with recent communication"
            );

            // Wait for communication timeout
            std::thread::sleep(Duration::from_millis(comm_timeout_ms + 10));

            // Process tick after communication loss
            let start = Instant::now();
            let result = system.process_tick(current_torque);
            let response_time = start.elapsed();

            // Property: torque must be zero on communication loss
            prop_assert_eq!(
                result.torque_command,
                0.0,
                "Communication loss must command zero torque"
            );

            // Property: fault must be reported
            prop_assert!(
                result.fault_occurred,
                "Communication loss must report a fault"
            );

            // Property: response time must be within 50ms budget
            prop_assert!(
                response_time < Duration::from_millis(50),
                "Response time {:?} exceeded 50ms budget",
                response_time
            );

            // Property: state must be SafeMode with CommunicationLoss trigger
            prop_assert!(
                matches!(
                    result.state,
                    SafetyInterlockState::SafeMode {
                        triggered_by: SafetyTrigger::CommunicationLoss,
                        ..
                    }
                ),
                "State must be SafeMode with CommunicationLoss trigger, got {:?}",
                result.state
            );
        }

        /// Feature: release-roadmap-v1, Property 35 (continued): Communication Loss Response
        ///
        /// Verify that communication loss is detected based on timeout threshold.
        ///
        /// **Validates: Requirements 18.6**
        #[test]
        fn prop_communication_loss_detection_threshold(
            timeout_ms in 10u64..100u64,
            wait_factor in 1.1f64..2.0f64,
        ) {
            let watchdog = Box::new(SoftwareWatchdog::new(500)); // Long watchdog timeout
            let torque_limit = TorqueLimit::new(25.0, 5.0);
            let mut system = SafetyInterlockSystem::with_config(
                watchdog,
                torque_limit,
                Duration::from_millis(timeout_ms),
            );

            system.arm().map_err(|e| TestCaseError::fail(format!("Failed to arm: {:?}", e)))?;

            // Report communication
            system.report_communication();

            // Wait for timeout * factor
            let wait_time = (timeout_ms as f64 * wait_factor) as u64;
            std::thread::sleep(Duration::from_millis(wait_time));

            // Should detect communication loss
            let result = system.process_tick(10.0);

            prop_assert!(
                result.fault_occurred,
                "Should detect communication loss after {}ms (timeout={}ms)",
                wait_time,
                timeout_ms
            );
        }
    }
}
