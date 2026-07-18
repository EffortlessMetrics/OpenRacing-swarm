//! Device types for racing wheel hardware abstraction
//!
//! This crate provides device type definitions for racing wheel hardware,
//! abstracted from specific vendor implementations.

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

use serde::{Deserialize, Serialize};

pub mod projection;
pub use projection::ControlProjector;

/// Telemetry data from device
#[derive(Debug, Clone)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

/// Number of button bits addressable by [`DeviceInputs`].
///
/// The backing `buttons` buffer is `[u8; 16]` = 128 bits, so indices
/// `0..=127` are valid. Indices `>= 128` are out of range.
pub const MAX_BUTTONS: usize = 128;

/// Generic non-RT control-surface snapshot used by input pipeline and diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceInputs {
    pub tick: u32,
    /// Raw button state bits (up to [`MAX_BUTTONS`] = 128 buttons, 1 bit each).
    pub buttons: [u8; 16],
    pub hat: u8,
    pub steering: Option<u16>,
    pub throttle: Option<u16>,
    pub brake: Option<u16>,
    pub clutch_left: Option<u16>,
    pub clutch_right: Option<u16>,
    pub clutch_combined: Option<u16>,
    pub clutch_left_button: Option<bool>,
    pub clutch_right_button: Option<bool>,
    pub handbrake: Option<u16>,
    pub rotaries: [i16; 8],
}

impl DeviceInputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_buttons(mut self, buttons: [u8; 16]) -> Self {
        self.buttons = buttons;
        self
    }

    pub fn with_steering(mut self, steering: u16) -> Self {
        self.steering = Some(steering);
        self
    }

    pub fn with_pedals(mut self, throttle: u16, brake: u16, clutch: u16) -> Self {
        self.throttle = Some(throttle);
        self.brake = Some(brake);
        self.clutch_combined = Some(clutch);
        self
    }

    pub fn with_handbrake(mut self, handbrake: u16) -> Self {
        self.handbrake = Some(handbrake);
        self
    }

    pub fn with_hat(mut self, hat: u8) -> Self {
        self.hat = hat;
        self
    }

    pub fn with_rotaries(mut self, rotaries: [i16; 8]) -> Self {
        self.rotaries = rotaries;
        self
    }

    /// Return the pressed state of button `index`.
    ///
    /// Valid indices are `0..=127` ([`MAX_BUTTONS`]); indices `>= 128`
    /// return `false` (no-op) rather than panicking.
    pub fn button(&self, index: usize) -> bool {
        if index < MAX_BUTTONS {
            self.buttons[index / 8] & (1 << (index % 8)) != 0
        } else {
            false
        }
    }

    /// Set the pressed state of button `index`.
    ///
    /// Valid indices are `0..=127` ([`MAX_BUTTONS`]); indices `>= 128`
    /// are ignored (no-op) rather than panicking.
    pub fn set_button(&mut self, index: usize, value: bool) {
        if index < MAX_BUTTONS {
            if value {
                self.buttons[index / 8] |= 1 << (index % 8);
            } else {
                self.buttons[index / 8] &= !(1 << (index % 8));
            }
        }
    }

    pub fn rotary(&self, index: usize) -> i16 {
        self.rotaries.get(index).copied().unwrap_or(0)
    }

    pub fn hat_direction(&self) -> HatDirection {
        match self.hat {
            0 => HatDirection::Up,
            1 => HatDirection::UpRight,
            2 => HatDirection::Right,
            3 => HatDirection::DownRight,
            4 => HatDirection::Down,
            5 => HatDirection::DownLeft,
            6 => HatDirection::Left,
            7 => HatDirection::UpLeft,
            _ => HatDirection::Neutral,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HatDirection {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    #[default]
    Neutral,
}

// ---------------------------------------------------------------------------
// Control-stream domain contract (transport-neutral).
//
// These types describe a generic, vendor-neutral, observe-only control surface
// stream. They are pure data with `serde` support only: no protobuf/tonic, no
// gRPC, no HID access, and no application- or product-specific control actions.
// Snapshot/edge projection (see the projection work item) and IPC transport are
// intentionally *not* modeled here. See the external-control-stream plan.
// ---------------------------------------------------------------------------

/// Category of a control surface element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlKind {
    /// A momentary or latching button (0/1).
    Button,
    /// A hat / D-pad direction switch.
    Hat,
    /// A relative rotary encoder (deltas / accumulating position).
    Encoder,
    /// An absolute analog axis.
    Axis,
}

/// Provenance of a control's optional semantic identity.
///
/// A physical control always has a stable raw identity; a *semantic* identity
/// (a meaning assigned by mapping/profile work) is optional and carries an
/// explicit trust level so consumers never treat an unverified guess as a
/// validated role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticStatus {
    /// No semantic meaning; only the raw control identity is known.
    Raw,
    /// A proposed semantic identity that has not been verified by evidence.
    Candidate,
    /// A semantic identity confirmed through the lane's evidence process.
    Validated,
}

