//! Deterministic projection of vendor-neutral [`DeviceInputs`] snapshots into
//! ordered, lossless control-stream items.
//!
//! This is pure, reusable logic with no transport, service, HID, protobuf, or
//! application/product coupling. It converts successive input snapshots into the
//! [`ControlStreamItem`] domain contract:
//!
//! - the first snapshot (or the one after a reset) produces a **descriptor** and
//!   a non-actionable **initial baseline** only — it never synthesizes button,
//!   hat, or rotary *actions*;
//! - subsequent snapshots produce ordered button/hat/encoder **events** for the
//!   values that actually changed;
//! - a producer epoch change (a firmware tick that moves backwards, e.g. a
//!   device restart/reconnect) or an explicit [`ControlProjector::reset`]
//!   produces a **reset** and a fresh baseline.
//!
//! Rotary handling is lossless: `DeviceInputs::rotaries` are per-report deltas,
//! and every non-zero delta is accumulated into a monotonic encoder position and
//! emitted as its own event. Multiple ticks that arrive before a consumer drains
//! are therefore never collapsed — three `+1` reports yield three `+1` events
//! whose positions advance `+1, +2, +3`.
//!
//! Scope is input-only (buttons, hat/D-pad, rotary encoders). Absolute axes
//! (steering, pedals, handbrake) are intentionally out of the initial contract.

use std::collections::VecDeque;

use crate::{
    ControlDescriptor, ControlEvent, ControlState, ControlStreamItem, ControlSurfaceDescriptor,
    ControlValue, DeviceIdentity, DeviceInputs, HatDirection, RawControlId, ResetReason,
    StreamMeta,
};

/// Maximum buttons a projector can surface (matches the `DeviceInputs` buffer).
pub const MAX_PROJECTED_BUTTONS: u8 = 128;
/// Maximum rotary encoders a projector can surface (matches `DeviceInputs`).
pub const MAX_PROJECTED_ENCODERS: u8 = 8;

/// Deterministic snapshot → control-stream-item projector for one device.
///
/// The projector is a pure state machine: feed it decoded [`DeviceInputs`]
/// snapshots with [`project`](Self::project) and drain the resulting ordered
/// [`ControlStreamItem`]s with [`drain`](Self::drain). It opens no device, does
/// no I/O, and never blocks.
#[derive(Debug, Clone)]
pub struct ControlProjector {
    device: DeviceIdentity,
    mapping_version: u32,
    button_count: u8,
    encoder_count: u8,

    epoch: u32,
    seq: u64,
    started: bool,
    last_tick: Option<u32>,

    prev_buttons: [u8; 16],
    prev_hat: u8,
    encoder_pos: [i32; 8],

    queue: VecDeque<ControlStreamItem>,
}

impl ControlProjector {
    /// Create a projector that surfaces the full 128-button / 8-encoder range.
    #[must_use]
    pub fn new(device: DeviceIdentity, mapping_version: u32) -> Self {
        Self::with_control_counts(
            device,
            mapping_version,
            MAX_PROJECTED_BUTTONS,
            MAX_PROJECTED_ENCODERS,
        )
    }

    /// Create a projector that surfaces `button_count` buttons and
    /// `encoder_count` encoders. Counts are clamped to the `DeviceInputs`
    /// buffer limits ([`MAX_PROJECTED_BUTTONS`], [`MAX_PROJECTED_ENCODERS`]).
    #[must_use]
    pub fn with_control_counts(
        device: DeviceIdentity,
        mapping_version: u32,
        button_count: u8,
        encoder_count: u8,
    ) -> Self {
        Self {
            device,
            mapping_version,
            button_count: button_count.min(MAX_PROJECTED_BUTTONS),
            encoder_count: encoder_count.min(MAX_PROJECTED_ENCODERS),
            epoch: 0,
            seq: 0,
            started: false,
            last_tick: None,
            prev_buttons: [0u8; 16],
            prev_hat: 0,
            encoder_pos: [0i32; 8],
            queue: VecDeque::new(),
        }
    }

