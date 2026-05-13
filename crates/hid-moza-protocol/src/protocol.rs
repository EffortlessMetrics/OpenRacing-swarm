//! Moza protocol handler: initialization handshake, input parsing, FFB configuration.
//!
//! Supports V1 (0x000x) and V2 (0x001x) hardware revisions.
//!
//! # Safety and Initialization
//! This implementation follows [ADR-0006] regarding safety interlocks and
//! handshake sequencing to ensure zero-torque states on fault.
//!
//! [ADR-0006]: file:///h:/Code/Rust/OpenRacing/docs/adr/0006-safety-interlocks.md

//!
//! # Serial configuration protocol (from boxflat community tool)
//!
//! Moza devices expose a CDC ACM serial interface for settings/configuration
//! alongside the HID interface used for input/FFB. The serial protocol uses:
//!
//! - **Baud rate**: 115200 (8N1)
//! - **Message start byte**: `0x7E`
//! - **Frame format**: `[0x7E, length, group, device_id, cmd_id..., payload..., checksum]`
//! - **Checksum**: `(magic_value + sum_of_all_frame_bytes) % 256`, magic = 13
//! - **Device IDs (serial)**: base=19, wheel=23, pedals=25, shifter=26,
//!   handbrake=27, e-stop=28, hub/main=18
//!
//! This crate handles the **HID** interface (input reports and FFB output),
//! not the serial configuration protocol. The serial protocol details are
//! documented here for cross-reference.
//!
//! Source: <https://github.com/Lawstorant/boxflat> (`data/serial.yml`,
//! `boxflat/serial_handler.py`, `boxflat/moza_command.py`)

#![deny(static_mut_refs)]

use crate::direct::REPORT_LEN;
use crate::ids::rim_ids;
use crate::report::{
    RawWheelbaseReport, input_report, looks_like_live_r5_v1_extended_report,
    parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report, report_ids,
};
use crate::types::{
    MozaDeviceCategory, MozaInputState, MozaModel, MozaPedalAxesRaw, es_compatibility,
    identify_device, is_wheelbase_product,
};
use crate::writer::{DeviceWriter, FfbConfig, VendorProtocol};
use racing_wheel_hbp::parse_hbp_usb_report_best_effort;
use racing_wheel_ks::{
    KS_ENCODER_COUNT, KsByteSource, KsClutchMode, KsJoystickMode, KsReportMap, KsReportSnapshot,
    KsRotaryMode,
};
use racing_wheel_srp::parse_srp_usb_report_best_effort;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{debug, info, warn};

const MOZA_INIT_STATE_UNINITIALIZED: u8 = 0;
const MOZA_INIT_STATE_INITIALIZING: u8 = 1;
const MOZA_INIT_STATE_READY: u8 = 2;
const MOZA_INIT_STATE_FAILED: u8 = 3;
const MOZA_INIT_STATE_PERMANENT_FAILURE: u8 = 4;

/// Default maximum number of handshake retries before giving up.
pub const DEFAULT_MAX_RETRIES: u8 = 3;

/// Retry policy for the Moza handshake state machine.
///
/// Controls how many times `initialize_device` can be retried after a failure
/// before the protocol transitions to `PermanentFailure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MozaRetryPolicy {
    /// Maximum number of retry attempts before entering `PermanentFailure`.
    pub max_retries: u8,
    /// Base delay in milliseconds between retries (doubles each attempt).
    pub base_delay_ms: u32,
}

impl Default for MozaRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay_ms: 500,
        }
    }
}

impl MozaRetryPolicy {
    /// Return the back-off delay for the given retry attempt (0-indexed).
    ///
    /// Delay is capped at 8x the base to avoid indefinite backoff.
    pub fn delay_ms_for(&self, attempt: u8) -> u32 {
        let shift = attempt.min(3) as u32;
        self.base_delay_ms.saturating_mul(1 << shift)
    }
}

/// Moza initialization lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaInitState {
    /// No handshake attempt has run on this protocol instance.
    Uninitialized,
    /// Handshake currently in progress.
    Initializing,
    /// Handshake completed successfully.
    Ready,
    /// Last handshake attempt failed; retries still available.
    Failed,
    /// Retry limit reached; manual reset required.
    PermanentFailure,
}