/// Stable, always-available raw identifier for a control on a device.
///
/// The value is opaque and stable for a given device/mapping version. Use the
/// [`ControlDescriptor`] constructors for the canonical encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RawControlId(pub u32);

impl RawControlId {
    // Canonical raw-id encoding: high byte selects the control family, low bits
    // carry the per-family index. Keeps ids stable and collision-free across
    // buttons/hat/encoders/axes without needing an allocation.
    const BUTTON_BASE: u32 = 0x0000_0000;
    const HAT_BASE: u32 = 0x0100_0000;
    const ENCODER_BASE: u32 = 0x0200_0000;
    const AXIS_BASE: u32 = 0x0300_0000;

    /// Raw id for button `index` (`0..=127`).
    #[must_use]
    pub const fn button(index: u8) -> Self {
        Self(Self::BUTTON_BASE | index as u32)
    }

    /// Raw id for the hat switch.
    #[must_use]
    pub const fn hat() -> Self {
        Self(Self::HAT_BASE)
    }

    /// Raw id for encoder `index`.
    #[must_use]
    pub const fn encoder(index: u8) -> Self {
        Self(Self::ENCODER_BASE | index as u32)
    }

    /// Raw id for absolute axis `index`.
    #[must_use]
    pub const fn axis(index: u8) -> Self {
        Self(Self::AXIS_BASE | index as u32)
    }
}

/// An optional, provenance-tagged semantic identity for a control.
///
/// The label is a generic mapping token (e.g. `"paddle_left"`), never an
/// application/product action. Consumers must honour [`SemanticStatus`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticControlId {
    /// Generic, vendor-neutral semantic label.
    pub label: String,
    /// Trust level of this semantic identity.
    pub status: SemanticStatus,
}

/// Description of a single control on a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDescriptor {
    /// Always-available stable raw identity.
    pub raw_id: RawControlId,
    /// Category of the control.
    pub kind: ControlKind,
    /// Optional semantic identity with explicit provenance.
    pub semantic: Option<SemanticControlId>,
}

impl ControlDescriptor {
    /// Raw-only button descriptor for `index` (`0..=127`).
    #[must_use]
    pub const fn button(index: u8) -> Self {
        Self {
            raw_id: RawControlId::button(index),
            kind: ControlKind::Button,
            semantic: None,
        }
    }

    /// Raw-only hat descriptor.
    #[must_use]
    pub const fn hat() -> Self {
        Self {
            raw_id: RawControlId::hat(),
            kind: ControlKind::Hat,
            semantic: None,
        }
    }

    /// Raw-only encoder descriptor for `index`.
    #[must_use]
    pub const fn encoder(index: u8) -> Self {
        Self {
            raw_id: RawControlId::encoder(index),
            kind: ControlKind::Encoder,
            semantic: None,
        }
    }

    /// Raw-only axis descriptor for `index`.
    #[must_use]
    pub const fn axis(index: u8) -> Self {
        Self {
            raw_id: RawControlId::axis(index),
            kind: ControlKind::Axis,
            semantic: None,
        }
    }

    /// Attach a provenance-tagged semantic identity to this descriptor.
    #[must_use]
    pub fn with_semantic(mut self, label: impl Into<String>, status: SemanticStatus) -> Self {
        self.semantic = Some(SemanticControlId {
            label: label.into(),
            status,
        });
        self
    }
}

