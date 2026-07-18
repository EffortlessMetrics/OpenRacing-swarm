//! Transport-neutral control-stream domain contract.
//!
//! These types describe a generic, **observe-only** control-input stream:
//! a device (control *surface*) exposes buttons, hats, and rotary encoders,
//! and the stream reports a descriptor, an initial non-actionable baseline,
//! ordered change events, and explicit reset/disconnect notifications.
//!
//! Design constraints (see issue #168 / ADR-0010):
//!
//! * **No transport dependency.** These are plain data types with `serde`
//!   support so later IPC/capture work can serialize them, but they carry no
//!   `protobuf`/`tonic` types and impose no wire format.
//! * **No application semantics.** No consumer/application name (e.g. any
//!   downstream bridge) appears in this contract; it is generic OpenRacing
//!   infrastructure.
//! * **Provenance is explicit.** A raw control id is always available; an
//!   optional semantic id carries `raw`/`candidate`/`validated` provenance so
//!   consumers never mistake an unverified guess for a validated role.
//! * **Baselines are not actions.** [`ControlStreamItem::InitialSnapshot`] is a
//!   distinct variant from [`ControlStreamItem::Event`]; a baseline never
//!   implies a button press or rotary action.
//!
//! This module defines the *shape* of the contract only. Projecting decoded
//! [`crate::DeviceInputs`] snapshots into these items (edge detection, lossless
//! rotary accumulation, sequencing) is deliberately out of scope here and is
//! handled by the projection work item (issue #169).

use serde::{Deserialize, Serialize};

use crate::HatDirection;

/// Stable identity of a physical control surface (an input-capable device).
///
/// Identity fields are chosen so descriptors, baselines, and events can be
/// correlated across a stream and across reconnects. Prefer a firmware serial
/// for [`Self::instance`]; fall back to a stable enumeration-derived key.
/// Unstable OS device paths must not be used as identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ControlSurfaceId {
    /// USB vendor id.
    pub vendor_id: u16,
    /// USB product id.
    pub product_id: u16,
    /// Stable per-instance key (serial number when reported, otherwise a
    /// stable enumeration-derived identity). Never a volatile OS path.
    pub instance: String,
}

impl ControlSurfaceId {
    /// Construct a surface id from its stable identity fields.
    pub fn new(vendor_id: u16, product_id: u16, instance: impl Into<String>) -> Self {
        Self {
            vendor_id,
            product_id,
            instance: instance.into(),
        }
    }
}

/// The category of a control on a surface.
///
/// Buttons, hats, and encoders are the initial input-only scope. `Axis` is
/// included so descriptors can *describe* absolute axes (steering/pedals)
/// without the stream claiming them as actionable; axis event projection is
/// intentionally out of the initial scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlKind {
    /// A digital push button (`0..=127`).
    Button,
    /// A hat switch / D-pad.
    Hat,
    /// A relative rotary encoder / rotary switch reporting deltas.
    Encoder,
    /// An absolute axis (described for completeness; not projected as events
    /// in the initial input-only scope).
    Axis,
}

/// A raw, always-available control identifier.
///
/// The raw id is derived purely from the decoded report layout — a `kind` plus
/// a stable numeric index — and never depends on semantic interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawControlId {
    /// The control category.
    pub kind: ControlKind,
    /// Stable zero-based index within the kind (e.g. button `0..=127`,
    /// encoder `0..=7`, hat `0`).
    pub index: u16,
}

impl RawControlId {
    /// Construct a raw control id.
    pub fn new(kind: ControlKind, index: u16) -> Self {
        Self { kind, index }
    }

    /// Convenience constructor for a button raw id.
    pub fn button(index: u16) -> Self {
        Self::new(ControlKind::Button, index)
    }

    /// Convenience constructor for an encoder raw id.
    pub fn encoder(index: u16) -> Self {
        Self::new(ControlKind::Encoder, index)
    }

    /// Convenience constructor for a hat raw id.
    pub fn hat(index: u16) -> Self {
        Self::new(ControlKind::Hat, index)
    }
}

/// Provenance of a semantic (named) control identity.
///
/// A semantic id is only ever a *hypothesis* until independently validated;
/// this status keeps that provenance explicit so consumers cannot silently
/// promote a raw guess into a named role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticStatus {
    /// No semantic meaning claimed; only the raw id is trustworthy.
    Raw,
    /// A proposed semantic mapping that has not been validated.
    Candidate,
    /// A semantic mapping validated through the lane's evidence process.
    Validated,
}

/// An optional semantic (human/application-facing) control identity together
/// with its [`SemanticStatus`] provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticId {
    /// Stable semantic identifier (e.g. `"paddle.left"`). Opaque to this crate.
    pub id: String,
    /// Provenance of the semantic mapping.
    pub status: SemanticStatus,
}

impl SemanticId {
    /// Construct a semantic id with explicit provenance.
    pub fn new(id: impl Into<String>, status: SemanticStatus) -> Self {
        Self {
            id: id.into(),
            status,
        }
    }
}

