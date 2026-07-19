//! `wheelctl controls` — observe-only control-stream diagnostics, capture, and
//! replay (issue #172).
//!
//! These commands make the generic control stream observable and reproducible
//! **without real hardware**. `replay` (and the human `monitor` view) feed
//! recorded decoded inputs through the *real* [`ControlProjector`] — the same
//! deterministic projection the service collector uses — rather than a separate
//! ad-hoc format, so a capture reproduces the same logical stream every time.
//!
//! Everything here is read-only: no device is opened, no FFB/output is produced,
//! and observing a control never establishes a named physical role. Semantic
//! (named) control identities are **not** invented here — validated roles come
//! from the lane's evidence/receipt process, so `list` reports raw controls with
//! `raw` provenance only.
//!
//! Captures use a small versioned JSON schema
//! ([`ControlCapture`], `schema_version` [`CONTROL_CAPTURE_SCHEMA_VERSION`]) that
//! records the surface identity, mapping version, and an ordered list of input
//! frames (each an optional decoded snapshot and/or an explicit reset), plus
//! sequence/timestamp metadata carried by the projected items. Live capture from
//! a running `wheeld` over the versioned gRPC stream (#171) is a follow-up; this
//! command group is the deterministic, hardware-free core.

use anyhow::{Context, Result};
use clap::Subcommand;
use openracing_device_types::{
    ControlProjector, ControlStreamItem, DeviceIdentity, DeviceInputs, ResetReason,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version of the control-capture JSON schema emitted and accepted here.
pub const CONTROL_CAPTURE_SCHEMA_VERSION: u32 = 1;

/// Mapping/contract version stamped onto generated sample captures.
const SAMPLE_MAPPING_VERSION: u32 = 1;

/// `wheelctl controls` subcommands.
#[derive(Subcommand, Debug)]
pub enum ControlsCommands {
    /// List the stable control descriptors a profile may bind for a surface.
    ///
    /// Reads the surface identity from a capture (or uses a default virtual
    /// profile) and prints each raw control id and kind. Semantic status is
    /// always `raw` here; validated named roles come from the lane's evidence
    /// process, not this observe-only tool.
    List {
        /// Capture file whose surface identity/mapping to describe.
        #[arg(long)]
        capture: Option<PathBuf>,
        /// Write the machine-readable listing to this JSON file.
        #[arg(long)]
        json_out: Option<PathBuf>,
    },

    /// Replay a capture and show ordered descriptor/baseline/event/reset items
    /// as a human-readable stream, visibly reporting resets and epoch changes.
    Monitor {
        /// Capture file to replay through the projection.
        capture: PathBuf,
    },

    /// Write a deterministic sample capture (virtual input; no hardware).
    Capture {
        /// Output path for the capture JSON.
        #[arg(long)]
        out: PathBuf,
    },

    /// Replay a capture's inputs through the real projection without hardware
    /// and emit the resulting ordered stream items.
    Replay {
        /// Capture file to replay.
        capture: PathBuf,
        /// Write the projected stream items to this JSON file.
        #[arg(long)]
        json_out: Option<PathBuf>,
    },
}

/// A versioned, serializable control-input capture.
///
/// Frames are replayed in order through a [`ControlProjector`] to reproduce the
/// exact logical stream. This records *inputs* (not projected items) precisely
/// so that replay exercises the real projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlCapture {
    /// Schema version for forward/backward compatibility.
    pub schema_version: u32,
    /// Stable identity of the captured surface.
    pub device: DeviceIdentity,
    /// Mapping/contract version the capture reflects.
    pub mapping_version: u32,
    /// Ordered input frames.
    pub frames: Vec<CaptureFrame>,
}

/// One ordered entry in a [`ControlCapture`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureFrame {
    /// Monotonic source timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Explicit reset injected before any inputs on this frame (disconnect,
    /// reconnect, gap/overflow, epoch change). Absent for ordinary frames.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<ResetReason>,
    /// Decoded input snapshot to project. Absent when the frame only carries a
    /// reset marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<CapturedInputs>,
}

/// Serializable mirror of [`DeviceInputs`] used inside captures.
///
/// `DeviceInputs` lives in a foundational crate without a serde dependency;
/// this local mirror keeps the capture format self-contained in the CLI.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CapturedInputs {
    #[serde(default)]
    pub tick: u32,
    #[serde(default)]
    pub buttons: [u8; 16],
    #[serde(default)]
    pub hat: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steering: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brake: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clutch_left: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clutch_right: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clutch_combined: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clutch_left_button: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clutch_right_button: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handbrake: Option<u16>,
    #[serde(default)]
    pub rotaries: [i16; 8],
}