    /// The current input epoch (incremented on every reset).
    #[must_use]
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Number of stream items currently queued for the consumer.
    #[must_use]
    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Feed one decoded snapshot, appending any resulting ordered stream items.
    ///
    /// `timestamp_ns` is the caller's monotonic capture time for this snapshot
    /// (the projector does not read a clock). Behaviour:
    ///
    /// - a snapshot whose `tick` equals the previously processed `tick` is a
    ///   duplicate and produces nothing;
    /// - a snapshot whose `tick` is *less* than the previous one is treated as a
    ///   producer epoch change: a [`ResetReason::EpochChange`] reset is emitted,
    ///   prior comparison state is cleared, and a fresh baseline follows;
    /// - the first snapshot of an epoch emits a descriptor and a non-actionable
    ///   initial baseline only;
    /// - later snapshots emit button/hat/encoder events for changed values, in
    ///   deterministic order (ascending button index, then hat, then ascending
    ///   encoder index).
    pub fn project(&mut self, inputs: &DeviceInputs, timestamp_ns: u64) {
        if let Some(last) = self.last_tick {
            if inputs.tick == last {
                // Duplicate source snapshot: never re-emit.
                return;
            }
            if inputs.tick < last {
                // Producer epoch moved backwards (restart / wrap / reconnect).
                self.begin_new_epoch(ResetReason::EpochChange, timestamp_ns);
            }
        }

        if !self.started {
            self.emit_baseline(inputs, timestamp_ns);
        } else {
            self.emit_edges(inputs, timestamp_ns);
        }

        self.prev_buttons = inputs.buttons;
        self.prev_hat = inputs.hat;
        self.last_tick = Some(inputs.tick);
    }

    /// Force a reset: clears comparison state and emits a [`ControlStreamItem::Reset`]
    /// with `reason`. The next [`project`](Self::project) re-establishes a baseline.
    ///
    /// Use this for producer-driven transitions the snapshot stream cannot convey
    /// on its own, such as an explicit device disconnect.
    pub fn reset(&mut self, reason: ResetReason, timestamp_ns: u64) {
        self.begin_new_epoch(reason, timestamp_ns);
    }

    /// Remove and return all queued stream items in order.
    #[must_use]
    pub fn drain(&mut self) -> Vec<ControlStreamItem> {
        self.queue.drain(..).collect()
    }

    // --- internals ------------------------------------------------------

    fn begin_new_epoch(&mut self, reason: ResetReason, timestamp_ns: u64) {
        self.epoch = self.epoch.wrapping_add(1);
        self.seq = 0;
        self.started = false;
        self.prev_buttons = [0u8; 16];
        self.prev_hat = 0;
        self.encoder_pos = [0i32; 8];
        // Clear the tick watermark so the next snapshot re-baselines even if the
        // producer's tick counter restarts at (or below) the pre-reset value.
        self.last_tick = None;
        let meta = self.next_meta(timestamp_ns);
        self.queue
            .push_back(ControlStreamItem::Reset { meta, reason });
    }

    fn next_meta(&mut self, timestamp_ns: u64) -> StreamMeta {
        let meta = StreamMeta {
            seq: self.seq,
            timestamp_ns,
            epoch: self.epoch,
        };
        self.seq = self.seq.wrapping_add(1);
        meta
    }

    fn descriptor(&self) -> ControlSurfaceDescriptor {
        let mut controls = Vec::with_capacity(
            usize::from(self.button_count) + 1 + usize::from(self.encoder_count),
        );
        for i in 0..self.button_count {
            controls.push(ControlDescriptor::button(i));
        }
        controls.push(ControlDescriptor::hat());
        for i in 0..self.encoder_count {
            controls.push(ControlDescriptor::encoder(i));
        }
        ControlSurfaceDescriptor {
            device: self.device.clone(),
            mapping_version: self.mapping_version,
            controls,
        }
    }