/// A static description of one control on a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDescriptor {
    /// Always-available raw identity.
    pub raw_id: RawControlId,
    /// Optional semantic identity with explicit provenance. `None` means the
    /// control is raw-only.
    pub semantic: Option<SemanticId>,
}

impl ControlDescriptor {
    /// A raw-only control descriptor (no semantic claim).
    pub fn raw(raw_id: RawControlId) -> Self {
        Self {
            raw_id,
            semantic: None,
        }
    }

    /// A control descriptor carrying a semantic identity.
    pub fn with_semantic(raw_id: RawControlId, semantic: SemanticId) -> Self {
        Self {
            raw_id,
            semantic: Some(semantic),
        }
    }
}

/// A description of a control surface and the controls it exposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlSurfaceDescriptor {
    /// Stable identity of the surface.
    pub surface: ControlSurfaceId,
    /// Human-readable device name, when known.
    pub name: Option<String>,
    /// The controls this surface exposes.
    pub controls: Vec<ControlDescriptor>,
}

/// The value of a control at a point in time.
///
/// `Encoder` carries a signed delta (accumulated ticks since the previous
/// event) rather than an absolute position, matching the lossless relative
/// rotary contract required downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlValue {
    /// Digital button state.
    Button(bool),
    /// Hat switch direction.
    Hat(HatDirection),
    /// Signed encoder delta (accumulated ticks since the previous event).
    Encoder(i32),
    /// Absolute axis value (`0..=65535`). Present for baselines only in the
    /// initial scope; not emitted as an actionable event.
    Axis(u16),
}

/// The state of a single control, used within a non-actionable baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlState {
    /// The control this state refers to.
    pub raw_id: RawControlId,
    /// Its current value.
    pub value: ControlValue,
}

impl ControlState {
    /// Construct a control state.
    pub fn new(raw_id: RawControlId, value: ControlValue) -> Self {
        Self { raw_id, value }
    }
}

/// A single actionable control change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEvent {
    /// The control that changed.
    pub raw_id: RawControlId,
    /// The new value (or, for encoders, the signed delta) of the control.
    pub value: ControlValue,
}

impl ControlEvent {
    /// Construct a control event.
    pub fn new(raw_id: RawControlId, value: ControlValue) -> Self {
        Self { raw_id, value }
    }
}

/// Why a stream reset/gap occurred. A reset invalidates the consumer's prior
/// comparison state and is followed by a fresh baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResetReason {
    /// First observation of the surface.
    InitialConnect,
    /// The device reconnected after a disconnect.
    Reconnect,
    /// The producer's input epoch changed (e.g. firmware/session restart).
    EpochChange,
    /// A subscriber fell too far behind and lost events.
    SubscriberLag,
    /// The bounded producer queue overflowed.
    ProducerOverflow,
    /// The owning service restarted.
    ServiceRestart,
}

/// Ordering metadata carried by every sequenced stream item.
///
/// `sequence` is monotonically increasing within an `epoch`; a reset starts a
/// new epoch. `timestamp_nanos` is a monotonic source-clock reading (in
/// nanoseconds) for diagnostics and does not define ordering on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamSeq {
    /// Input epoch; incremented on every reset/reconnect.
    pub epoch: u32,
    /// Monotonically increasing sequence within the current epoch.
    pub sequence: u64,
    /// Monotonic source timestamp in nanoseconds.
    pub timestamp_nanos: u64,
}

impl StreamSeq {
    /// Construct sequencing metadata.
    pub fn new(epoch: u32, sequence: u64, timestamp_nanos: u64) -> Self {
        Self {
            epoch,
            sequence,
            timestamp_nanos,
        }
    }
}

/// One item in a control stream.
///
/// Variants are ordered as a consumer observes them:
/// [`Descriptor`](Self::Descriptor) (surface metadata, may be re-sent),
/// [`InitialSnapshot`](Self::InitialSnapshot) (non-actionable baseline that
/// begins a sequenced epoch), [`Event`](Self::Event) (actionable changes), and
/// [`Reset`](Self::Reset) (an explicit gap/disconnect that ends the current
/// epoch). Baselines are deliberately distinct from events so a consumer never
/// treats current state as a fresh action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlStreamItem {
    /// Static description of a surface and the controls it exposes. Descriptors
    /// are out-of-band metadata and carry no sequence.
    Descriptor(ControlSurfaceDescriptor),
    /// Non-actionable initial state of every control, beginning an epoch.
    InitialSnapshot {
        /// Sequencing metadata for this baseline.
        seq: StreamSeq,
        /// The surface this baseline describes.
        surface: ControlSurfaceId,
        /// Current state of each control; never synthesizes actions.
        controls: Vec<ControlState>,
    },
    /// A single ordered, actionable control change.
    Event {
        /// Sequencing metadata for this event.
        seq: StreamSeq,
        /// The surface that produced the event.
        surface: ControlSurfaceId,
        /// The change itself.
        event: ControlEvent,
    },
    /// An explicit reset/gap/disconnect notification. Consumers must drop prior
    /// comparison state and await a fresh baseline.
    Reset {
        /// Sequencing metadata for this reset.
        seq: StreamSeq,
        /// The affected surface.
        surface: ControlSurfaceId,
        /// Why the reset occurred.
        reason: ResetReason,
    },
}