impl CapturedInputs {
    /// Convert to the canonical [`DeviceInputs`] domain type.
    fn to_device_inputs(self) -> DeviceInputs {
        DeviceInputs {
            tick: self.tick,
            buttons: self.buttons,
            hat: self.hat,
            steering: self.steering,
            throttle: self.throttle,
            brake: self.brake,
            clutch_left: self.clutch_left,
            clutch_right: self.clutch_right,
            clutch_combined: self.clutch_combined,
            clutch_left_button: self.clutch_left_button,
            clutch_right_button: self.clutch_right_button,
            handbrake: self.handbrake,
            rotaries: self.rotaries,
        }
    }

    /// Build a captured frame's inputs from a [`DeviceInputs`] snapshot.
    fn from_device_inputs(inputs: DeviceInputs) -> Self {
        Self {
            tick: inputs.tick,
            buttons: inputs.buttons,
            hat: inputs.hat,
            steering: inputs.steering,
            throttle: inputs.throttle,
            brake: inputs.brake,
            clutch_left: inputs.clutch_left,
            clutch_right: inputs.clutch_right,
            clutch_combined: inputs.clutch_combined,
            clutch_left_button: inputs.clutch_left_button,
            clutch_right_button: inputs.clutch_right_button,
            handbrake: inputs.handbrake,
            rotaries: inputs.rotaries,
        }
    }
}

/// A default virtual surface identity used when no capture is supplied.
fn virtual_surface() -> DeviceIdentity {
    DeviceIdentity {
        logical_id: "virtual-controls".to_string(),
        vendor_id: 0x1234,
        product_id: 0x5678,
        serial: Some("VIRTUAL-CONTROLS".to_string()),
        instance: 1,
    }
}

/// Project a whole capture through the real [`ControlProjector`], returning the
/// ordered stream items exactly as a live consumer would observe them.
///
/// A descriptor is emitted first, then each frame's optional reset and inputs
/// are applied in order. This is the single projection path shared by `replay`
/// and `monitor`, so both reproduce identical logical streams.
pub fn project_capture(capture: &ControlCapture) -> Vec<ControlStreamItem> {
    let mut projector = ControlProjector::new(capture.device.clone(), capture.mapping_version);
    let mut items = Vec::new();

    let first_ts = capture.frames.first().map_or(0, |f| f.timestamp_ns);
    items.push(projector.descriptor(first_ts));

    for frame in &capture.frames {
        if let Some(reason) = frame.reset {
            items.push(projector.reset(reason, frame.timestamp_ns));
        }
        if let Some(inputs) = frame.inputs {
            items.extend(projector.observe(&inputs.to_device_inputs(), frame.timestamp_ns));
        }
    }

    items
}

/// Load and validate a capture from disk.
fn load_capture(path: &Path) -> Result<ControlCapture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read control capture {}", path.display()))?;
    let capture: ControlCapture = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse control capture {}", path.display()))?;
    if capture.schema_version != CONTROL_CAPTURE_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported control capture schema_version {} (expected {})",
            capture.schema_version,
            CONTROL_CAPTURE_SCHEMA_VERSION
        );
    }
    Ok(capture)
}