impl MozaInitState {
    fn from_u8(value: u8) -> Self {
        match value {
            MOZA_INIT_STATE_INITIALIZING => Self::Initializing,
            MOZA_INIT_STATE_READY => Self::Ready,
            MOZA_INIT_STATE_FAILED => Self::Failed,
            MOZA_INIT_STATE_PERMANENT_FAILURE => Self::PermanentFailure,
            _ => Self::Uninitialized,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Uninitialized => MOZA_INIT_STATE_UNINITIALIZED,
            Self::Initializing => MOZA_INIT_STATE_INITIALIZING,
            Self::Ready => MOZA_INIT_STATE_READY,
            Self::Failed => MOZA_INIT_STATE_FAILED,
            Self::PermanentFailure => MOZA_INIT_STATE_PERMANENT_FAILURE,
        }
    }
}

/// FFB mode options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfbMode {
    /// Keep FFB disabled.
    Off = 0xFF,
    /// Use vendor PID/PIDFF reporting mode.
    Standard = 0x00,
    /// Use raw direct torque mode.
    Direct = 0x02,
}

const MOZA_FFB_MODE_ENV: &str = "OPENRACING_MOZA_FFB_MODE";
const MOZA_HIGH_TORQUE_ENV: &str = "OPENRACING_MOZA_HIGH_TORQUE";
const MOZA_DESCRIPTOR_CRC32_ALLOWLIST_ENV: &str = "OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST";
const MOZA_ALLOW_UNKNOWN_SIGNATURE_ENV: &str = "OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE";

fn parse_ffb_mode(value: &str) -> Option<FfbMode> {
    match &*value.trim().to_ascii_lowercase() {
        "off" => Some(FfbMode::Off),
        "standard" | "pidff" | "pid" => Some(FfbMode::Standard),
        "direct" | "raw" => Some(FfbMode::Direct),
        "0" => Some(FfbMode::Standard),
        "2" => Some(FfbMode::Direct),
        _ => None,
    }
}

fn parse_bool_env(value: &str) -> bool {
    matches!(
        &*value.trim().to_ascii_lowercase(),
        "1" | "true" | "yes" | "on" | "enable" | "enabled"
    )
}

pub fn default_ffb_mode() -> FfbMode {
    std::env::var(MOZA_FFB_MODE_ENV)
        .ok()
        .and_then(|value| parse_ffb_mode(&value))
        .unwrap_or(FfbMode::Standard)
}

pub fn default_high_torque_enabled() -> bool {
    std::env::var(MOZA_HIGH_TORQUE_ENV)
        .ok()
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn parse_crc32_token(token: &str) -> Option<u32> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let raw = t.trim_start_matches("0x").trim_start_matches("0X");
    let looks_hex = t.starts_with("0x")
        || t.starts_with("0X")
        || raw.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F'));
    if looks_hex {
        u32::from_str_radix(raw, 16).ok()
    } else {
        t.parse::<u32>()
            .ok()
            .or_else(|| u32::from_str_radix(raw, 16).ok())
    }
}

fn crc32_allowlist() -> &'static [u32] {
    use std::sync::OnceLock;
    static ALLOWLIST: OnceLock<Vec<u32>> = OnceLock::new();
    ALLOWLIST
        .get_or_init(|| {
            let raw = std::env::var(MOZA_DESCRIPTOR_CRC32_ALLOWLIST_ENV).unwrap_or_default();
            raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                .filter_map(parse_crc32_token)
                .collect()
        })
        .as_slice()
}

/// Returns `true` when the given descriptor CRC32 is allowed to use high-risk paths
/// (high torque, direct FFB mode) without an explicit override.
pub fn signature_is_trusted(descriptor_crc32: Option<u32>) -> bool {
    if std::env::var(MOZA_ALLOW_UNKNOWN_SIGNATURE_ENV)
        .ok()
        .as_deref()
        .is_some_and(parse_bool_env)
    {
        return true;
    }
    let Some(crc) = descriptor_crc32 else {
        return false;
    };
    crc32_allowlist().contains(&crc)
}

