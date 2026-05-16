//! Synthetic capture builders for each supported vendor.
//!
//! All captures produced here are **machine-generated** and marked with
//! `synthetic: true` in their metadata.  They exercise the protocol parsers
//! with structurally valid byte layouts but do **not** originate from real
//! hardware.

use crate::{CaptureMetadata, CaptureRecord, CaptureSession, DeviceId, Direction};

/// Known vendor/product pairs for synthetic capture generation.
struct VendorSpec {
    vid: u16,
    pid: u16,
    name: &'static str,
    /// Report ID used for standard input reports.
    input_report_id: u8,
    /// Minimum payload length for the input report (excluding report ID).
    input_len: usize,
    /// Builder that fills a payload buffer with structurally valid data.
    /// `frame` is 0-based and can be used to vary the data per record.
    fill: fn(frame: usize, buf: &mut [u8]),
}

// ── Fill functions ───────────────────────────────────────────────────────────

/// Moza: steering i16 LE at \[0..2\], throttle u8 at \[2\], brake u8 at \[3\].
fn fill_moza(frame: usize, buf: &mut [u8]) {
    let steering = ((frame as i16).wrapping_mul(100)) % 10000;
    let bytes = steering.to_le_bytes();
    buf[0] = bytes[0];
    buf[1] = bytes[1];
    buf[2] = (frame % 256) as u8; // throttle
    buf[3] = ((frame * 3) % 256) as u8; // brake
}

/// Fanatec: report_id 0x01 in first byte of the *report* (handled externally),
/// steering u16 LE at [0..2], inverted axes at [2..5].
fn fill_fanatec(frame: usize, buf: &mut [u8]) {
    let steering = 0x8000u16.wrapping_add((frame as u16).wrapping_mul(50));
    let bytes = steering.to_le_bytes();
    buf[0] = bytes[0];
    buf[1] = bytes[1];
    // Inverted axes: 0xFF = released
    buf[2] = 0xFFu8.wrapping_sub((frame % 256) as u8); // throttle
    buf[3] = 0xFFu8.wrapping_sub(((frame * 2) % 256) as u8); // brake
    buf[4] = 0xFF; // clutch released
    buf[5] = 0x00; // padding
    buf[6] = 0x00; // buttons lo
    buf[7] = 0x00; // buttons hi
    buf[8] = 0x0F; // hat neutral
}

/// Thrustmaster: report_id 0x01, steering u16 LE at [0..2], axes at [2..5].
fn fill_thrustmaster(frame: usize, buf: &mut [u8]) {
    let steering = 0x8000u16.wrapping_add((frame as u16).wrapping_mul(30));
    let bytes = steering.to_le_bytes();
    buf[0] = bytes[0];
    buf[1] = bytes[1];
    buf[2] = (frame % 256) as u8; // throttle
    buf[3] = ((frame * 2) % 256) as u8; // brake
    buf[4] = 0; // clutch
    buf[5] = 0; // buttons lo
    buf[6] = 0; // buttons hi
    buf[7] = 0x0F; // hat neutral
    buf[8] = 0; // paddles
}

/// Simagic: steering u16 LE at [0..2], throttle/brake u16 LE, 17 bytes min.
fn fill_simagic(frame: usize, buf: &mut [u8]) {
    let steering = 0x8000u16.wrapping_add((frame as u16).wrapping_mul(40));
    let s = steering.to_le_bytes();
    buf[0] = s[0];
    buf[1] = s[1];
    let throttle = ((frame * 256) % 65536) as u16;
    let t = throttle.to_le_bytes();
    buf[2] = t[0];
    buf[3] = t[1];
    // brake, clutch, handbrake = 0
}

