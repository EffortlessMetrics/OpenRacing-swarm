//! Deterministic projection of [`DeviceInputs`] snapshots into ordered,
//! lossless control-stream items (issue #169).
//!
//! [`ControlProjector`] is pure, reusable logic that converts OpenRacing's
//! vendor-neutral input snapshots into the [`ControlStreamItem`] domain
//! contract. It performs edge detection and lossless rotary accounting with
//! deterministic ordering and per-epoch sequencing. It has **no** transport,
//! service, threading, or protobuf concerns — a caller drives it by handing
//! successive snapshots to [`ControlProjector::observe`] and collecting the
//! returned items.
//!
//! ## Scope
//!
//! The initial input-only scope is buttons (`0..=127`), the hat/D-pad, and
//! relative rotary encoders. Absolute axes (steering/pedals) are intentionally
//! **not** projected as events here; projecting them would imply an input role
//! claim that is out of scope for this work item.
//!
//! ## Lossless rotary contract
//!
//! Each `DeviceInputs.rotaries[i]` value is treated as the signed encoder tick
//! **delta** accumulated by the report decoder for that snapshot. The projector
//! maintains a monotonically accumulating absolute position per encoder and
//! emits one [`ControlEvent`] per non-zero delta, carrying both the new
//! absolute position ([`ControlValue::Encoder`]) and the signed delta
//! ([`ControlEvent::delta`]). Because every non-zero delta is emitted and the
//! position is monotonic, no tick is ever collapsed by latest-snapshot polling:
//! three `+1` ticks observed before a consumer drains yield an effective `+3`.
//!
//! ## Baselines vs. actions
//!
//! The first observation (and the first observation after a reset) emits a
//! non-actionable [`ControlStreamItem::InitialSnapshot`] baseline and
//! establishes the previous state for edge detection. A baseline never
//! synthesizes button presses, hat actions, or rotary deltas.

use crate::{
    ControlDescriptor, ControlEvent, ControlState, ControlStreamItem, ControlSurfaceDescriptor,
    ControlValue, DeviceIdentity, DeviceInputs, MAX_BUTTONS, RawControlId, ResetReason, StreamMeta,
};

/// Number of rotary encoders addressable by [`DeviceInputs::rotaries`].
const ENCODER_COUNT: usize = 8;

/// Comparison state retained between observations for edge detection.
#[derive(Debug, Clone)]
struct PrevState {
    buttons: [u8; 16],
    hat: u8,
    /// Monotonically accumulating absolute position per encoder.
    encoder_positions: [i32; ENCODER_COUNT],
}

/// Projects [`DeviceInputs`] snapshots into ordered [`ControlStreamItem`]s.
///
/// The projector is deterministic: the same sequence of `observe`/`reset` calls
/// always produces the same items. It never opens a device, never polls, and
/// never blocks; the caller owns the input cadence.
#[derive(Debug, Clone)]
pub struct ControlProjector {
    device: DeviceIdentity,
    mapping_version: u32,
    /// Next sequence number to assign within the current epoch.
    next_seq: u64,
    /// Current input epoch; incremented on every reset.
    epoch: u32,
    /// `None` until a baseline is established (initial state and after a reset).
    prev: Option<PrevState>,
}

impl ControlProjector {
    /// Create a projector for a device surface at a given mapping version.
    #[must_use]
    pub fn new(device: DeviceIdentity, mapping_version: u32) -> Self {
        Self {
            device,
            mapping_version,
            next_seq: 0,
            epoch: 0,
            prev: None,
        }
    }

    /// The device identity this projector reports for.
    #[must_use]
    pub fn device(&self) -> &DeviceIdentity {
        &self.device
    }

    /// The mapping/contract version this projector reflects.
    #[must_use]
    pub fn mapping_version(&self) -> u32 {
        self.mapping_version
    }

    /// Emit the descriptor for this projector's generic input surface.
    ///
    /// The descriptor consumes the next sequence number so a collector can
    /// announce a surface before its initial baseline without creating a
    /// second sequence namespace.
    pub fn descriptor(&mut self, timestamp_ns: u64) -> ControlStreamItem {
        let mut controls = Vec::with_capacity(MAX_BUTTONS + 1 + ENCODER_COUNT);
        for index in 0..MAX_BUTTONS {
            controls.push(ControlDescriptor::button(index as u8));
        }
        controls.push(ControlDescriptor::hat());
        for index in 0..ENCODER_COUNT {
            controls.push(ControlDescriptor::encoder(index as u8));
        }

        let meta = self.next_meta(timestamp_ns);
        ControlStreamItem::Descriptor {
            meta,
            surface: ControlSurfaceDescriptor {
                device: self.device.clone(),
                mapping_version: self.mapping_version,
                controls,
            },
        }
    }