/// Returns the effective FFB mode to use given a requested mode and device signature.
///
/// If `Direct` mode is requested but the signature is not trusted, downgrades to `Standard`.
pub fn effective_ffb_mode(requested: FfbMode, descriptor_crc32: Option<u32>) -> FfbMode {
    if matches!(requested, FfbMode::Direct) && !signature_is_trusted(descriptor_crc32) {
        FfbMode::Standard
    } else {
        requested
    }
}

/// Returns `true` when high torque should actually be enabled given the current env + signature.
///
/// Requires both `OPENRACING_MOZA_HIGH_TORQUE=1` AND a trusted signature (or the escape hatch).
pub fn effective_high_torque_opt_in(descriptor_crc32: Option<u32>) -> bool {
    default_high_torque_enabled() && signature_is_trusted(descriptor_crc32)
}

fn default_wheelbase_ks_map() -> KsReportMap {
    KsReportMap {
        report_id: Some(input_report::REPORT_ID),
        buttons_offset: Some(input_report::BUTTONS_START),
        hat_offset: Some(input_report::HAT_START),
        encoders: [None; KS_ENCODER_COUNT],
        clutch_left_axis: None,
        clutch_right_axis: None,
        clutch_combined_axis: None,
        clutch_left_button: None,
        clutch_right_button: None,
        clutch_mode_hint: KsClutchMode::Unknown,
        rotary_mode_hint: KsRotaryMode::Unknown,
        left_rotary_axis: None,
        right_rotary_axis: None,
        joystick_mode_hint: KsJoystickMode::Unknown,
        joystick_hat: Some(KsByteSource::new(input_report::HAT_START)),
    }
}

fn live_r5_v1_extended_wheelbase_ks_map() -> KsReportMap {
    KsReportMap {
        report_id: Some(input_report::REPORT_ID),
        buttons_offset: Some(input_report::R5_V1_EXTENDED_BUTTONS_START),
        hat_offset: Some(input_report::R5_V1_EXTENDED_HAT_START),
        encoders: [None; KS_ENCODER_COUNT],
        clutch_left_axis: None,
        clutch_right_axis: None,
        clutch_combined_axis: None,
        clutch_left_button: None,
        clutch_right_button: None,
        clutch_mode_hint: KsClutchMode::Unknown,
        rotary_mode_hint: KsRotaryMode::Unknown,
        left_rotary_axis: None,
        right_rotary_axis: None,
        joystick_mode_hint: KsJoystickMode::DPad,
        joystick_hat: Some(KsByteSource::new(input_report::R5_V1_EXTENDED_HAT_START)),
    }
}

fn uses_live_r5_v1_extended_layout(report: &[u8]) -> bool {
    looks_like_live_r5_v1_extended_report(report)
}

fn wheelbase_ks_map_for_report(report: &[u8]) -> KsReportMap {
    if uses_live_r5_v1_extended_layout(report) {
        live_r5_v1_extended_wheelbase_ks_map()
    } else {
        default_wheelbase_ks_map()
    }
}

/// Moza protocol handler.
pub struct MozaProtocol {
    product_id: u16,
    model: MozaModel,
    is_v2: bool,
    init_state: AtomicU8,
    retry_count: AtomicU8,
    ffb_mode: FfbMode,
    high_torque_enabled: bool,
    max_retries: u8,
}

impl MozaProtocol {
    /// Create a new Moza protocol handler.
    pub fn new(product_id: u16) -> Self {
        Self::new_with_ffb_mode(product_id, default_ffb_mode())
    }

    /// Create a new Moza protocol handler with explicit FFB mode.
    pub fn new_with_ffb_mode(product_id: u16, ffb_mode: FfbMode) -> Self {
        Self::new_with_config(product_id, ffb_mode, default_high_torque_enabled())
    }