/// Stable identity of the physical device backing a control surface.
///
/// Fields are vendor-neutral and contain no application/product identity.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// USB vendor id (0 when unknown).
    pub vendor_id: u16,
    /// USB product id (0 when unknown).
    pub product_id: u16,
    /// Firmware/OS-reported serial, when available and safe to expose.
    pub serial: Option<String>,
    /// Stable logical instance id assigned by the owner; survives reconnect
    /// where the underlying device can be re-identified.
    pub instance: u64,
}

/// Descriptor for a whole control surface (a device's exposed controls).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlSurfaceDescriptor {
    /// Stable physical device identity.
    pub device: DeviceIdentity,
    /// Version of the control mapping/contract this descriptor set reflects.
    pub mapping_version: u32,
    /// All controls exposed by the surface.
    pub controls: Vec<ControlDescriptor>,
}

/// A concrete value held by a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlValue {
    /// Button pressed state.
    Button(bool),
    /// Hat direction.
    Hat(HatDirection),
    /// Absolute, monotonically accumulating encoder position. Consumers derive
    /// deltas from successive positions so no tick is lost to snapshot polling.
    Encoder(i32),
    /// Absolute analog axis value.
    Axis(u16),
}

/// A snapshot of one control's current value (part of an initial baseline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlState {
    /// Which control this value belongs to.
    pub raw_id: RawControlId,
    /// The control's current value.
    pub value: ControlValue,
}

/// An actionable change to a single control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEvent {
    /// Which control changed.
    pub raw_id: RawControlId,
    /// The control's value after the change.
    pub value: ControlValue,
    /// For encoders, the signed delta applied to reach `value`. `None` for
    /// non-encoder controls. Enables lossless rotary accounting downstream.
    pub delta: Option<i32>,
}

/// Why a stream reset/gap was emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetReason {
    /// First observation; a fresh baseline is being established.
    Initial,
    /// The producer's input epoch changed (e.g. re-enumeration).
    EpochChange,
    /// The device physically disconnected.
    Disconnect,
    /// The device reconnected after a disconnect.
    Reconnect,
    /// A subscriber lagged/overflowed and must resynchronise from a new baseline.
    Overflow,
}

/// Ordering metadata common to every stream item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StreamMeta {
    /// Monotonically increasing sequence number within an input epoch.
    ///
    /// Named `seq` (not `sequence`) to stay clear of the repository's removed
    /// telemetry `sequence` field and its deprecated-token gate.
    pub seq: u64,
    /// Monotonic source-clock timestamp in nanoseconds, for ordering.
    pub timestamp_ns: u64,
    /// Epoch identifier; increments on every reset/reconnect.
    pub epoch: u32,
}

/// A single item delivered over the control stream.
///
/// Initial baselines ([`ControlStreamItem::InitialSnapshot`]) are structurally
/// distinct from actionable [`ControlStreamItem::Event`]s, so a consumer can
/// never mistake a baseline for a fresh button press. Every item carries
/// [`StreamMeta`] for deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlStreamItem {
    /// Surface descriptor announcement.
    Descriptor {
        meta: StreamMeta,
        surface: ControlSurfaceDescriptor,
    },
    /// Initial, non-actionable baseline snapshot of current control values.
    InitialSnapshot {
        meta: StreamMeta,
        states: Vec<ControlState>,
    },
    /// An ordered, actionable control change.
    Event {
        meta: StreamMeta,
        event: ControlEvent,
    },
    /// A reset / gap / disconnect notification.
    Reset {
        meta: StreamMeta,
        reason: ResetReason,
    },
}

impl ControlStreamItem {
    /// Ordering metadata for this item.
    #[must_use]
    pub fn meta(&self) -> &StreamMeta {
        match self {
            Self::Descriptor { meta, .. }
            | Self::InitialSnapshot { meta, .. }
            | Self::Event { meta, .. }
            | Self::Reset { meta, .. } => meta,
        }
    }

    /// Sequence number within the current epoch.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.meta().seq
    }

    /// Whether this item represents an actionable control change. Descriptors,
    /// initial baselines, and resets are **not** actionable.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        matches!(self, Self::Event { .. })
    }
}

#[cfg(feature = "proptest")]
mod proptest_shrinks {
    use super::*;
    use proptest::prelude::*;

