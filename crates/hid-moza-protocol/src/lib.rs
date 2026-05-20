//! Moza Racing HID protocol: report parsing, handshake frames, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.

#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

/// [ADR-0007]: Multi-Vendor HID Protocol Architecture
/// This crate implements the Moza Racing protocol following the "SRP Microcrate" pattern.
pub mod direct;
pub mod ids;
pub mod protocol;
pub mod report;
pub mod rt_types;
#[doc(hidden)]
pub mod serial;
pub mod signature;
pub mod standalone;
pub mod types;
pub mod writer;

// Flat re-exports so callers can use `racing_wheel_hid_moza_protocol::Foo`.
pub use direct::{MozaDirectTorqueEncoder, REPORT_LEN};
pub use ids::{MOZA_VENDOR_ID, product_ids, rim_ids};
pub use protocol::{
    DEFAULT_MAX_RETRIES, FfbMode, MozaInitState, MozaProtocol, MozaRetryPolicy, default_ffb_mode,
    default_high_torque_enabled, effective_ffb_mode, effective_high_torque_opt_in,
    signature_is_trusted,
};
pub use racing_wheel_hbp::{
    HbpHandbrakeSample, HbpHandbrakeSampleRaw, parse_hbp_usb_report_best_effort,
};
pub use racing_wheel_srp::{SrpPedalAxes, SrpPedalAxesRaw, parse_srp_usb_report_best_effort};
pub use report::{
    RawWheelbaseReport, WheelbaseInputRaw, WheelbasePedalAxesRaw, hbp_report, input_report,
    parse_axis, parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report,
    report_ids,
};
pub use rt_types::{TorqueEncoder, TorqueQ8_8};
pub use signature::{DeviceSignature, SignatureVerdict, verify_signature};
pub use standalone::{StandaloneAxes, StandaloneParseResult, parse_hbp_report, parse_srp_report};
pub use types::{
    ES_BUTTON_COUNT, ES_LED_COUNT, MozaDeviceCategory, MozaDeviceIdentity, MozaEsCompatibility,
    MozaEsJoystickMode, MozaHatDirection, MozaInputState, MozaModel, MozaPedalAxes,
    MozaPedalAxesRaw, MozaTopologyHint, es_compatibility, identify_device, is_wheelbase_product,
};
pub use writer::{DeviceWriter, FfbConfig, VendorProtocol};

// KS control-surface types re-exported so callers don't need a direct
// `racing-wheel-ks` dependency when inspecting `MozaInputState::ks_snapshot`.
pub use racing_wheel_ks::{
    KS_BUTTON_BYTES, KS_ENCODER_COUNT, KsAxisSource, KsBitSource, KsByteSource, KsClutchMode,
    KsJoystickMode, KsReportMap, KsReportSnapshot, KsRotaryMode,
};