    /// Create a new Moza protocol handler with explicit FFB mode and high-torque gate.
    ///
    /// High torque is off by default; pass `true` only when the user has explicitly
    /// opted in (e.g. via `OPENRACING_MOZA_HIGH_TORQUE=1`).
    pub fn new_with_config(product_id: u16, ffb_mode: FfbMode, high_torque_enabled: bool) -> Self {
        let is_v2 = (product_id & 0x0010) != 0;
        let model = MozaModel::from_pid(product_id);

        debug!(
            "Created MozaProtocol for PID 0x{:04X}, model: {:?}, V2: {}",
            product_id, model, is_v2
        );

        Self {
            product_id,
            model,
            is_v2,
            init_state: AtomicU8::new(MOZA_INIT_STATE_UNINITIALIZED),
            retry_count: AtomicU8::new(0),
            ffb_mode,
            high_torque_enabled,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    /// Whether high-torque mode will be sent during initialization.
    pub fn is_high_torque_enabled(&self) -> bool {
        self.high_torque_enabled
    }

    /// Whether the device is ready to receive FFB output.
    ///
    /// Returns `true` only when the handshake has completed successfully.
    /// Callers must check this before writing torque data.
    pub fn is_ffb_ready(&self) -> bool {
        self.init_state() == MozaInitState::Ready
    }

    /// Current number of handshake failures since last reset.
    pub fn retry_count(&self) -> u8 {
        self.retry_count.load(Ordering::Acquire)
    }

    /// Whether another `initialize_device` call is permitted under the default policy.
    pub fn can_retry(&self) -> bool {
        let state = self.init_state();
        state == MozaInitState::Failed
            && self.retry_count.load(Ordering::Acquire) < self.max_retries
    }

    /// Reset the protocol state machine to `Uninitialized`.
    ///
    /// Call this on device disconnect so the next `initialize_device` starts fresh.
    pub fn reset_to_uninitialized(&self) {
        self.retry_count.store(0, Ordering::Release);
        self.init_state
            .store(MOZA_INIT_STATE_UNINITIALIZED, Ordering::Release);
        debug!("Moza {:?} protocol reset to Uninitialized", self.model);
    }

    /// Get current protocol init state.
    pub fn init_state(&self) -> MozaInitState {
        MozaInitState::from_u8(self.init_state.load(Ordering::Acquire))
    }

    /// Get the product ID.
    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    /// Get the device model.
    pub fn model(&self) -> MozaModel {
        self.model
    }

    /// Selected mode for FFB initialization and reporting.
    pub fn ffb_mode(&self) -> FfbMode {
        self.ffb_mode
    }

    /// Get ES compatibility state for this wheelbase/product.
    pub fn es_compatibility(&self) -> crate::types::MozaEsCompatibility {
        es_compatibility(self.product_id)
    }

    /// Parse pedal axis data from a wheelbase input report.
    ///
    /// SR-P Lite pedals are typically connected to the wheelbase pedal port,
    /// so their axis values are carried in the wheelbase input report rather
    /// than a standalone USB pedal device.
    pub fn parse_aggregated_pedal_axes(&self, report: &[u8]) -> Option<MozaPedalAxesRaw> {
        let axes = parse_wheelbase_pedal_axes(report)?;

        Some(MozaPedalAxesRaw {
            throttle: axes.throttle,
            brake: axes.brake,
            clutch: axes.clutch,
            handbrake: axes.handbrake,
        })
    }

    /// Parse a wheelbase-style report into a lightweight, non-owning view.
    pub fn parse_wheelbase_report<'a>(&self, report: &'a [u8]) -> Option<RawWheelbaseReport<'a>> {
        parse_wheelbase_report(report)
    }