    /// Emit the descriptor and a non-actionable baseline. Encoder positions are
    /// reset to zero (there is no absolute position in the snapshot, and a
    /// baseline must not synthesize rotary motion), so any rotary delta carried
    /// by the baseline snapshot is intentionally not turned into an event.
    fn emit_baseline(&mut self, inputs: &DeviceInputs, timestamp_ns: u64) {
        self.encoder_pos = [0i32; 8];

        let surface = self.descriptor();
        let meta = self.next_meta(timestamp_ns);
        self.queue
            .push_back(ControlStreamItem::Descriptor { meta, surface });

        let mut states: Vec<ControlState> = Vec::new();
        for i in 0..self.button_count {
            if inputs.button(usize::from(i)) {
                states.push(ControlState {
                    raw_id: RawControlId::button(i),
                    value: ControlValue::Button(true),
                });
            }
        }
        let hat_dir = inputs.hat_direction();
        if hat_dir != HatDirection::Neutral {
            states.push(ControlState {
                raw_id: RawControlId::hat(),
                value: ControlValue::Hat(hat_dir),
            });
        }
        for i in 0..self.encoder_count {
            states.push(ControlState {
                raw_id: RawControlId::encoder(i),
                value: ControlValue::Encoder(0),
            });
        }

        let meta = self.next_meta(timestamp_ns);
        self.queue
            .push_back(ControlStreamItem::InitialSnapshot { meta, states });

        self.started = true;
    }

    fn emit_edges(&mut self, inputs: &DeviceInputs, timestamp_ns: u64) {
        // Buttons: ascending index, one event per changed bit.
        for i in 0..self.button_count {
            let idx = usize::from(i);
            let cur = inputs.button(idx);
            let prev = button_bit(&self.prev_buttons, idx);
            if cur != prev {
                let meta = self.next_meta(timestamp_ns);
                self.queue.push_back(ControlStreamItem::Event {
                    meta,
                    event: ControlEvent {
                        raw_id: RawControlId::button(i),
                        value: ControlValue::Button(cur),
                        delta: None,
                    },
                });
            }
        }

        // Hat: compare decoded directions so out-of-range codes collapse to
        // Neutral and never produce spurious transitions.
        let cur_hat = inputs.hat_direction();
        let prev_hat = DeviceInputs::default()
            .with_hat(self.prev_hat)
            .hat_direction();
        if cur_hat != prev_hat {
            let meta = self.next_meta(timestamp_ns);
            self.queue.push_back(ControlStreamItem::Event {
                meta,
                event: ControlEvent {
                    raw_id: RawControlId::hat(),
                    value: ControlValue::Hat(cur_hat),
                    delta: None,
                },
            });
        }

        // Encoders: ascending index, accumulate deltas into monotonic positions
        // so no tick is lost between consumer drains.
        for i in 0..self.encoder_count {
            let delta = i32::from(inputs.rotaries[usize::from(i)]);
            if delta != 0 {
                let pos = &mut self.encoder_pos[usize::from(i)];
                *pos = pos.saturating_add(delta);
                let position = *pos;
                let meta = self.next_meta(timestamp_ns);
                self.queue.push_back(ControlStreamItem::Event {
                    meta,
                    event: ControlEvent {
                        raw_id: RawControlId::encoder(i),
                        value: ControlValue::Encoder(position),
                        delta: Some(delta),
                    },
                });
            }
        }
    }
}