    /// The current input epoch.
    #[must_use]
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Whether a baseline has been established (i.e. the next `observe` emits
    /// events rather than a fresh baseline).
    #[must_use]
    pub fn is_established(&self) -> bool {
        self.prev.is_some()
    }

    /// Take and advance the next [`StreamMeta`] for the current epoch.
    fn next_meta(&mut self, timestamp_ns: u64) -> StreamMeta {
        let meta = StreamMeta {
            seq: self.next_seq,
            timestamp_ns,
            epoch: self.epoch,
        };
        self.next_seq += 1;
        meta
    }

    /// Observe an input snapshot and return the resulting stream items in order.
    ///
    /// On the first observation (or the first after [`Self::reset`]) this emits
    /// a single non-actionable [`ControlStreamItem::InitialSnapshot`] baseline
    /// and synthesizes no edges. On subsequent observations it emits one
    /// [`ControlStreamItem::Event`] per changed control, ordered deterministically
    /// as buttons (ascending index), then hat, then encoders (ascending index).
    /// An unchanged (duplicate) snapshot yields no events.
    pub fn observe(&mut self, inputs: &DeviceInputs, timestamp_ns: u64) -> Vec<ControlStreamItem> {
        match self.prev.take() {
            None => vec![self.emit_baseline(inputs, timestamp_ns)],
            Some(prev) => self.emit_edges(&prev, inputs, timestamp_ns),
        }
    }

    /// Establish a baseline from the current snapshot without synthesizing edges.
    fn emit_baseline(&mut self, inputs: &DeviceInputs, timestamp_ns: u64) -> ControlStreamItem {
        let mut states = Vec::new();

        // Buttons: report only currently-pressed buttons; unpressed are implied.
        for index in 0..MAX_BUTTONS {
            if inputs.button(index) {
                states.push(ControlState {
                    raw_id: RawControlId::button(index as u8),
                    value: ControlValue::Button(true),
                });
            }
        }

        // Hat: always report current direction.
        states.push(ControlState {
            raw_id: RawControlId::hat(),
            value: ControlValue::Hat(inputs.hat_direction()),
        });

        // Encoders: baseline positions start at zero; the first snapshot's raw
        // delta is intentionally *not* treated as motion (no synthesized rotary
        // action from a baseline).
        for i in 0..ENCODER_COUNT {
            states.push(ControlState {
                raw_id: RawControlId::encoder(i as u8),
                value: ControlValue::Encoder(0),
            });
        }

        self.prev = Some(PrevState {
            buttons: inputs.buttons,
            hat: inputs.hat,
            encoder_positions: [0; ENCODER_COUNT],
        });

        let meta = self.next_meta(timestamp_ns);
        ControlStreamItem::InitialSnapshot {
            meta,
            device: self.device.clone(),
            states,
        }
    }

    /// Emit ordered actionable events for the changes between `prev` and `inputs`.
    fn emit_edges(
        &mut self,
        prev: &PrevState,
        inputs: &DeviceInputs,
        timestamp_ns: u64,
    ) -> Vec<ControlStreamItem> {
        let mut events: Vec<ControlEvent> = Vec::new();

        // Buttons, ascending index, full 0..=127 range.
        let mut next = PrevState {
            buttons: inputs.buttons,
            hat: inputs.hat,
            encoder_positions: prev.encoder_positions,
        };
        for index in 0..MAX_BUTTONS {
            let was = button_bit(&prev.buttons, index);
            let now = button_bit(&inputs.buttons, index);
            if was != now {
                events.push(ControlEvent {
                    raw_id: RawControlId::button(index as u8),
                    value: ControlValue::Button(now),
                    delta: None,
                });
            }
        }

        // Hat, including transitions to and from neutral.
        let prev_hat = hat_direction(prev.hat);
        let now_hat = inputs.hat_direction();
        if prev_hat != now_hat {
            events.push(ControlEvent {
                raw_id: RawControlId::hat(),
                value: ControlValue::Hat(now_hat),
                delta: None,
            });
        }

        // Encoders, ascending index; accumulate deltas losslessly.
        for i in 0..ENCODER_COUNT {
            let delta = i32::from(inputs.rotaries[i]);
            if delta != 0 {
                let position = prev.encoder_positions[i].saturating_add(delta);
                next.encoder_positions[i] = position;
                events.push(ControlEvent {
                    raw_id: RawControlId::encoder(i as u8),
                    value: ControlValue::Encoder(position),
                    delta: Some(delta),
                });
            }
        }

        self.prev = Some(next);

        events
            .into_iter()
            .map(|event| {
                let meta = self.next_meta(timestamp_ns);
                ControlStreamItem::Event {
                    meta,
                    device: self.device.clone(),
                    event,
                }
            })
            .collect()
    }

    /// Emit a reset for `reason`, starting a new epoch and clearing prior
    /// comparison state so the next [`Self::observe`] re-establishes a baseline.
    ///
    /// The returned [`ControlStreamItem::Reset`] is sequence `0` of the new
    /// epoch; subsequent items in that epoch continue from `1`.
    pub fn reset(&mut self, reason: ResetReason, timestamp_ns: u64) -> ControlStreamItem {
        self.epoch += 1;
        self.next_seq = 0;
        self.prev = None;
        let meta = self.next_meta(timestamp_ns);
        ControlStreamItem::Reset {
            meta,
            device: self.device.clone(),
            reason,
        }
    }
}

/// Read a single button bit from a `[u8; 16]` bitfield (indices `0..128`).
fn button_bit(buttons: &[u8; 16], index: usize) -> bool {
    if index < MAX_BUTTONS {
        buttons[index / 8] & (1 << (index % 8)) != 0
    } else {
        false
    }
}

/// Map a raw hat byte to a [`crate::HatDirection`] the same way
/// [`DeviceInputs::hat_direction`] does, for previous-state comparison.
fn hat_direction(hat: u8) -> crate::HatDirection {
    let probe = DeviceInputs {
        hat,
        ..DeviceInputs::default()
    };
    probe.hat_direction()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HatDirection;

    fn device() -> DeviceIdentity {
        DeviceIdentity {
            logical_id: "projection-device".to_string(),
            vendor_id: 0x1234,
            product_id: 0x5678,
            serial: Some("SN-1".to_string()),
            instance: 1,
        }
    }

    fn projector() -> ControlProjector {
        ControlProjector::new(device(), 1)
    }

    fn events_only(items: Vec<ControlStreamItem>) -> Vec<ControlEvent> {
        items
            .into_iter()
            .filter_map(|item| match item {
                ControlStreamItem::Event { event, .. } => Some(event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn first_snapshot_is_baseline_only_with_no_edges() {
        let mut p = projector();
        let mut inputs = DeviceInputs::default();
        inputs.set_button(3, true);

        let items = p.observe(&inputs, 100);
        assert_eq!(items.len(), 1, "first snapshot emits exactly one item");
        assert!(matches!(
            items[0],
            ControlStreamItem::InitialSnapshot { .. }
        ));
        if let ControlStreamItem::InitialSnapshot { meta, states, .. } = &items[0] {
            assert_eq!(meta.seq, 0);
            assert_eq!(meta.epoch, 0);
            // Pressed button 3 is present as a state, not as an event.
            assert!(
                states.iter().any(|s| s.raw_id == RawControlId::button(3)
                    && s.value == ControlValue::Button(true))
            );
        }
        assert!(p.is_established());
    }

    #[test]
    fn button_edges_project_across_full_range_including_127() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0);

        let mut inputs = DeviceInputs::default();
        inputs.set_button(0, true);
        inputs.set_button(127, true);
        let events = events_only(p.observe(&inputs, 10));

        assert_eq!(events.len(), 2);
        // Deterministic ascending order: button 0 before button 127.
        assert_eq!(events[0].raw_id, RawControlId::button(0));
        assert_eq!(events[0].value, ControlValue::Button(true));
        assert_eq!(events[1].raw_id, RawControlId::button(127));
        assert_eq!(events[1].value, ControlValue::Button(true));

        // Release button 127 -> a single release edge.
        let mut released = inputs;
        released.set_button(127, false);
        let events = events_only(p.observe(&released, 20));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].raw_id, RawControlId::button(127));
        assert_eq!(events[0].value, ControlValue::Button(false));
    }

    #[test]
    fn simultaneous_button_edges_are_deterministically_ordered() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0);