    /// Parse a full Moza input report into `MozaInputState`.
    pub fn parse_input_state(&self, report: &[u8]) -> Option<MozaInputState> {
        if report.first().copied() != Some(input_report::REPORT_ID) {
            return self.parse_standalone_peripheral_state(report);
        }

        let Some(parsed) = parse_wheelbase_input_report(report) else {
            return self.parse_standalone_peripheral_state(report);
        };
        let steering_u16 = parsed.steering;
        let throttle_u16 = parsed.pedals.throttle;
        let brake_u16 = parsed.pedals.brake;
        let clutch_u16 = parsed.pedals.clutch.unwrap_or(0);
        let handbrake_u16 = parsed.pedals.handbrake.unwrap_or(0);
        let buttons = parsed.buttons;
        let hat = parsed.hat;
        let funky = parsed.funky;
        let rotary = parsed.rotary;

        let ks_snapshot = if self.is_wheelbase() {
            let mut fallback_snapshot = KsReportSnapshot::from_common_controls(0, buttons, hat);
            fallback_snapshot.encoders[0] = i16::from(rotary[0]);
            fallback_snapshot.encoders[1] = i16::from(rotary[1]);

            if funky == rim_ids::KS || uses_live_r5_v1_extended_layout(report) {
                if let Some(mut mapped) = wheelbase_ks_map_for_report(report).parse(0, report) {
                    if !uses_live_r5_v1_extended_layout(report) {
                        // Base wheelbase rotary bytes are authoritative for the legacy path.
                        // Keep these values even when the KS map parser yields encoder data.
                        mapped.encoders[0] = fallback_snapshot.encoders[0];
                        mapped.encoders[1] = fallback_snapshot.encoders[1];
                    }
                    mapped
                } else {
                    fallback_snapshot
                }
            } else {
                fallback_snapshot
            }
        } else {
            KsReportSnapshot::default()
        };

        Some(MozaInputState {
            steering_u16,
            throttle_u16,
            brake_u16,
            clutch_u16,
            handbrake_u16,
            buttons,
            hat,
            funky,
            rotary,
            ks_snapshot,
            tick: 0,
        })
    }

    /// Enable high torque mode.
    pub fn enable_high_torque(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = [report_ids::HIGH_TORQUE, 0x00, 0x00, 0x00];
        writer.write_feature_report(&report)?;
        info!("Enabled high torque mode for Moza {:?}", self.model);
        Ok(())
    }

    /// Start input reports.
    pub fn start_input_reports(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = [report_ids::START_REPORTS, 0x00, 0x00, 0x00];
        writer.write_feature_report(&report)?;
        debug!("Started input reports for Moza {:?}", self.model);
        Ok(())
    }

    /// Set FFB mode.
    pub fn set_ffb_mode(
        &self,
        writer: &mut dyn DeviceWriter,
        mode: FfbMode,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = [report_ids::FFB_MODE, mode as u8, 0x00, 0x00];
        writer.write_feature_report(&report)?;
        debug!("Set FFB mode to {:?} for Moza {:?}", mode, self.model);
        Ok(())
    }

    /// Set rotation range in degrees.
    pub fn set_rotation_range(
        &self,
        writer: &mut dyn DeviceWriter,
        degrees: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let range_bytes = degrees.to_le_bytes();
        let report = [
            report_ids::ROTATION_RANGE,
            0x01, // Command: Set Range
            range_bytes[0],
            range_bytes[1],
        ];
        writer.write_feature_report(&report)?;
        debug!(
            "Set rotation range to {} degrees for Moza {:?}",
            degrees, self.model
        );
        Ok(())
    }

    fn is_output_capable(&self) -> bool {
        is_wheelbase_product(self.product_id)
    }

    fn try_enter_initialization(&self) -> bool {
        let mut state = self.init_state.load(Ordering::Acquire);
        loop {
            match state {
                MOZA_INIT_STATE_READY
                | MOZA_INIT_STATE_INITIALIZING
                | MOZA_INIT_STATE_PERMANENT_FAILURE => return false,
                _ => {
                    match self.init_state.compare_exchange(
                        state,
                        MOZA_INIT_STATE_INITIALIZING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => return true,
                        Err(observed) => state = observed,
                    }
                }
            }
        }
    }

    fn finalize_initialization(&self, success: bool) {
        if success {
            self.init_state
                .store(MOZA_INIT_STATE_READY, Ordering::Release);
            return;
        }

        let prev = self.retry_count.fetch_add(1, Ordering::AcqRel);
        let next_count = prev.saturating_add(1);
        let next_state = if next_count >= self.max_retries {
            warn!(
                "Moza {:?} handshake failed permanently after {} attempts",
                self.model, next_count
            );
            MOZA_INIT_STATE_PERMANENT_FAILURE
        } else {
            MOZA_INIT_STATE_FAILED
        };
        self.init_state.store(next_state, Ordering::Release);
    }