#[inline]
fn button_bit(buttons: &[u8; 16], index: usize) -> bool {
    if index < crate::MAX_BUTTONS {
        buttons[index / 8] & (1 << (index % 8)) != 0
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> DeviceIdentity {
        DeviceIdentity {
            vendor_id: 0x1234,
            product_id: 0x5678,
            serial: None,
            instance: 1,
        }
    }

    /// Small projector for focused tests: 8 buttons, 2 encoders.
    fn projector() -> ControlProjector {
        ControlProjector::with_control_counts(device(), 1, 8, 2)
    }

    fn tick(n: u32) -> DeviceInputs {
        DeviceInputs {
            tick: n,
            ..Default::default()
        }
    }

    #[test]
    fn first_snapshot_emits_baseline_only_no_action_edges() {
        let mut p = projector();
        let mut snap = tick(1);
        snap.set_button(3, true); // pressed at baseline
        p.project(&snap, 100);
        let items = p.drain();

        // Descriptor + InitialSnapshot, and nothing actionable.
        assert!(matches!(items[0], ControlStreamItem::Descriptor { .. }));
        assert!(matches!(
            items[1],
            ControlStreamItem::InitialSnapshot { .. }
        ));
        assert!(items.iter().all(|it| !it.is_actionable()));

        // The pressed button appears as a baseline state, not an event.
        let pressed_in_baseline = items.iter().any(|it| {
            matches!(it, ControlStreamItem::InitialSnapshot { states, .. }
            if states.iter().any(|s| {
                s.raw_id == RawControlId::button(3) && s.value == ControlValue::Button(true)
            }))
        });
        assert!(
            pressed_in_baseline,
            "pressed button 3 must be a baseline state"
        );
    }

    #[test]
    fn button_edges_project_press_and_release_in_order() {
        let mut p = projector();
        p.project(&tick(1), 0);
        let _ = p.drain();

        let mut s = tick(2);
        s.set_button(1, true);
        s.set_button(5, true);
        p.project(&s, 10);
        let items = p.drain();

        let events: Vec<_> = items
            .iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].raw_id, RawControlId::button(1));
        assert_eq!(events[0].value, ControlValue::Button(true));
        assert_eq!(events[1].raw_id, RawControlId::button(5));

        // Release button 1.
        let mut s2 = tick(3);
        s2.set_button(5, true); // still held
        p.project(&s2, 20);
        let released: Vec<_> = p
            .drain()
            .into_iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].raw_id, RawControlId::button(1));
        assert_eq!(released[0].value, ControlValue::Button(false));
    }

    #[test]
    fn full_button_range_projects_through_127() {
        let mut p = ControlProjector::new(device(), 1); // 128 buttons
        p.project(&tick(1), 0);
        let _ = p.drain();

        let mut s = tick(2);
        s.set_button(127, true);
        p.project(&s, 1);
        let events: Vec<_> = p
            .drain()
            .into_iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].raw_id, RawControlId::button(127));
        assert_eq!(events[0].value, ControlValue::Button(true));
    }

    #[test]
    fn hat_transitions_including_neutral_are_projected() {
        // Note: per `hat_direction`, code 0 is `Up` and codes >= 8 are `Neutral`;
        // the default snapshot (hat = 0) therefore baselines at `Up`.
        let mut p = projector();
        p.project(&tick(1), 0); // baseline at Up
        let _ = p.drain();

        // Up -> Right.
        let mut right = tick(2);
        right.hat = 2;
        p.project(&right, 1);
        assert_eq!(
            hat_events(p.drain()),
            vec![ControlValue::Hat(HatDirection::Right)]
        );

        // Right -> Neutral (code 8).
        let mut neutral = tick(3);
        neutral.hat = 8;
        p.project(&neutral, 2);
        assert_eq!(
            hat_events(p.drain()),
            vec![ControlValue::Hat(HatDirection::Neutral)]
        );
    }

    fn hat_events(items: Vec<ControlStreamItem>) -> Vec<ControlValue> {
        items
            .into_iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. } => match event.value {
                    ControlValue::Hat(_) => Some(event.value),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn three_rotary_ticks_before_drain_yield_plus_three_in_order() {
        let mut p = projector();
        p.project(&tick(1), 0); // baseline
        let _ = p.drain();

        for (i, ts) in (2u32..=4).enumerate() {
            let mut s = tick(ts);
            s.rotaries[0] = 1; // +1 delta each report
            p.project(&s, 100 + i as u64);
        }
        // Consumer drains only now: all three ticks must survive.
        let enc: Vec<_> = p
            .drain()
            .into_iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. }
                    if event.raw_id == RawControlId::encoder(0) =>
                {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(enc.len(), 3, "no tick may be collapsed");
        assert_eq!(enc[0].value, ControlValue::Encoder(1));
        assert_eq!(enc[1].value, ControlValue::Encoder(2));
        assert_eq!(enc[2].value, ControlValue::Encoder(3));
        assert_eq!(enc.iter().map(|e| e.delta.unwrap_or(0)).sum::<i32>(), 3);
    }

    #[test]
    fn duplicate_snapshot_tick_produces_no_events() {
        let mut p = projector();
        p.project(&tick(1), 0);
        let _ = p.drain();

        let mut s = tick(2);
        s.set_button(0, true);
        p.project(&s, 1);
        assert_eq!(p.drain().len(), 1); // one press event

        // Re-present the exact same tick: must be ignored.
        p.project(&s, 2);
        assert_eq!(p.drain().len(), 0);
    }

    #[test]
    fn simultaneous_changes_have_deterministic_order() {
        let mut p = projector();
        p.project(&tick(1), 0);
        let _ = p.drain();

        let mut s = tick(2);
        s.set_button(2, true);
        s.set_button(0, true);
        s.hat = 4; // Down
        s.rotaries[1] = 3;
        s.rotaries[0] = -1;
        p.project(&s, 1);

        let kinds: Vec<RawControlId> = p
            .drain()
            .into_iter()
            .filter_map(|it| match it {
                ControlStreamItem::Event { event, .. } => Some(event.raw_id),
                _ => None,
            })
            .collect();
        // Buttons ascending, then hat, then encoders ascending.
        assert_eq!(
            kinds,
            vec![
                RawControlId::button(0),
                RawControlId::button(2),
                RawControlId::hat(),
                RawControlId::encoder(0),
                RawControlId::encoder(1),
            ]
        );
    }

    #[test]
    fn reset_clears_prior_state_and_starts_new_epoch() {
        let mut p = projector();
        let mut s = tick(1);
        s.set_button(0, true);
        p.project(&s, 0);
        let _ = p.drain();

        p.reset(ResetReason::Disconnect, 5);
        let items = p.drain();
        assert_eq!(items.len(), 1);
        assert!(
            matches!(&items[0], ControlStreamItem::Reset { reason: ResetReason::Disconnect, meta }
                if meta.epoch == 1 && meta.seq == 0),
            "reset must open epoch 1 at seq 0"
        );

        // Next snapshot re-baselines; button 0 still held is baseline state, not
        // a re-synthesized press.
        let mut s2 = tick(2);
        s2.set_button(0, true);
        p.project(&s2, 6);
        let after = p.drain();
        assert!(matches!(after[0], ControlStreamItem::Descriptor { .. }));
        assert!(after.iter().all(|it| !it.is_actionable()));
    }

    #[test]
    fn backwards_tick_forces_epoch_change_reset_then_baseline() {
        let mut p = projector();
        p.project(&tick(10), 0);
        let _ = p.drain();

        let mut s = tick(11);
        s.set_button(1, true);
        p.project(&s, 1);
        let _ = p.drain();

        // Firmware restarts: tick goes backwards.
        p.project(&tick(3), 2);
        let items = p.drain();
        assert!(matches!(
            items[0],
            ControlStreamItem::Reset {
                reason: ResetReason::EpochChange,
                ..
            }
        ));
        assert!(matches!(items[1], ControlStreamItem::Descriptor { .. }));
        assert!(matches!(
            items[2],
            ControlStreamItem::InitialSnapshot { .. }
        ));
        assert_eq!(p.epoch(), 1);
    }

    #[test]
    fn sequence_is_monotonic_within_epoch_and_restarts_on_reset() {
        let mut p = projector();
        p.project(&tick(1), 0);
        let mut s = tick(2);
        s.set_button(0, true);
        p.project(&s, 1);
        let seqs: Vec<u64> = p.drain().iter().map(|it| it.seq()).collect();
        // baseline descriptor=0, baseline snapshot=1, press event=2
        assert_eq!(seqs, vec![0, 1, 2]);

        p.reset(ResetReason::Overflow, 2);
        let after = p.drain();
        assert_eq!(after[0].seq(), 0, "sequence restarts at the new epoch");
    }

    #[test]
    fn unchanged_state_produces_no_events() {
        let mut p = projector();
        let mut s = tick(1);
        s.set_button(4, true);
        p.project(&s, 0);
        let _ = p.drain();

        // Same buttons/hat, new tick, zero rotary delta: nothing changed.
        let mut s2 = tick(2);
        s2.set_button(4, true);
        p.project(&s2, 1);
        assert!(p.drain().is_empty());
    }
}