/// Cammus: steering i16 LE at [0..2], throttle u16 LE at [2..4], brake u16 LE at [4..6],
/// buttons at [6..8], clutch u16 LE at [8..10], handbrake u16 LE at [10..12].
fn fill_cammus(frame: usize, buf: &mut [u8]) {
    let steering = ((frame as i16).wrapping_mul(200)) % 30000;
    let s = steering.to_le_bytes();
    buf[0] = s[0];
    buf[1] = s[1];
    let throttle = ((frame * 512) % 65536) as u16;
    let t = throttle.to_le_bytes();
    buf[2] = t[0];
    buf[3] = t[1];
    let brake = ((frame * 300) % 65536) as u16;
    let b = brake.to_le_bytes();
    buf[4] = b[0];
    buf[5] = b[1];
    buf[6] = 0; // buttons lo
    buf[7] = 0; // buttons hi
    // clutch and handbrake = 0
}

/// VRS: steering i16 LE at [0..2], throttle u16 LE at [2..4], 17 bytes min.
fn fill_vrs(frame: usize, buf: &mut [u8]) {
    let steering = ((frame as i16).wrapping_mul(150)) % 20000;
    let s = steering.to_le_bytes();
    buf[0] = s[0];
    buf[1] = s[1];
    let throttle = ((frame * 400) % 65536) as u16;
    let t = throttle.to_le_bytes();
    buf[2] = t[0];
    buf[3] = t[1];
}

/// Leo Bodnar: generic button-box style payload (no standard input parse).
fn fill_leo_bodnar(frame: usize, buf: &mut [u8]) {
    // Button states cycling through patterns
    buf[0] = (frame % 256) as u8;
    buf[1] = ((frame / 256) % 256) as u8;
}

/// Cube Controls: generic button/rim input (no standard input parse).
fn fill_cube_controls(frame: usize, buf: &mut [u8]) {
    buf[0] = (frame % 256) as u8;
    buf[1] = ((frame * 7) % 256) as u8;
}

// ── Vendor table ─────────────────────────────────────────────────────────────

const VENDORS: &[VendorSpec] = &[
    VendorSpec {
        vid: 0x346E,
        pid: 0x0004,
        name: "Moza R9 V2 (synthetic)",
        input_report_id: 0x01,
        input_len: 63,
        fill: fill_moza,
    },
    VendorSpec {
        vid: 0x0EB7,
        pid: 0x0001,
        name: "Fanatec CSL DD (synthetic)",
        input_report_id: 0x01,
        input_len: 63,
        fill: fill_fanatec,
    },
    VendorSpec {
        vid: 0x044F,
        pid: 0x0001,
        name: "Thrustmaster T300RS (synthetic)",
        input_report_id: 0x01,
        input_len: 63,
        fill: fill_thrustmaster,
    },
    VendorSpec {
        vid: 0x3670,
        pid: 0x0500,
        name: "Simagic EVO Sport (synthetic)",
        input_report_id: 0x00,
        input_len: 63,
        fill: fill_simagic,
    },
    VendorSpec {
        vid: 0x3416,
        pid: 0x0301,
        name: "Cammus C5 (synthetic)",
        input_report_id: 0x01,
        input_len: 63,
        fill: fill_cammus,
    },
    VendorSpec {
        vid: 0x0483,
        pid: 0xA355,
        name: "VRS DirectForce Pro (synthetic)",
        input_report_id: 0x00,
        input_len: 63,
        fill: fill_vrs,
    },
    VendorSpec {
        vid: 0x1DD2,
        pid: 0x000E,
        name: "Leo Bodnar Wheel Interface (synthetic)",
        input_report_id: 0x01,
        input_len: 31,
        fill: fill_leo_bodnar,
    },
    VendorSpec {
        vid: 0x0483,
        pid: 0x0C73,
        name: "Cube Controls GT Pro (synthetic)",
        input_report_id: 0x01,
        input_len: 31,
        fill: fill_cube_controls,
    },
];

/// Build a synthetic [`CaptureSession`] for a known vendor.
///
/// `vendor_name` is matched case-insensitively against the short vendor names:
/// `moza`, `fanatec`, `thrustmaster`, `simagic`, `cammus`, `vrs`,
/// `leo_bodnar` / `leobodnar`, `cube_controls` / `cubecontrols`.
///
/// Returns `None` if the vendor is not recognised.
#[must_use]
pub fn build_synthetic_session(vendor_name: &str, num_records: usize) -> Option<CaptureSession> {
    let spec = find_vendor(vendor_name)?;
    Some(build_session(spec, num_records))
}