    fn is_standalone_handbrake(&self) -> bool {
        identify_device(self.product_id).category == MozaDeviceCategory::Handbrake
    }

    fn is_standalone_pedals(&self) -> bool {
        identify_device(self.product_id).category == MozaDeviceCategory::Pedals
    }

    fn is_wheelbase(&self) -> bool {
        identify_device(self.product_id).category == MozaDeviceCategory::Wheelbase
    }

    fn parse_standalone_peripheral_state(&self, report: &[u8]) -> Option<MozaInputState> {
        self.parse_standalone_pedal_state(report)
            .or_else(|| self.parse_standalone_handbrake_state(report))
    }

    fn parse_standalone_pedal_state(&self, report: &[u8]) -> Option<MozaInputState> {
        if !self.is_standalone_pedals() {
            return None;
        }

        let axes = parse_srp_usb_report_best_effort(report)?;
        let mut state = MozaInputState::empty(0);
        state.throttle_u16 = axes.throttle;
        state.brake_u16 = axes.brake.unwrap_or(0);
        Some(state)
    }

    fn parse_standalone_handbrake_state(&self, report: &[u8]) -> Option<MozaInputState> {
        if !self.is_standalone_handbrake() {
            return None;
        }

        let parsed = parse_hbp_usb_report_best_effort(report)?;

        let mut state = MozaInputState::empty(0);
        state.handbrake_u16 = parsed.handbrake;
        if let Some(buttons) = parsed.button_byte {
            state.buttons[0] = buttons;
        }

        Some(state)
    }

    /// Get encoder CPR based on model and hardware version.
    fn encoder_cpr(&self) -> u32 {
        if self.is_v2 {
            match self.model {
                MozaModel::R16 | MozaModel::R21 => 2097152, // 21-bit
                _ => 262144,                                // 18-bit
            }
        } else {
            32768 // 15-bit for V1
        }
    }
}

impl VendorProtocol for MozaProtocol {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_output_capable() {
            debug!(
                "Skipping initialization for non-wheelbase Moza product: pid=0x{:04X}, model={:?}",
                self.product_id, self.model
            );
            return Ok(());
        }

        if !self.try_enter_initialization() {
            debug!(
                "Skipping Moza initialize while in-flight or already initialized: pid=0x{:04X}",
                self.product_id
            );
            return Ok(());
        }

        // Device initialization sequence (HID feature reports):
        //   1. [optional] Enable high-torque mode (report 0x02) — unlocks full amplitude
        //   2. Start input reports (report 0x03) — device begins sending input data
        //   3. Set FFB mode (report 0x11) — Standard (0x00) or Direct (0x02)
        //
        // This sequence is confirmed by USB capture analysis. The serial/CDC ACM
        // configuration interface (115200 baud, 0x7E framing) is separate and not
        // needed for FFB operation.

        info!(
            "Initializing Moza {:?} (V{})",
            self.model,
            if self.is_v2 { 2 } else { 1 }
        );

        let mut success = true;

        match self.es_compatibility() {
            crate::types::MozaEsCompatibility::UnsupportedHardwareRevision => warn!(
                "Moza PID 0x{:04X} is R9 V1; ES wheel compatibility is not supported",
                self.product_id
            ),
            crate::types::MozaEsCompatibility::UnknownWheelbase => debug!(
                "Moza PID 0x{:04X} ES compatibility is not capture-validated",
                self.product_id
            ),
            crate::types::MozaEsCompatibility::Supported
            | crate::types::MozaEsCompatibility::NotWheelbase => {}
        }

        // Step 1: Optionally enable high torque mode (unlocks full FFB amplitude).
        // Off by default — requires explicit opt-in via config or environment variable.
        if self.high_torque_enabled {
            if let Err(e) = self.enable_high_torque(writer) {
                warn!("Failed to enable high torque: {}", e);
                success = false;
            }
        } else {
            debug!(
                "High torque not enabled for Moza {:?} (use OPENRACING_MOZA_HIGH_TORQUE=1 to enable)",
                self.model
            );
        }