/// A deterministic sample capture exercising the required fixture cases:
/// baseline, button press/release, hat direction + neutral, multiple encoder
/// ticks, and an explicit reset/reconnect.
pub fn sample_capture() -> ControlCapture {
    let mut frames = Vec::new();
    let mut ts = 1_000u64;
    let push = |frames: &mut Vec<CaptureFrame>, ts: &mut u64, inputs: DeviceInputs| {
        frames.push(CaptureFrame {
            timestamp_ns: *ts,
            reset: None,
            inputs: Some(CapturedInputs::from_device_inputs(inputs)),
        });
        *ts += 1_000;
    };

    // Baseline frame (no actions synthesized).
    push(&mut frames, &mut ts, DeviceInputs::default());

    // Button press then release.
    let mut pressed = DeviceInputs::default();
    pressed.set_button(3, true);
    push(&mut frames, &mut ts, pressed);
    push(&mut frames, &mut ts, DeviceInputs::default());

    // Hat direction then neutral.
    let hat_right = DeviceInputs {
        hat: 2,
        ..DeviceInputs::default()
    };
    push(&mut frames, &mut ts, hat_right);
    let hat_neutral = DeviceInputs {
        hat: 0xFF,
        ..DeviceInputs::default()
    };
    push(&mut frames, &mut ts, hat_neutral);

    // Multiple encoder ticks (lossless accumulation): +2 then +1.
    let mut enc_a = DeviceInputs::default();
    enc_a.rotaries[0] = 2;
    push(&mut frames, &mut ts, enc_a);
    let mut enc_b = DeviceInputs::default();
    enc_b.rotaries[0] = 1;
    push(&mut frames, &mut ts, enc_b);

    // Explicit reset (disconnect), then reconnect baseline.
    frames.push(CaptureFrame {
        timestamp_ns: ts,
        reset: Some(ResetReason::Disconnect),
        inputs: None,
    });
    ts += 1_000;
    frames.push(CaptureFrame {
        timestamp_ns: ts,
        reset: Some(ResetReason::Reconnect),
        inputs: Some(CapturedInputs::from_device_inputs(DeviceInputs::default())),
    });

    ControlCapture {
        schema_version: CONTROL_CAPTURE_SCHEMA_VERSION,
        device: virtual_surface(),
        mapping_version: SAMPLE_MAPPING_VERSION,
        frames,
    }
}

/// Dispatch a `controls` subcommand.
pub async fn execute(cmd: &ControlsCommands, json: bool, _endpoint: Option<&str>) -> Result<()> {
    match cmd {
        ControlsCommands::List { capture, json_out } => {
            run_list(capture.as_deref(), json, json_out.as_deref())
        }
        ControlsCommands::Monitor { capture } => run_monitor(capture),
        ControlsCommands::Capture { out } => run_capture(out, json),
        ControlsCommands::Replay { capture, json_out } => {
            run_replay(capture, json, json_out.as_deref())
        }
    }
}

/// A single bindable control descriptor rendered for `list`.
#[derive(Debug, Serialize)]
struct ListedControl {
    raw_id: u32,
    kind: String,
    semantic: Option<String>,
    status: String,
}

fn run_list(capture: Option<&Path>, json: bool, json_out: Option<&Path>) -> Result<()> {
    let (device, mapping_version) = match capture {
        Some(path) => {
            let cap = load_capture(path)?;
            (cap.device, cap.mapping_version)
        }
        None => (virtual_surface(), SAMPLE_MAPPING_VERSION),
    };

    // The descriptor enumerates exactly the control ids a profile may bind.
    let mut projector = ControlProjector::new(device.clone(), mapping_version);
    let descriptor_item = projector.descriptor(0);
    let controls = match &descriptor_item {
        ControlStreamItem::Descriptor { surface, .. } => surface
            .controls
            .iter()
            .map(|c| ListedControl {
                raw_id: c.raw_id.0,
                kind: format!("{:?}", c.kind),
                semantic: c.semantic.as_ref().map(|s| s.label.clone()),
                status: c
                    .semantic
                    .as_ref()
                    .map_or_else(|| "raw".to_string(), |s| format!("{:?}", s.status)),
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    if let Some(path) = json_out {
        write_json(path, &controls)?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&controls)?);
    } else {
        println!(
            "surface {:04x}:{:04x} (instance {}) — {} bindable controls, mapping v{}",
            device.vendor_id,
            device.product_id,
            device.instance,
            controls.len(),
            mapping_version
        );
        for c in &controls {
            match &c.semantic {
                Some(label) => {
                    println!(
                        "  raw {:#010x}  {:<8} {} [{}]",
                        c.raw_id, c.kind, label, c.status
                    )
                }
                None => println!("  raw {:#010x}  {:<8} (raw-only)", c.raw_id, c.kind),
            }
        }
        println!("note: semantic roles are validated only through the lane's evidence process.");
    }
    Ok(())
}

fn run_replay(capture: &Path, json: bool, json_out: Option<&Path>) -> Result<()> {
    let cap = load_capture(capture)?;
    let items = project_capture(&cap);

    if let Some(path) = json_out {
        write_json(path, &items)?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        println!(
            "replayed {} frames -> {} stream items from {}",
            cap.frames.len(),
            items.len(),
            capture.display()
        );
        for item in &items {
            println!("  {}", describe_item(item));
        }
    }
    Ok(())
}

fn run_monitor(capture: &Path) -> Result<()> {
    let cap = load_capture(capture)?;
    let items = project_capture(&cap);
    println!(
        "monitoring replay of {} ({} items) — observe-only, no hardware",
        capture.display(),
        items.len()
    );
    let mut last_epoch: Option<u32> = None;
    for item in &items {
        let meta = item.meta();
        if last_epoch != Some(meta.epoch) {
            if last_epoch.is_some() {
                println!("--- epoch {} (stream reset / new baseline) ---", meta.epoch);
            }
            last_epoch = Some(meta.epoch);
        }
        println!(
            "  seq {:>4} @ {:>12}ns  {}",
            meta.seq,
            meta.timestamp_ns,
            describe_item(item)
        );
    }
    Ok(())
}

fn run_capture(out: &Path, json: bool) -> Result<()> {
    let capture = sample_capture();
    write_json(out, &capture)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&capture)?);
    } else {
        println!(
            "wrote deterministic sample capture ({} frames) to {}",
            capture.frames.len(),
            out.display()
        );
    }
    Ok(())
}