fn find_vendor(name: &str) -> Option<&'static VendorSpec> {
    let name_lower = name.to_ascii_lowercase();
    let name_lower = name_lower.as_str();
    VENDORS.iter().find(|v| {
        let vname = v.name.to_ascii_lowercase();
        match name_lower {
            "moza" => vname.contains("moza"),
            "fanatec" => vname.contains("fanatec"),
            "thrustmaster" => vname.contains("thrustmaster"),
            "simagic" => vname.contains("simagic"),
            "cammus" => vname.contains("cammus"),
            "vrs" => vname.contains("vrs"),
            "leo_bodnar" | "leobodnar" | "leo bodnar" => vname.contains("leo bodnar"),
            "cube_controls" | "cubecontrols" | "cube controls" => vname.contains("cube controls"),
            _ => false,
        }
    })
}

fn build_session(spec: &VendorSpec, num_records: usize) -> CaptureSession {
    let interval_ns: u64 = 1_000_000; // 1 ms (1 kHz)
    let mut records = Vec::with_capacity(num_records);

    for i in 0..num_records {
        let mut payload = vec![0u8; spec.input_len];
        (spec.fill)(i, &mut payload);

        records.push(CaptureRecord {
            timestamp_ns: (i as u64) * interval_ns,
            direction: Direction::DeviceToHost,
            report_id: spec.input_report_id,
            payload,
        });
    }

    CaptureSession {
        device: DeviceId {
            vid: spec.vid,
            pid: spec.pid,
            name: Some(spec.name.to_owned()),
        },
        metadata: CaptureMetadata::synthetic(spec.name),
        records,
    }
}