        // Step 2: Start input reports
        if let Err(e) = self.start_input_reports(writer) {
            warn!("Failed to start input reports: {}", e);
            success = false;
        }

        // Step 3: Set FFB to the configured mode.
        if let Err(e) = self.set_ffb_mode(writer, self.ffb_mode) {
            warn!("Failed to set FFB mode: {}", e);
            success = false;
        }

        self.finalize_initialization(success);
        if success {
            info!("Moza {:?} initialization complete", self.model);
        } else {
            warn!(
                "Moza {:?} initialization incomplete; device not ready for native output",
                self.model
            );
        }
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        const MAX_REPORT_BYTES: usize = 64;

        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "feature report payload too large: {} > {} bytes",
                data.len() + 1,
                MAX_REPORT_BYTES
            )
            .into());
        }

        let mut report = [0u8; MAX_REPORT_BYTES];
        report[0] = report_id;
        let end = data.len() + 1;
        report[1..end].copy_from_slice(data);
        writer.write_feature_report(&report[..end])?;
        Ok(())
    }

    /// Get FFB configuration including quirks.
    ///
    /// `fix_conditional_direction` matches the Linux kernel
    /// `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION` applied to all Moza
    /// wheelbases in `hid-universal-pidff.c`.
    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: true,
            uses_vendor_usage_page: true,
            required_b_interval: Some(1), // 1ms for 1kHz
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: self.encoder_cpr(),
        }
    }

    fn is_v2_hardware(&self) -> bool {
        self.is_v2
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.is_output_capable() {
            Some(report_ids::DIRECT_TORQUE)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.is_output_capable() {
            Some(REPORT_LEN)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{product_ids, rim_ids};

    #[test]
    fn parse_input_state_maps_standalone_srp_axes() -> Result<(), Box<dyn std::error::Error>> {
        let protocol = MozaProtocol::new(product_ids::SR_P_PEDALS);
        let report = [0x01u8, 0x34, 0x12, 0x78, 0x56];

        let state = protocol
            .parse_input_state(&report)
            .ok_or("expected standalone SR-P parse")?;

        assert_eq!(state.steering_u16, 0);
        assert_eq!(state.throttle_u16, 0x1234);
        assert_eq!(state.brake_u16, 0x5678);
        assert_eq!(state.clutch_u16, 0);
        assert_eq!(state.handbrake_u16, 0);
        Ok(())
    }

    #[test]
    fn parse_input_state_rejects_short_standalone_srp_report() {
        let protocol = MozaProtocol::new(product_ids::SR_P_PEDALS);
        let report = [0x01u8, 0x34, 0x12];

        assert_eq!(protocol.parse_input_state(&report), None);
    }

    #[test]
    fn parse_input_state_accepts_hbp_report_id_one_layout() -> Result<(), Box<dyn std::error::Error>>
    {
        let protocol = MozaProtocol::new(product_ids::HBP_HANDBRAKE);
        let report = [0x01u8, 0x34, 0x12, 0xA5];

        let state = protocol
            .parse_input_state(&report)
            .ok_or("expected standalone HBP parse")?;

        assert_eq!(state.handbrake_u16, 0x1234);
        assert_eq!(state.buttons[0], 0xA5);
        assert_eq!(state.throttle_u16, 0);
        assert_eq!(state.brake_u16, 0);
        Ok(())
    }

    #[test]
    fn parse_input_state_wheelbase_rotary_bytes_stay_stable_without_ks_rim()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = MozaProtocol::new(product_ids::R9_V2);
        let report = [
            input_report::REPORT_ID,
            0x00,
            0x80, // steering
            0x00,
            0x00, // throttle
            0x00,
            0x00, // brake
            0x00,
            0x00, // clutch
            0x00,
            0x00, // handbrake
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,        // buttons
            0x00,        // hat
            rim_ids::ES, // non-KS rim marker
            0x2A,        // rotary 0
            0x7F,        // rotary 1
        ];

        let parsed = protocol
            .parse_input_state(&report)
            .ok_or("expected wheelbase parse")?;

        assert_eq!(parsed.ks_snapshot.encoders[0], 0x2A);
        assert_eq!(parsed.ks_snapshot.encoders[1], 0x7F);
        Ok(())
    }

    #[test]
    fn parse_input_state_wheelbase_rotary_bytes_stay_stable_with_ks_rim()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = MozaProtocol::new(product_ids::R9_V2);
        let report = [
            input_report::REPORT_ID,
            0x00,
            0x80, // steering
            0x00,
            0x00, // throttle
            0x00,
            0x00, // brake
            0x00,
            0x00, // clutch
            0x00,
            0x00, // handbrake
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,        // buttons
            0x00,        // hat
            rim_ids::KS, // KS rim marker
            0x19,        // rotary 0
            0x64,        // rotary 1
        ];

        let parsed = protocol
            .parse_input_state(&report)
            .ok_or("expected wheelbase parse")?;

        assert_eq!(parsed.ks_snapshot.encoders[0], 0x19);
        assert_eq!(parsed.ks_snapshot.encoders[1], 0x64);
        Ok(())
    }

    #[test]
    fn parse_input_state_maps_live_r5_v1_extended_ks_controls()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = MozaProtocol::new(product_ids::R5_V1);
        let mut report = [0u8; input_report::R5_V1_EXTENDED_REPORT_LEN];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x7A37u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_KS_AXIS0_START
            ..input_report::R5_V1_EXTENDED_KS_AXIS0_START + 2]
            .copy_from_slice(&0x4321u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS0_START
            ..input_report::R5_V1_EXTENDED_AXIS0_START + 2]
            .copy_from_slice(&0x8000u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS1_START
            ..input_report::R5_V1_EXTENDED_AXIS1_START + 2]
            .copy_from_slice(&0x8001u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_AXIS2_START
            ..input_report::R5_V1_EXTENDED_AXIS2_START + 2]
            .copy_from_slice(&0x8000u16.to_le_bytes());
        report[input_report::R5_V1_EXTENDED_BUTTONS_START] = 0x08;
        report[input_report::R5_V1_EXTENDED_BUTTONS_START + 1] = 0x04;
        report[input_report::R5_V1_EXTENDED_BUTTONS_START + 10] = 0x80;
        report[input_report::R5_V1_EXTENDED_HAT_START] = 0x03;

        let parsed = protocol
            .parse_input_state(&report)
            .ok_or("expected live R5 V1 extended wheelbase parse")?;

        assert_eq!(parsed.throttle_u16, 0);
        assert_eq!(parsed.brake_u16, 0);
        assert_eq!(parsed.clutch_u16, 0);
        assert_eq!(parsed.handbrake_u16, 0);
        assert_eq!(parsed.buttons[0], 0x08);
        assert_eq!(parsed.buttons[1], 0x04);
        assert_eq!(parsed.buttons[10], 0x80);
        assert_eq!(parsed.hat, 0x03);
        assert_eq!(parsed.funky, 0x00);
        assert_eq!(parsed.rotary, [0x00, 0x00]);
        assert_eq!(parsed.ks_snapshot.buttons[0], 0x08);
        assert_eq!(parsed.ks_snapshot.buttons[1], 0x04);
        assert_eq!(parsed.ks_snapshot.buttons[10], 0x80);
        assert_eq!(parsed.ks_snapshot.hat, 0x03);
        assert_eq!(parsed.ks_snapshot.clutch_combined, None);
        assert_eq!(parsed.ks_snapshot.clutch_mode, KsClutchMode::Unknown);
        assert_eq!(parsed.ks_snapshot.rotary_mode, KsRotaryMode::Unknown);
        Ok(())
    }

    #[test]
    fn test_is_output_capable() {
        // Wheelbases should be output capable
        assert!(MozaProtocol::new(product_ids::R9_V2).is_output_capable());
        assert!(MozaProtocol::new(product_ids::R5_V1).is_output_capable());

        // Peripherals should NOT be output capable
        assert!(!MozaProtocol::new(product_ids::SR_P_PEDALS).is_output_capable());
        assert!(!MozaProtocol::new(product_ids::HBP_HANDBRAKE).is_output_capable());
    }
}
