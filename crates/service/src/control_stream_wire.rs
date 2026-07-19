//! gRPC conversion and filtering for the observe-only control stream.
//!
//! This module is deliberately a transport boundary: it contains no HID,
//! device ownership, or real-time work. Unknown subscription values are
//! rejected, while domain items are converted without assigning application
//! or safety semantics.

use openracing_device_types::{
    ControlDescriptor, ControlEvent, ControlKind, ControlState, ControlStreamItem,
    ControlSurfaceDescriptor, ControlValue, DeviceIdentity, HatDirection, RawControlId,
    ResetReason, SemanticStatus,
};
use racing_wheel_schemas::generated::wheel::v1::{
    ControlBaseline, ControlDescriptor as WireControlDescriptor, ControlDeviceIdentity,
    ControlDisconnect, ControlEvent as WireControlEvent, ControlReset, ControlSemantic,
    ControlState as WireControlState, ControlStreamItem as WireControlStreamItem,
    ControlStreamMetadata, ControlSubscription,
    ControlSurfaceDescriptor as WireControlSurfaceDescriptor, ControlValue as WireControlValue,
    control_stream_item::Item as WireControlStreamItemVariant,
    control_value::Value as WireControlValueVariant,
};
use tonic::Status;

/// Feature name advertised by servers that expose this RPC.
pub const CONTROL_STREAM_FEATURE: &str = "control_stream_v1";

/// Parse the wire filter into domain control kinds.
pub fn requested_kinds(request: &ControlSubscription) -> Result<Vec<ControlKind>, Status> {
    request
        .control_kinds
        .iter()
        .map(|value| match *value {
            1 => Ok(ControlKind::Button),
            2 => Ok(ControlKind::Hat),
            3 => Ok(ControlKind::Encoder),
            4 => Ok(ControlKind::Axis),
            _ => Err(Status::invalid_argument(format!(
                "unknown control kind value {value}"
            ))),
        })
        .collect()
}

/// Apply the device and control-kind filters to one domain item.
pub fn filter_item(
    item: ControlStreamItem,
    request: &ControlSubscription,
    kinds: &[ControlKind],
) -> Option<ControlStreamItem> {
    if !request.device_id.is_empty() && item.device().logical_id != request.device_id {
        return None;
    }
    if kinds.is_empty() {
        return Some(item);
    }

    match item {
        ControlStreamItem::Descriptor { meta, mut surface } => {
            surface
                .controls
                .retain(|control| kinds.contains(&control.kind));
            Some(ControlStreamItem::Descriptor { meta, surface })
        }
        ControlStreamItem::InitialSnapshot {
            meta,
            device,
            mut states,
        } => {
            states.retain(|state| {
                control_kind(state.raw_id).is_some_and(|kind| kinds.contains(&kind))
            });
            Some(ControlStreamItem::InitialSnapshot {
                meta,
                device,
                states,
            })
        }
        ControlStreamItem::Event {
            meta,
            device,
            event,
        } => control_kind(event.raw_id)
            .filter(|kind| kinds.contains(kind))
            .map(|_| ControlStreamItem::Event {
                meta,
                device,
                event,
            }),
        reset @ ControlStreamItem::Reset { .. } => Some(reset),
    }
}

/// Convert one filtered domain item into the versioned wire representation.
pub fn to_wire_item(item: ControlStreamItem) -> Result<WireControlStreamItem, Status> {
    let metadata = metadata(item.meta(), item.device());
    let item_variant = match item {
        ControlStreamItem::Descriptor { surface, .. } => {
            WireControlStreamItemVariant::Descriptor(surface_to_wire(surface))
        }
        ControlStreamItem::InitialSnapshot { states, .. } => {
            WireControlStreamItemVariant::Baseline(ControlBaseline {
                states: states.into_iter().map(state_to_wire).collect(),
            })
        }
        ControlStreamItem::Event { event, .. } => {
            WireControlStreamItemVariant::Event(event_to_wire(event)?)
        }
        ControlStreamItem::Reset {
            reason: ResetReason::Disconnect,
            ..
        } => WireControlStreamItemVariant::Disconnect(ControlDisconnect {
            reason: "device_disconnected".to_string(),
        }),
        ControlStreamItem::Reset { reason, .. } => {
            WireControlStreamItemVariant::Reset(ControlReset {
                reason: reset_reason_value(reason),
            })
        }
    };

    Ok(WireControlStreamItem {
        metadata: Some(metadata),
        item: Some(item_variant),
    })
}

fn metadata(
    meta: &openracing_device_types::StreamMeta,
    device: &DeviceIdentity,
) -> ControlStreamMetadata {
    ControlStreamMetadata {
        sequence: meta.seq,
        timestamp_ns: meta.timestamp_ns,
        epoch: meta.epoch,
        device: Some(identity_to_wire(device)),
    }
}

fn identity_to_wire(device: &DeviceIdentity) -> ControlDeviceIdentity {
    ControlDeviceIdentity {
        logical_id: device.logical_id.clone(),
        vendor_id: u32::from(device.vendor_id),
        product_id: u32::from(device.product_id),
        serial: device.serial.clone().unwrap_or_default(),
        instance: device.instance,
    }
}

fn surface_to_wire(surface: ControlSurfaceDescriptor) -> WireControlSurfaceDescriptor {
    WireControlSurfaceDescriptor {
        device: Some(identity_to_wire(&surface.device)),
        mapping_version: surface.mapping_version,
        controls: surface
            .controls
            .into_iter()
            .map(descriptor_to_wire)
            .collect(),
    }
}