/// Return the list of supported vendor short names.
#[must_use]
pub fn supported_vendors() -> &'static [&'static str] {
    &[
        "moza",
        "fanatec",
        "thrustmaster",
        "simagic",
        "cammus",
        "vrs",
        "leo_bodnar",
        "cube_controls",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_vendors_produce_sessions() -> Result<(), String> {
        for vendor in supported_vendors() {
            let session = build_synthetic_session(vendor, 10)
                .ok_or_else(|| format!("vendor {vendor} returned None"))?;
            assert_eq!(session.records.len(), 10);
            assert!(session.metadata.synthetic);
            session
                .validate_timestamps()
                .map_err(|e| format!("vendor {vendor}: {e}"))?;
        }
        Ok(())
    }

    #[test]
    fn unknown_vendor_returns_none() -> Result<(), String> {
        assert!(build_synthetic_session("nonexistent", 5).is_none());
        Ok(())
    }

    #[test]
    fn records_have_monotonic_timestamps() -> Result<(), String> {
        let session = build_synthetic_session("cammus", 100)
            .ok_or_else(|| "cammus returned None".to_owned())?;
        session.validate_timestamps().map_err(|e| format!("{e}"))?;
        Ok(())
    }

    #[test]
    fn synthetic_metadata_flag() -> Result<(), String> {
        let session =
            build_synthetic_session("moza", 1).ok_or_else(|| "moza returned None".to_owned())?;
        assert!(session.metadata.synthetic);
        assert_eq!(session.metadata.format_version, "1.0");
        Ok(())
    }

    /// Helper: build a session with one record and return its first payload.
    fn first_payload(vendor: &str) -> Result<Vec<u8>, String> {
        let session = build_synthetic_session(vendor, 1)
            .ok_or_else(|| format!("vendor {vendor} returned None"))?;
        let record = session
            .records
            .first()
            .ok_or_else(|| format!("vendor {vendor} produced no records"))?;
        Ok(record.payload.clone())
    }

    #[test]
    fn moza_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0, throttle=0, brake=0
        let payload = first_payload("moza")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(&payload[0..4], &[0x00, 0x00, 0x00, 0x00]);
        // Remaining bytes are zero.
        assert!(payload[4..].iter().all(|b| *b == 0));
        Ok(())
    }

    #[test]
    fn fanatec_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0x8000 LE, throttle/brake/clutch released=0xFF,
        // padding+buttons=0, hat=0x0F.
        let payload = first_payload("fanatec")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(
            &payload[0..9],
            &[0x00, 0x80, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x0F]
        );
        Ok(())
    }

    #[test]
    fn thrustmaster_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0x8000 LE, axes/buttons=0, hat=0x0F, paddles=0.
        let payload = first_payload("thrustmaster")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(
            &payload[0..9],
            &[0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0x00]
        );
        Ok(())
    }

    #[test]
    fn simagic_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0x8000 LE, throttle=0.
        let payload = first_payload("simagic")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(&payload[0..4], &[0x00, 0x80, 0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn cammus_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0, throttle=0, brake=0, buttons=0.
        let payload = first_payload("cammus")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(
            &payload[0..8],
            &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        Ok(())
    }

    #[test]
    fn vrs_first_record_byte_layout() -> Result<(), String> {
        // frame=0: steering=0, throttle=0.
        let payload = first_payload("vrs")?;
        assert_eq!(payload.len(), 63);
        assert_eq!(&payload[0..4], &[0x00, 0x00, 0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn leo_bodnar_first_record_byte_layout() -> Result<(), String> {
        // frame=0: buf[0]=0, buf[1]=0.
        let payload = first_payload("leo_bodnar")?;
        assert_eq!(payload.len(), 31);
        assert_eq!(&payload[0..2], &[0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn cube_controls_first_record_byte_layout() -> Result<(), String> {
        // frame=0: buf[0]=0, buf[1]=0.
        let payload = first_payload("cube_controls")?;
        assert_eq!(payload.len(), 31);
        assert_eq!(&payload[0..2], &[0x00, 0x00]);
        Ok(())
    }

    #[test]
    fn vendor_matching_is_case_insensitive() -> Result<(), String> {
        for vendor in ["MOZA", "Fanatec", "ThrustMaster", "Leo_Bodnar"] {
            assert!(
                build_synthetic_session(vendor, 1).is_some(),
                "vendor {vendor} should resolve case-insensitively"
            );
        }
        Ok(())
    }

    #[test]
    fn vendor_aliases_resolve() -> Result<(), String> {
        // Underscore, no-separator, and space variants all resolve.
        for vendor in [
            "leobodnar",
            "leo bodnar",
            "leo_bodnar",
            "cubecontrols",
            "cube controls",
            "cube_controls",
        ] {
            assert!(
                build_synthetic_session(vendor, 1).is_some(),
                "alias {vendor} should resolve"
            );
        }
        Ok(())
    }

    #[test]
    fn zero_record_session_is_empty() -> Result<(), String> {
        let session = build_synthetic_session("fanatec", 0)
            .ok_or_else(|| "fanatec returned None".to_owned())?;
        assert!(session.records.is_empty());
        assert!(session.metadata.synthetic);
        Ok(())
    }

    #[test]
    fn all_records_flow_device_to_host() -> Result<(), String> {
        for vendor in supported_vendors() {
            let session = build_synthetic_session(vendor, 5)
                .ok_or_else(|| format!("vendor {vendor} returned None"))?;
            for record in &session.records {
                assert_eq!(
                    record.direction,
                    Direction::DeviceToHost,
                    "vendor {vendor} produced a non-device-to-host record"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn successive_records_have_distinct_payloads() -> Result<(), String> {
        // cube_controls frame=0 → [0, 0]; frame=1 → [1, 7]. Confirms the fill
        // function actually varies with the frame index.
        let session = build_synthetic_session("cube_controls", 2)
            .ok_or_else(|| "cube_controls returned None".to_owned())?;
        assert_eq!(session.records.len(), 2);
        assert_eq!(&session.records[0].payload[0..2], &[0x00, 0x00]);
        assert_eq!(&session.records[1].payload[0..2], &[0x01, 0x07]);
        assert_ne!(session.records[0].payload, session.records[1].payload);
        Ok(())
    }
}