    impl Arbitrary for DeviceInputs {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            (
                (
                    any::<u32>(),
                    any::<[u8; 16]>(),
                    any::<u8>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                ),
                (
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<bool>>(),
                    any::<Option<bool>>(),
                    any::<Option<u16>>(),
                    any::<[i16; 8]>(),
                ),
            )
                .prop_map(
                    |(
                        (tick, buttons, hat, steering, throttle, brake),
                        (
                            clutch_left,
                            clutch_right,
                            clutch_combined,
                            clutch_left_button,
                            clutch_right_button,
                            handbrake,
                            rotaries,
                        ),
                    )| {
                        Self {
                            tick,
                            buttons,
                            hat,
                            steering,
                            throttle,
                            brake,
                            clutch_left,
                            clutch_right,
                            clutch_combined,
                            clutch_left_button,
                            clutch_right_button,
                            handbrake,
                            rotaries,
                        }
                    },
                )
                .boxed()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_inputs_default() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.tick, 0);
        assert_eq!(inputs.buttons, [0u8; 16]);
        assert_eq!(inputs.hat, 0);
        assert!(inputs.steering.is_none());
    }

    #[test]
    fn test_device_inputs_builder() {
        let inputs = DeviceInputs::new()
            .with_buttons([
                0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ])
            .with_steering(32768)
            .with_pedals(1024, 2048, 512)
            .with_handbrake(0);

        assert!(inputs.button(0));
        assert!(inputs.button(1));
        assert!(inputs.button(2));
        assert!(inputs.button(3));
        assert!(inputs.button(4));
        assert!(inputs.button(5));
        assert!(inputs.button(6));
        assert!(inputs.button(7));
        assert_eq!(inputs.steering, Some(32768));
        assert_eq!(inputs.throttle, Some(1024));
        assert_eq!(inputs.brake, Some(2048));
        assert_eq!(inputs.clutch_combined, Some(512));
    }

    #[test]
    fn test_button_access() {
        let mut inputs = DeviceInputs::default();

        inputs.set_button(0, true);
        assert!(inputs.button(0));

        inputs.set_button(7, true);
        assert!(inputs.button(7));

        inputs.set_button(0, false);
        assert!(!inputs.button(0));
        assert!(inputs.button(7));

        inputs.set_button(15, true);
        assert!(inputs.button(15));
    }

    #[test]
    fn test_button_out_of_bounds_returns_false() {
        let inputs = DeviceInputs::default();
        // 128 and above are out of range (buffer holds exactly 128 bits).
        assert!(!inputs.button(MAX_BUTTONS));
        assert!(!inputs.button(128));
        assert!(!inputs.button(200));
        assert!(!inputs.button(usize::MAX));
    }

    #[test]
    fn test_set_button_out_of_bounds_is_noop() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(128, true); // out of range: no-op, must not panic
        inputs.set_button(200, true); // out of range: no-op, must not panic
        inputs.set_button(usize::MAX, true); // no-op, must not panic
        // No in-range bit was touched.
        for i in 0..MAX_BUTTONS {
            assert!(!inputs.button(i));
        }
        // The backing buffer is untouched.
        assert_eq!(inputs.buttons, [0u8; 16]);
    }

    #[test]
    fn test_button_full_128_range() {
        // Every index in 0..=127 must be individually settable and clearable.
        let mut inputs = DeviceInputs::default();
        for i in 0..MAX_BUTTONS {
            assert!(!inputs.button(i), "button {i} should start unset");
            inputs.set_button(i, true);
            assert!(inputs.button(i), "button {i} should be set");
            inputs.set_button(i, false);
            assert!(!inputs.button(i), "button {i} should be cleared");
        }
    }

    #[test]
    fn test_button_boundary_indices() {
        // Boundary indices called out by the domain contract: 0, 7, 8, 15, 16,
        // 31, 63, 127 are in range; 128 is not.
        let mut inputs = DeviceInputs::default();
        for &i in &[0usize, 7, 8, 15, 16, 31, 63, 127] {
            inputs.set_button(i, true);
            assert!(inputs.button(i), "in-range boundary button {i} must set");
        }
        // Setting high buttons must not disturb low ones and vice versa.
        assert!(inputs.button(0));
        assert!(inputs.button(127));
        // 128 stays out of range.
        inputs.set_button(128, true);
        assert!(!inputs.button(128));
    }

    #[test]
    fn test_button_index_independence() {
        // Setting one high button must not flip any other bit.
        let mut inputs = DeviceInputs::default();
        inputs.set_button(64, true);
        for i in 0..MAX_BUTTONS {
            assert_eq!(inputs.button(i), i == 64, "only button 64 should be set");
        }
    }

    #[test]
    fn test_button_toggle() {
        let mut inputs = DeviceInputs::default();
        inputs.set_button(5, true);
        assert!(inputs.button(5));
        inputs.set_button(5, false);
        assert!(!inputs.button(5));
        inputs.set_button(5, true);
        assert!(inputs.button(5));
    }

    #[test]
    fn test_rotary_access() {
        let inputs = DeviceInputs::new().with_rotaries([1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(inputs.rotary(0), 1);
        assert_eq!(inputs.rotary(7), 8);
        assert_eq!(inputs.rotary(8), 0);
    }

    #[test]
    fn test_rotary_out_of_bounds_returns_zero() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.rotary(8), 0);
        assert_eq!(inputs.rotary(100), 0);
    }

    #[test]
    fn test_hat_direction() {
        let mut inputs = DeviceInputs::default();

        for dir in 0..8 {
            inputs.hat = dir;
            assert_ne!(inputs.hat_direction(), HatDirection::Neutral);
        }

        inputs.hat = 0xFF;
        assert_eq!(inputs.hat_direction(), HatDirection::Neutral);
    }

    #[test]
    fn test_hat_direction_all_values() {
        let expected = [
            (0, HatDirection::Up),
            (1, HatDirection::UpRight),
            (2, HatDirection::Right),
            (3, HatDirection::DownRight),
            (4, HatDirection::Down),
            (5, HatDirection::DownLeft),
            (6, HatDirection::Left),
            (7, HatDirection::UpLeft),
        ];
        let mut inputs = DeviceInputs::default();
        for (val, dir) in expected {
            inputs.hat = val;
            assert_eq!(
                inputs.hat_direction(),
                dir,
                "hat value {} should map to {:?}",
                val,
                dir
            );
        }
    }

    #[test]
    fn test_hat_boundary_values_neutral() {
        let mut inputs = DeviceInputs::default();
        // Values 8 and above should all be neutral
        for val in [8, 9, 15, 128, 255] {
            inputs.hat = val;
            assert_eq!(inputs.hat_direction(), HatDirection::Neutral);
        }
    }

    #[test]
    fn test_clutch_pedal_separation() {
        let inputs = DeviceInputs {
            clutch_left: Some(100),
            clutch_right: Some(200),
            clutch_combined: Some(150),
            ..Default::default()
        };

        assert_eq!(inputs.clutch_left, Some(100));
        assert_eq!(inputs.clutch_right, Some(200));
        assert_eq!(inputs.clutch_combined, Some(150));
    }

    #[test]
    fn test_telemetry_data() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 10.0,
            temperature_c: 50,
            fault_flags: 0,
            hands_on: true,
        };

        assert_eq!(telemetry.wheel_angle_deg, 45.0);
        assert_eq!(telemetry.temperature_c, 50);
        assert!(telemetry.hands_on);
    }

    #[test]
    fn test_new_equals_default() {
        let new_inputs = DeviceInputs::new();
        let default_inputs = DeviceInputs::default();
        assert_eq!(new_inputs.tick, default_inputs.tick);
        assert_eq!(new_inputs.buttons, default_inputs.buttons);
        assert_eq!(new_inputs.hat, default_inputs.hat);
        assert_eq!(new_inputs.steering, default_inputs.steering);
    }

    #[test]
    fn test_with_hat_builder() {
        let inputs = DeviceInputs::new().with_hat(4);
        assert_eq!(inputs.hat, 4);
        assert_eq!(inputs.hat_direction(), HatDirection::Down);
    }

    #[test]
    fn test_clutch_button_fields() {
        let inputs = DeviceInputs {
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            ..Default::default()
        };
        assert_eq!(inputs.clutch_left_button, Some(true));
        assert_eq!(inputs.clutch_right_button, Some(false));
    }

    #[test]
    fn test_telemetry_fault_flags() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0b1010_0101,
            hands_on: false,
        };
        assert_eq!(telemetry.fault_flags, 0b1010_0101);
        assert!(!telemetry.hands_on);
    }

    // --- Control-stream domain contract ---------------------------------

    #[test]
    fn test_raw_control_id_families_are_disjoint() {
        // Every family must occupy a distinct id space so ids never collide.
        let button = RawControlId::button(0).0;
        let hat = RawControlId::hat().0;
        let encoder = RawControlId::encoder(0).0;
        let axis = RawControlId::axis(0).0;
        assert_ne!(button, hat);
        assert_ne!(hat, encoder);
        assert_ne!(encoder, axis);
        // Buttons 0..=127 stay within their family and below the hat base.
        assert!(RawControlId::button(127).0 < hat);
        assert_eq!(RawControlId::button(5), RawControlId::button(5));
        assert_ne!(RawControlId::button(5), RawControlId::button(6));
    }

    #[test]
    fn test_control_descriptor_constructors() {
        assert_eq!(ControlDescriptor::button(3).kind, ControlKind::Button);
        assert_eq!(ControlDescriptor::hat().kind, ControlKind::Hat);
        assert_eq!(ControlDescriptor::encoder(1).kind, ControlKind::Encoder);
        assert_eq!(ControlDescriptor::axis(0).kind, ControlKind::Axis);
        // Raw-only descriptors carry no semantic identity.
        assert!(ControlDescriptor::button(3).semantic.is_none());
    }

    #[test]
    fn test_semantic_provenance_is_explicit() {
        let candidate =
            ControlDescriptor::button(10).with_semantic("paddle_left", SemanticStatus::Candidate);
        // A candidate identity carries its label and an explicit, non-validated
        // provenance so consumers never treat a guess as a confirmed role.
        assert_eq!(
            candidate.semantic,
            Some(SemanticControlId {
                label: "paddle_left".to_string(),
                status: SemanticStatus::Candidate,
            })
        );
    }

    #[test]
    fn test_initial_snapshot_is_not_actionable() {
        let meta = StreamMeta {
            seq: 1,
            timestamp_ns: 100,
            epoch: 0,
        };
        let snapshot = ControlStreamItem::InitialSnapshot {
            meta,
            states: vec![ControlState {
                raw_id: RawControlId::button(0),
                value: ControlValue::Button(true),
            }],
        };
        // A baseline that shows a button as pressed must NOT be actionable.
        assert!(!snapshot.is_actionable());

        let event = ControlStreamItem::Event {
            meta,
            event: ControlEvent {
                raw_id: RawControlId::button(0),
                value: ControlValue::Button(true),
                delta: None,
            },
        };
        assert!(event.is_actionable());
    }

    #[test]
    fn test_stream_item_meta_accessors() {
        let meta = StreamMeta {
            seq: 42,
            timestamp_ns: 9_000,
            epoch: 3,
        };
        let reset = ControlStreamItem::Reset {
            meta,
            reason: ResetReason::Overflow,
        };
        assert_eq!(reset.seq(), 42);
        assert_eq!(reset.meta().epoch, 3);
        assert!(!reset.is_actionable());
    }

    #[test]
    fn test_encoder_event_carries_delta() {
        let event = ControlEvent {
            raw_id: RawControlId::encoder(0),
            value: ControlValue::Encoder(3),
            delta: Some(3),
        };
        assert_eq!(event.delta, Some(3));
        assert_eq!(event.value, ControlValue::Encoder(3));
    }

    #[test]
    fn test_stream_item_serde_roundtrip() -> Result<(), serde_json::Error> {
        let item = ControlStreamItem::Descriptor {
            meta: StreamMeta {
                seq: 1,
                timestamp_ns: 0,
                epoch: 0,
            },
            surface: ControlSurfaceDescriptor {
                device: DeviceIdentity {
                    vendor_id: 0x1234,
                    product_id: 0x5678,
                    serial: Some("SN1".to_string()),
                    instance: 7,
                },
                mapping_version: 1,
                controls: vec![
                    ControlDescriptor::button(0),
                    ControlDescriptor::hat(),
                    ControlDescriptor::encoder(0)
                        .with_semantic("rotary_a", SemanticStatus::Validated),
                ],
            },
        };
        let json = serde_json::to_string(&item)?;
        let back: ControlStreamItem = serde_json::from_str(&json)?;
        assert_eq!(item, back);
        Ok(())
    }
}