fn descriptor_to_wire(control: ControlDescriptor) -> WireControlDescriptor {
    WireControlDescriptor {
        raw_id: control.raw_id.0,
        kind: kind_value(control.kind),
        semantic: control.semantic.map(|semantic| ControlSemantic {
            label: semantic.label,
            status: status_value(semantic.status),
        }),
    }
}

fn state_to_wire(state: ControlState) -> WireControlState {
    WireControlState {
        raw_id: state.raw_id.0,
        value: Some(value_to_wire(state.value)),
    }
}

fn event_to_wire(event: ControlEvent) -> Result<WireControlEvent, Status> {
    Ok(WireControlEvent {
        raw_id: event.raw_id.0,
        value: Some(value_to_wire(event.value)),
        delta: event.delta,
    })
}

fn value_to_wire(value: ControlValue) -> WireControlValue {
    let value = match value {
        ControlValue::Button(value) => WireControlValueVariant::Button(value),
        ControlValue::Hat(value) => WireControlValueVariant::Hat(hat_value(value)),
        ControlValue::Encoder(value) => WireControlValueVariant::Encoder(value),
        ControlValue::Axis(value) => WireControlValueVariant::Axis(u32::from(value)),
    };
    WireControlValue { value: Some(value) }
}

fn control_kind(raw_id: RawControlId) -> Option<ControlKind> {
    match raw_id.0 >> 24 {
        0x00 => Some(ControlKind::Button),
        0x01 => Some(ControlKind::Hat),
        0x02 => Some(ControlKind::Encoder),
        0x03 => Some(ControlKind::Axis),
        _ => None,
    }
}

const fn kind_value(kind: ControlKind) -> i32 {
    match kind {
        ControlKind::Button => 1,
        ControlKind::Hat => 2,
        ControlKind::Encoder => 3,
        ControlKind::Axis => 4,
    }
}

const fn status_value(status: SemanticStatus) -> i32 {
    match status {
        SemanticStatus::Raw => 0,
        SemanticStatus::Candidate => 1,
        SemanticStatus::Validated => 2,
    }
}

const fn hat_value(value: HatDirection) -> i32 {
    match value {
        HatDirection::Neutral => 0,
        HatDirection::Up => 1,
        HatDirection::UpRight => 2,
        HatDirection::Right => 3,
        HatDirection::DownRight => 4,
        HatDirection::Down => 5,
        HatDirection::DownLeft => 6,
        HatDirection::Left => 7,
        HatDirection::UpLeft => 8,
    }
}

const fn reset_reason_value(value: ResetReason) -> i32 {
    match value {
        ResetReason::Initial => 1,
        ResetReason::EpochChange => 2,
        ResetReason::Disconnect => 3,
        ResetReason::Reconnect => 4,
        ResetReason::Overflow => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openracing_device_types::{ControlDescriptor, StreamMeta};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn device() -> DeviceIdentity {
        DeviceIdentity {
            logical_id: "wire-device".to_string(),
            vendor_id: 0x1234,
            product_id: 0x5678,
            serial: Some("serial".to_string()),
            instance: 4,
        }
    }

    #[test]
    fn rejects_unknown_filter_enum_values() {
        let request = ControlSubscription {
            device_id: String::new(),
            control_kinds: vec![0],
        };
        let result = requested_kinds(&request);
        assert!(result.is_err());
    }

    #[test]
    fn filters_descriptor_and_event_by_kind_and_device() -> TestResult {
        let request = ControlSubscription {
            device_id: "wire-device".to_string(),
            control_kinds: vec![1],
        };
        let kinds = requested_kinds(&request)?;
        let item = ControlStreamItem::Descriptor {
            meta: StreamMeta::default(),
            surface: ControlSurfaceDescriptor {
                device: device(),
                mapping_version: 1,
                controls: vec![ControlDescriptor::button(0), ControlDescriptor::encoder(0)],
            },
        };
        let filtered =
            filter_item(item, &request, &kinds).ok_or("matching descriptor was filtered out")?;
        let ControlStreamItem::Descriptor { surface, .. } = filtered else {
            return Err("expected descriptor".into());
        };
        assert_eq!(surface.controls.len(), 1);
        assert_eq!(
            surface.controls.first().map(|control| control.kind),
            Some(ControlKind::Button)
        );

        let encoder = ControlStreamItem::Event {
            meta: StreamMeta::default(),
            device: device(),
            event: ControlEvent {
                raw_id: RawControlId::encoder(0),
                value: ControlValue::Encoder(1),
                delta: Some(1),
            },
        };
        assert!(filter_item(encoder, &request, &kinds).is_none());
        Ok(())
    }

    #[test]
    fn emits_metadata_and_disconnect_variant_without_action_claims() -> TestResult {
        let item = ControlStreamItem::Reset {
            meta: StreamMeta {
                seq: 9,
                timestamp_ns: 10,
                epoch: 2,
            },
            device: device(),
            reason: ResetReason::Disconnect,
        };
        let wire = to_wire_item(item)?;
        let metadata = wire.metadata.as_ref().ok_or("missing metadata")?;
        assert_eq!(metadata.sequence, 9);
        assert_eq!(metadata.epoch, 2);
        assert_eq!(
            metadata.device.as_ref().map(|d| d.logical_id.as_str()),
            Some("wire-device")
        );
        assert!(matches!(
            wire.item,
            Some(WireControlStreamItemVariant::Disconnect(_))
        ));
        Ok(())
    }
}