impl ControlStreamItem {
    /// Sequencing metadata for sequenced items; `None` for a bare descriptor.
    pub fn seq(&self) -> Option<StreamSeq> {
        match self {
            ControlStreamItem::Descriptor(_) => None,
            ControlStreamItem::InitialSnapshot { seq, .. }
            | ControlStreamItem::Event { seq, .. }
            | ControlStreamItem::Reset { seq, .. } => Some(*seq),
        }
    }

    /// Whether this item is a non-actionable baseline. A consumer must not
    /// treat a baseline as a fresh button press or rotary action.
    pub fn is_baseline(&self) -> bool {
        matches!(self, ControlStreamItem::InitialSnapshot { .. })
    }

    /// Whether this item is an actionable control change.
    pub fn is_actionable(&self) -> bool {
        matches!(self, ControlStreamItem::Event { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_surface() -> ControlSurfaceId {
        ControlSurfaceId::new(0x1234, 0x5678, "SN-0001")
    }

    #[test]
    fn raw_id_is_always_available_and_semantic_is_optional() {
        let raw_only = ControlDescriptor::raw(RawControlId::button(7));
        assert_eq!(raw_only.raw_id, RawControlId::button(7));
        assert!(raw_only.semantic.is_none());

        let named = ControlDescriptor::with_semantic(
            RawControlId::button(7),
            SemanticId::new("paddle.left", SemanticStatus::Candidate),
        );
        assert_eq!(named.raw_id.kind, ControlKind::Button);
        assert_eq!(
            named.semantic,
            Some(SemanticId::new("paddle.left", SemanticStatus::Candidate))
        );
    }

    #[test]
    fn semantic_status_provenance_is_distinguishable() {
        // The three provenance levels must be distinct values.
        assert_ne!(SemanticStatus::Raw, SemanticStatus::Candidate);
        assert_ne!(SemanticStatus::Candidate, SemanticStatus::Validated);
        assert_ne!(SemanticStatus::Raw, SemanticStatus::Validated);
    }

    #[test]
    fn baseline_is_distinguishable_from_event() {
        let seq = StreamSeq::new(0, 1, 1_000);
        let baseline = ControlStreamItem::InitialSnapshot {
            seq,
            surface: sample_surface(),
            controls: vec![ControlState::new(
                RawControlId::button(0),
                ControlValue::Button(true),
            )],
        };
        let event = ControlStreamItem::Event {
            seq: StreamSeq::new(0, 2, 2_000),
            surface: sample_surface(),
            event: ControlEvent::new(RawControlId::button(0), ControlValue::Button(false)),
        };

        assert!(baseline.is_baseline());
        assert!(!baseline.is_actionable());
        assert!(event.is_actionable());
        assert!(!event.is_baseline());
    }

    #[test]
    fn descriptor_has_no_sequence_but_sequenced_items_do() {
        let descriptor = ControlStreamItem::Descriptor(ControlSurfaceDescriptor {
            surface: sample_surface(),
            name: Some("Test Wheel".to_string()),
            controls: vec![ControlDescriptor::raw(RawControlId::encoder(0))],
        });
        assert!(descriptor.seq().is_none());

        let reset = ControlStreamItem::Reset {
            seq: StreamSeq::new(1, 1, 42),
            surface: sample_surface(),
            reason: ResetReason::Reconnect,
        };
        assert_eq!(reset.seq().map(|s| s.epoch), Some(1));
    }

    #[test]
    fn encoder_value_carries_signed_delta() {
        // Lossless relative rotary: three +1 ticks accumulate to +3.
        let event = ControlEvent::new(RawControlId::encoder(2), ControlValue::Encoder(3));
        assert_eq!(event.value, ControlValue::Encoder(3));
    }

    #[test]
    fn stream_item_roundtrips_through_json() -> Result<(), serde_json::Error> {
        let item = ControlStreamItem::Event {
            seq: StreamSeq::new(2, 9, 123_456),
            surface: sample_surface(),
            event: ControlEvent::new(RawControlId::hat(0), ControlValue::Hat(HatDirection::Up)),
        };
        let json = serde_json::to_string(&item)?;
        let back: ControlStreamItem = serde_json::from_str(&json)?;
        assert_eq!(item, back);
        Ok(())
    }

    #[test]
    fn surface_id_is_hashable_for_correlation() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(sample_surface());
        assert!(set.contains(&ControlSurfaceId::new(0x1234, 0x5678, "SN-0001")));
        assert!(!set.contains(&ControlSurfaceId::new(0x1234, 0x5678, "SN-0002")));
    }
}