/// One-line human description of a stream item.
fn describe_item(item: &ControlStreamItem) -> String {
    match item {
        ControlStreamItem::Descriptor { surface, .. } => {
            format!("descriptor: {} controls", surface.controls.len())
        }
        ControlStreamItem::InitialSnapshot { states, .. } => {
            format!("baseline: {} control states (non-actionable)", states.len())
        }
        ControlStreamItem::Event { event, .. } => match event.delta {
            Some(delta) => format!(
                "event: raw {:#010x} = {:?} (delta {})",
                event.raw_id.0, event.value, delta
            ),
            None => format!("event: raw {:#010x} = {:?}", event.raw_id.0, event.value),
        },
        ControlStreamItem::Reset { reason, .. } => format!("reset: {reason:?}"),
    }
}

/// Serialize `value` as pretty JSON to `path`.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_capture_roundtrips_through_json() -> Result<()> {
        let capture = sample_capture();
        let json = serde_json::to_string(&capture)?;
        let back: ControlCapture = serde_json::from_str(&json)?;
        assert_eq!(back.schema_version, CONTROL_CAPTURE_SCHEMA_VERSION);
        assert_eq!(back.frames.len(), capture.frames.len());
        // Projecting either yields the identical logical stream (determinism).
        assert_eq!(project_capture(&capture), project_capture(&back));
        Ok(())
    }

    #[test]
    fn replay_emits_descriptor_baseline_then_actions() -> Result<()> {
        let items = project_capture(&sample_capture());
        // First item is the descriptor, second is the non-actionable baseline.
        assert!(matches!(items[0], ControlStreamItem::Descriptor { .. }));
        assert!(matches!(
            items[1],
            ControlStreamItem::InitialSnapshot { .. }
        ));
        // The stream contains at least one actionable event and one reset.
        assert!(
            items
                .iter()
                .any(|i| matches!(i, ControlStreamItem::Event { .. }))
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, ControlStreamItem::Reset { .. }))
        );
        Ok(())
    }

    #[test]
    fn encoder_ticks_are_lossless_across_frames() -> Result<()> {
        // The sample capture applies +2 then +1 on encoder 0; the replayed
        // deltas must sum to +3 with no collapse.
        let items = project_capture(&sample_capture());
        let total: i32 = items
            .iter()
            .filter_map(|i| match i {
                ControlStreamItem::Event { event, .. } => event.delta,
                _ => None,
            })
            .sum();
        assert_eq!(total, 3);
        Ok(())
    }

    #[test]
    fn reset_starts_a_new_epoch() -> Result<()> {
        let items = project_capture(&sample_capture());
        let max_epoch = items.iter().map(|i| i.meta().epoch).max().unwrap_or(0);
        assert!(
            max_epoch >= 1,
            "a disconnect/reconnect must advance the epoch"
        );
        Ok(())
    }

    #[test]
    fn list_reports_raw_controls_for_virtual_surface() {
        let mut projector = ControlProjector::new(virtual_surface(), SAMPLE_MAPPING_VERSION);
        let descriptor = projector.descriptor(0);
        assert!(matches!(descriptor, ControlStreamItem::Descriptor { .. }));
        if let ControlStreamItem::Descriptor { surface, .. } = descriptor {
            // 128 buttons + 1 hat + 8 encoders.
            assert_eq!(surface.controls.len(), 128 + 1 + 8);
            assert!(surface.controls.iter().all(|c| c.semantic.is_none()));
        }
    }
}