        let mut inputs = DeviceInputs::default();
        for idx in [5usize, 1, 63, 8] {
            inputs.set_button(idx, true);
        }
        let events = events_only(p.observe(&inputs, 10));
        let ids: Vec<u32> = events.iter().map(|e| e.raw_id.0).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "button edges must be ascending by raw id");
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn hat_transitions_including_neutral_are_projected() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0); // baseline hat = Up (raw 0)

        let up_to_right = DeviceInputs {
            hat: 2,
            ..DeviceInputs::default()
        };
        let events = events_only(p.observe(&up_to_right, 10));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, ControlValue::Hat(HatDirection::Right));

        // Move to neutral (raw >= 8).
        let to_neutral = DeviceInputs {
            hat: 0xFF,
            ..DeviceInputs::default()
        };
        let events = events_only(p.observe(&to_neutral, 20));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, ControlValue::Hat(HatDirection::Neutral));
    }

    #[test]
    fn three_plus_one_encoder_ticks_accumulate_to_plus_three() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0);

        // Three separate snapshots, each a +1 delta on encoder 2, all before
        // the consumer inspects the accumulated result.
        let mut all = Vec::new();
        for t in 0..3u64 {
            let mut rotaries = [0i16; 8];
            rotaries[2] = 1;
            let inputs = DeviceInputs {
                rotaries,
                ..DeviceInputs::default()
            };
            all.extend(events_only(p.observe(&inputs, 10 + t)));
        }

        assert_eq!(all.len(), 3, "no tick may be collapsed");
        let total: i32 = all.iter().filter_map(|e| e.delta).sum();
        assert_eq!(total, 3, "three +1 ticks must net +3");
        // Positions are monotonically accumulating: 1, 2, 3.
        let positions: Vec<i32> = all
            .iter()
            .map(|e| match e.value {
                ControlValue::Encoder(pos) => pos,
                _ => -1,
            })
            .collect();
        assert_eq!(positions, vec![1, 2, 3]);
    }

    #[test]
    fn single_snapshot_multi_tick_delta_is_lossless() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0);

        let mut rotaries = [0i16; 8];
        rotaries[0] = 3; // decoder accumulated three ticks into one snapshot
        let inputs = DeviceInputs {
            rotaries,
            ..DeviceInputs::default()
        };
        let events = events_only(p.observe(&inputs, 10));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].delta, Some(3));
        assert_eq!(events[0].value, ControlValue::Encoder(3));
    }

    #[test]
    fn duplicate_snapshot_produces_no_events() {
        let mut p = projector();
        let mut inputs = DeviceInputs::default();
        inputs.set_button(4, true);
        p.observe(&inputs, 0); // baseline

        // Same state again (encoders zero) => nothing changed.
        let events = events_only(p.observe(&inputs, 10));
        assert!(
            events.is_empty(),
            "duplicate snapshot must not duplicate events"
        );
    }

    #[test]
    fn reset_clears_state_starts_new_epoch_and_re_baselines() {
        let mut p = projector();
        let mut pressed = DeviceInputs::default();
        pressed.set_button(2, true);
        p.observe(&pressed, 0); // baseline epoch 0, seq 0

        // An event advances the sequence.
        let mut also = pressed;
        also.set_button(9, true);
        let ev = p.observe(&also, 5);
        assert_eq!(ev[0].meta().seq, 1);
        assert_eq!(ev[0].meta().epoch, 0);

        // Reset: new epoch, seq restarts at 0, prior state dropped.
        let reset = p.reset(ResetReason::Disconnect, 10);
        assert!(matches!(reset, ControlStreamItem::Reset { .. }));
        if let ControlStreamItem::Reset { meta, reason, .. } = reset {
            assert_eq!(meta.epoch, 1);
            assert_eq!(meta.seq, 0);
            assert_eq!(reason, ResetReason::Disconnect);
        }
        assert!(!p.is_established(), "reset must clear comparison state");

        // Next observe re-establishes a baseline (not an event) in the new epoch.
        let items = p.observe(&pressed, 15);
        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0],
            ControlStreamItem::InitialSnapshot { .. }
        ));
        if let ControlStreamItem::InitialSnapshot { meta, .. } = &items[0] {
            assert_eq!(meta.epoch, 1);
            assert_eq!(meta.seq, 1, "baseline follows the reset within the epoch");
        }
    }

    #[test]
    fn sequence_is_monotonic_within_an_epoch() {
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0); // seq 0

        let mut seqs = Vec::new();
        for idx in 0..5u8 {
            let mut inputs = DeviceInputs::default();
            inputs.set_button(idx as usize, true);
            for item in p.observe(&inputs, 10 + u64::from(idx)) {
                seqs.push(item.meta().seq);
            }
        }
        // Strictly increasing, starting after the baseline's seq 0.
        assert!(
            seqs.windows(2).all(|w| w[1] > w[0]),
            "sequence must be monotonic"
        );
        assert_eq!(seqs.first().copied(), Some(1));
    }

    #[test]
    fn projector_never_panics_on_arbitrary_input() {
        // Exercise extreme values: all buttons set, out-of-range hat, saturating
        // encoder deltas. The projector must not panic or overflow.
        let mut p = projector();
        p.observe(&DeviceInputs::default(), 0);

        let inputs = DeviceInputs {
            buttons: [0xFF; 16],
            hat: u8::MAX,
            rotaries: [i16::MAX; 8],
            ..DeviceInputs::default()
        };
        let _ = p.observe(&inputs, 10);
        // A second identical extreme snapshot: encoders still move by i16::MAX,
        // and the accumulator must saturate rather than overflow.
        let _ = p.observe(&inputs, 20);
        assert!(p.is_established());
    }
}
