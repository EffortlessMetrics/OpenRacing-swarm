//! Deterministic virtual-input fixtures replayed through [`ControlProjector`].
//!
//! These exercise the projector end-to-end without any hardware, and double as a
//! reference for external consumers integrating against the control-stream
//! domain contract. Each fixture is a plain sequence of decoded `DeviceInputs`
//! snapshots plus the timestamp the producer captured them at.

use openracing_device_types::{
    ControlProjector, ControlStreamItem, ControlValue, DeviceIdentity, DeviceInputs, HatDirection,
    RawControlId, ResetReason, SemanticStatus,
};

fn device() -> DeviceIdentity {
    DeviceIdentity {
        vendor_id: 0x1234,
        product_id: 0x5678,
        serial: Some("FIXTURE-001".to_string()),
        instance: 7,
    }
}

fn snap(tick: u32) -> DeviceInputs {
    DeviceInputs {
        tick,
        ..Default::default()
    }
}

/// Collect only the actionable events (raw_id + value) from a stream slice.
fn events(items: &[ControlStreamItem]) -> Vec<(RawControlId, ControlValue)> {
    items
        .iter()
        .filter_map(|it| match it {
            ControlStreamItem::Event { event, .. } => Some((event.raw_id, event.value)),
            _ => None,
        })
        .collect()
}

#[test]
fn full_session_button_hat_encoder_replays_losslessly() {
    // 4 buttons, 2 encoders for a compact, readable fixture.
    let mut p = ControlProjector::with_control_counts(device(), 1, 4, 2);

    // t1: baseline (button 1 already held — must be state, not a press event).
    let mut s1 = snap(1);
    s1.set_button(1, true);
    p.project(&s1, 1_000);

    // t2: press button 0, move hat to Down (code 4).
    let mut s2 = snap(2);
    s2.set_button(1, true);
    s2.set_button(0, true);
    s2.hat = 4;
    p.project(&s2, 2_000);

    // t3, t4, t5: three +1 encoder-0 ticks with no consumer drain in between.
    for (i, t) in (3u32..=5).enumerate() {
        let mut s = snap(t);
        s.set_button(1, true);
        s.set_button(0, true);
        s.hat = 4;
        s.rotaries[0] = 1;
        p.project(&s, 3_000 + i as u64 * 1_000);
    }

    // t6: release button 0.
    let mut s6 = snap(6);
    s6.set_button(1, true);
    s6.hat = 4;
    p.project(&s6, 6_000);

    let stream = p.drain();

    // Descriptor then baseline lead the stream and are non-actionable.
    assert!(matches!(stream[0], ControlStreamItem::Descriptor { .. }));
    assert!(matches!(
        stream[1],
        ControlStreamItem::InitialSnapshot { .. }
    ));

    let ev = events(&stream);
    assert_eq!(
        ev,
        vec![
            // t2
            (RawControlId::button(0), ControlValue::Button(true)),
            (RawControlId::hat(), ControlValue::Hat(HatDirection::Down)),
            // t3, t4, t5 — three ticks preserved, positions advance +1/+2/+3
            (RawControlId::encoder(0), ControlValue::Encoder(1)),
            (RawControlId::encoder(0), ControlValue::Encoder(2)),
            (RawControlId::encoder(0), ControlValue::Encoder(3)),
            // t6
            (RawControlId::button(0), ControlValue::Button(false)),
        ]
    );

    // Sequence numbers are strictly monotonic across the whole epoch.
    let seqs: Vec<u64> = stream.iter().map(|it| it.seq()).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(stream.iter().all(|it| it.meta().epoch == 0));
}

#[test]
fn disconnect_reconnect_fixture_starts_a_fresh_baseline() {
    let mut p = ControlProjector::with_control_counts(device(), 1, 4, 1);

    let mut s1 = snap(1);
    s1.set_button(2, true);
    p.project(&s1, 10);
    let first = p.drain();
    assert!(matches!(first[0], ControlStreamItem::Descriptor { .. }));

    // Producer reports a disconnect out-of-band.
    p.reset(ResetReason::Disconnect, 20);

    // Reconnect: new epoch, tick counter restarts low; button 2 still held is a
    // baseline state again, never a replayed press.
    let mut s2 = snap(1);
    s2.set_button(2, true);
    p.project(&s2, 30);

    let after = p.drain();
    assert!(matches!(
        after[0],
        ControlStreamItem::Reset {
            reason: ResetReason::Disconnect,
            ..
        }
    ));
    assert!(matches!(after[1], ControlStreamItem::Descriptor { .. }));
    assert!(matches!(
        after[2],
        ControlStreamItem::InitialSnapshot { .. }
    ));
    assert!(
        after.iter().all(|it| !it.is_actionable()),
        "reconnect must not synthesize held-button presses"
    );
    assert!(after.iter().all(|it| it.meta().epoch == 1));
}

#[test]
fn raw_only_and_validated_semantic_controls_are_distinguishable() {
    // The projector emits raw-only descriptors; a consumer/profile may attach a
    // semantic identity with explicit provenance. This fixture documents that a
    // raw control and a validated one remain distinguishable to consumers.
    use openracing_device_types::ControlDescriptor;

    let raw = ControlDescriptor::button(5);
    assert!(raw.semantic.is_none(), "projector output is raw-only");

    let validated =
        ControlDescriptor::encoder(0).with_semantic("rotary_a", SemanticStatus::Validated);
    let candidate = ControlDescriptor::button(5).with_semantic("paddle", SemanticStatus::Candidate);

    assert_eq!(
        validated.semantic.as_ref().map(|s| s.status),
        Some(SemanticStatus::Validated)
    );
    assert_eq!(
        candidate.semantic.as_ref().map(|s| s.status),
        Some(SemanticStatus::Candidate)
    );
    // A candidate is never silently promoted to validated.
    assert_ne!(
        candidate.semantic.map(|s| s.status),
        Some(SemanticStatus::Validated)
    );
}

#[test]
fn explicit_gap_reset_is_visible_between_epochs() {
    let mut p = ControlProjector::with_control_counts(device(), 1, 2, 1);
    p.project(&snap(1), 0);
    let _ = p.drain();

    // Simulate a subscriber-lag overflow forcing a resync.
    p.reset(ResetReason::Overflow, 5);
    let items = p.drain();
    assert!(matches!(
        items[0],
        ControlStreamItem::Reset {
            reason: ResetReason::Overflow,
            ..
        }
    ));
    assert_eq!(items[0].meta().epoch, 1);
    assert_eq!(items[0].seq(), 0);
}
