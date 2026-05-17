//! Integration tests for newer game telemetry adapters added during the RC sprint.
//!
//! Covers: ETS2/ATS, Wreckfest, Rennsport, WRC Generations, Dirt 4, Project CARS 2, LFS.
//!
//! Each game section provides:
//!   - A happy-path parse test (valid minimal packet → expected `TelemetryFrame` fields)
//!   - Truncated / malformed-packet tests (graceful `Err`, no panic)
//!   - A proptest fuzz section (≥256 random-byte cases → never panics)

use openracing_telemetry_adapters::{
    DakarDesertRallyAdapter, Dirt4Adapter, Ets2Adapter, FlatOutAdapter, LFSAdapter, PCars2Adapter,
    RennsportAdapter, TelemetryAdapter, WrcGenerationsAdapter, WreckfestAdapter, ets2::Ets2Variant,
};

mod helpers;
use helpers::write_f32_le;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Byte-write helpers ───────────────────────────────────────────────────────

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ─── ETS2 / ATS ──────────────────────────────────────────────────────────────
//
// SCS Telemetry SDK v1.14 layout (512-byte shared-memory snapshot):
//   offset 0  u32  version (must be 1)
//   offset 4  f32  speed_ms
//   offset 8  f32  rpm
//   offset 12 i32  gear (>0 forward, <0 reverse, 0 neutral)
//   offset 16 f32  fuel_ratio
//   offset 20 f32  engine_load

fn make_scs_packet(speed: f32, rpm: f32, gear: i32, fuel: f32, load: f32) -> Vec<u8> {
    let mut data = vec![0u8; 512];
    write_u32_le(&mut data, 0, 1);
    write_f32_le(&mut data, 4, speed);
    write_f32_le(&mut data, 8, rpm);
    write_i32_le(&mut data, 12, gear);
    write_f32_le(&mut data, 16, fuel);
    write_f32_le(&mut data, 20, load);
    data
}

#[test]
fn ets2_happy_path_parses_fields() -> TestResult {
    let pkt = make_scs_packet(20.0, 1500.0, 4, 0.7, 0.5);
    let t = Ets2Adapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 20.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 1500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 4, "gear={}", t.gear);
    assert!(
        (t.fuel_percent - 0.7).abs() < 0.001,
        "fuel={}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn ets2_empty_packet_returns_error() -> TestResult {
    assert!(Ets2Adapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn ets2_short_packet_returns_error() -> TestResult {
    assert!(Ets2Adapter::new().normalize(&[0u8; 10]).is_err());
    Ok(())
}

#[test]
fn ets2_wrong_version_returns_error() -> TestResult {
    let mut pkt = make_scs_packet(10.0, 1000.0, 1, 0.5, 0.3);
    write_u32_le(&mut pkt, 0, 2); // version 2 ≠ expected 1
    assert!(Ets2Adapter::new().normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn ets2_reverse_gear_maps_to_minus_one() -> TestResult {
    let pkt = make_scs_packet(0.0, 800.0, -1, 0.5, 0.2);
    let t = Ets2Adapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn ets2_neutral_gear_maps_to_zero() -> TestResult {
    let pkt = make_scs_packet(0.0, 700.0, 0, 0.9, 0.1);
    let t = Ets2Adapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn ats_variant_happy_path_parses_fields() -> TestResult {
    let pkt = make_scs_packet(15.0, 1200.0, 3, 0.6, 0.4);
    let t = Ets2Adapter::with_variant(Ets2Variant::Ats).normalize(&pkt)?;
    assert!((t.rpm - 1200.0).abs() < 0.1);
    assert_eq!(t.gear, 3);
    Ok(())
}

// ─── Wreckfest ───────────────────────────────────────────────────────────────
//
// UDP port 5606, WRKF magic (4 bytes), then:
//   offset 4  u32  sequence
//   offset 8  f32  speed_ms
//   offset 12 f32  rpm
//   offset 16 u8   gear  (0=neutral, 1+=forward)
//   offset 20 f32  lateral_g
//   offset 24 f32  longitudinal_g

const WRKF_MAGIC: [u8; 4] = [0x57, 0x52, 0x4B, 0x46];

fn make_wreckfest_packet(speed: f32, rpm: f32, gear: u8, lat_g: f32, lon_g: f32) -> Vec<u8> {
    let mut data = vec![0u8; 28];
    data[0..4].copy_from_slice(&WRKF_MAGIC);
    write_f32_le(&mut data, 8, speed);
    write_f32_le(&mut data, 12, rpm);
    data[16] = gear;
    write_f32_le(&mut data, 20, lat_g);
    write_f32_le(&mut data, 24, lon_g);
    data
}

#[test]
fn wreckfest_happy_path_parses_fields() -> TestResult {
    let pkt = make_wreckfest_packet(30.0, 4000.0, 3, 0.5, 0.2);
    let t = WreckfestAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 4000.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 3, "gear={}", t.gear);
    assert!(
        (t.lateral_g - 0.5).abs() < 0.001,
        "lateral_g={}",
        t.lateral_g
    );
    Ok(())
}

#[test]
fn wreckfest_empty_packet_returns_error() -> TestResult {
    assert!(WreckfestAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn wreckfest_short_packet_returns_error() -> TestResult {
    assert!(WreckfestAdapter::new().normalize(&[0u8; 10]).is_err());
    Ok(())
}

#[test]
fn wreckfest_bad_magic_returns_error() -> TestResult {
    let mut pkt = make_wreckfest_packet(10.0, 2000.0, 2, 0.0, 0.0);
    pkt[0] = 0xFF; // corrupt first magic byte
    assert!(WreckfestAdapter::new().normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn wreckfest_ffb_scalar_stays_in_range() -> TestResult {
    let pkt = make_wreckfest_packet(60.0, 7000.0, 5, 2.0, 1.5);
    let t = WreckfestAdapter::new().normalize(&pkt)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar={} must be in [-1, 1]",
        t.ffb_scalar
    );
    Ok(())
}

// ─── Rennsport ───────────────────────────────────────────────────────────────
//
// UDP port 9000, identifier byte 0x52 ('R') at offset 0, then:
//   offset 4  f32  speed_kmh
//   offset 8  f32  rpm
//   offset 12 i8   gear  (-1=reverse, 0=neutral, 1+=forward)
//   offset 16 f32  ffb_scalar  [-1, 1]
//   offset 20 f32  slip_ratio  [0, 1]

fn make_rennsport_packet(speed_kmh: f32, rpm: f32, gear: i8, ffb: f32, slip: f32) -> Vec<u8> {
    let mut data = vec![0u8; 24];
    data[0] = 0x52;
    write_f32_le(&mut data, 4, speed_kmh);
    write_f32_le(&mut data, 8, rpm);
    data[12] = gear as u8;
    write_f32_le(&mut data, 16, ffb);
    write_f32_le(&mut data, 20, slip);
    data
}

#[test]
fn rennsport_happy_path_parses_fields() -> TestResult {
    // 180 km/h → 50 m/s
    let pkt = make_rennsport_packet(180.0, 7500.0, 4, 0.6, 0.1);
    let t = RennsportAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 50.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 7500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 4, "gear={}", t.gear);
    assert!(
        (t.ffb_scalar - 0.6).abs() < 0.001,
        "ffb_scalar={}",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn rennsport_empty_packet_returns_error() -> TestResult {
    assert!(RennsportAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn rennsport_short_packet_returns_error() -> TestResult {
    assert!(RennsportAdapter::new().normalize(&[0u8; 8]).is_err());
    Ok(())
}

#[test]
fn rennsport_wrong_identifier_returns_error() -> TestResult {
    let mut pkt = make_rennsport_packet(100.0, 5000.0, 3, 0.0, 0.0);
    pkt[0] = 0x41; // 'A' instead of 'R'
    assert!(RennsportAdapter::new().normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn rennsport_reverse_gear_maps_to_minus_one() -> TestResult {
    let pkt = make_rennsport_packet(0.0, 1000.0, -1, -0.1, 0.0);
    let t = RennsportAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn rennsport_ffb_scalar_clamped() -> TestResult {
    let pkt = make_rennsport_packet(200.0, 8000.0, 5, 9.0, 0.0);
    let t = RennsportAdapter::new().normalize(&pkt)?;
    assert!(
        t.ffb_scalar <= 1.0,
        "ffb_scalar not clamped: {}",
        t.ffb_scalar
    );
    Ok(())
}

// ─── WRC Generations ─────────────────────────────────────────────────────────
//
// UDP port 6777, Codemasters Mode 1 / RallyEngine layout (264 bytes, 66 × f32 LE).
// Verified against dr2_logger udp_data.py, Codemasters telemetry spreadsheet,
// and dirt-rally-time-recorder gearTracker.py.
//   offset 100..112 wheel_speed_{rl,rr,fl,fr}
//   offset 116 throttle   offset 132 gear (-1=reverse, 0=neutral, 1+=forward)
//   offset 148 rpm        offset 188 in_pit
//   offset 252 max_rpm

const WRC_GEN_MIN: usize = 264;

fn make_wrc_gen_packet() -> Vec<u8> {
    vec![0u8; WRC_GEN_MIN]
}

#[test]
fn wrc_generations_happy_path_parses_fields() -> TestResult {
    let mut pkt = make_wrc_gen_packet();
    // wheel speeds → speed_ms = 25.0
    write_f32_le(&mut pkt, 108, 25.0);
    write_f32_le(&mut pkt, 112, 25.0);
    write_f32_le(&mut pkt, 100, 25.0);
    write_f32_le(&mut pkt, 104, 25.0);
    write_f32_le(&mut pkt, 148, 5000.0); // rpm
    write_f32_le(&mut pkt, 252, 8000.0); // max_rpm
    write_f32_le(&mut pkt, 132, 3.0); // gear = 3rd
    write_f32_le(&mut pkt, 116, 0.8); // throttle

    let t = WrcGenerationsAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 25.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 5000.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 3, "gear={}", t.gear);
    assert!((t.throttle - 0.8).abs() < 0.001, "throttle={}", t.throttle);
    Ok(())
}

#[test]
fn wrc_generations_empty_packet_returns_error() -> TestResult {
    assert!(WrcGenerationsAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn wrc_generations_short_packet_returns_error() -> TestResult {
    assert!(WrcGenerationsAdapter::new().normalize(&[0u8; 100]).is_err());
    Ok(())
}

#[test]
fn wrc_generations_gear_zero_maps_to_neutral() -> TestResult {
    // Verified: raw 0.0 = neutral per Codemasters Mode 1 spec (gearTracker.py, udp_data.py).
    let pkt = make_wrc_gen_packet();
    let t = WrcGenerationsAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, 0, "gear 0.0 must map to 0 (neutral)");
    Ok(())
}

#[test]
fn wrc_generations_gear_negative_one_maps_to_reverse() -> TestResult {
    // Verified: DR2.0/WRC sends -1.0 for reverse (gearTracker.py: "Handle reverse gear = -1").
    let mut pkt = make_wrc_gen_packet();
    write_f32_le(&mut pkt, 132, -1.0); // gear = reverse
    let t = WrcGenerationsAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1, "gear -1.0 must map to -1 (reverse)");
    Ok(())
}

#[test]
fn wrc_generations_in_pits_flag_set() -> TestResult {
    let mut pkt = make_wrc_gen_packet();
    write_f32_le(&mut pkt, 188, 1.0); // in_pit = 1.0
    let t = WrcGenerationsAdapter::new().normalize(&pkt)?;
    assert!(t.flags.in_pits, "in_pits must be true when in_pit=1.0");
    Ok(())
}

// ─── Dirt 4 ──────────────────────────────────────────────────────────────────
//
// UDP port 20777, Codemasters extradata v0 layout (≥264 bytes, identical offsets to WRC Generations).

const DIRT4_MIN: usize = 264;

fn make_dirt4_packet() -> Vec<u8> {
    vec![0u8; DIRT4_MIN]
}

#[test]
fn dirt4_happy_path_parses_fields() -> TestResult {
    let mut pkt = make_dirt4_packet();
    write_f32_le(&mut pkt, 108, 20.0); // wheel speed FL
    write_f32_le(&mut pkt, 112, 20.0); // wheel speed FR
    write_f32_le(&mut pkt, 100, 20.0); // wheel speed RL
    write_f32_le(&mut pkt, 104, 20.0); // wheel speed RR
    write_f32_le(&mut pkt, 148, 4500.0); // rpm
    write_f32_le(&mut pkt, 252, 7000.0); // max_rpm
    write_f32_le(&mut pkt, 132, 2.0); // gear = 2nd
    write_f32_le(&mut pkt, 116, 0.6); // throttle

    let t = Dirt4Adapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 20.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 4500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 2, "gear={}", t.gear);
    assert!((t.throttle - 0.6).abs() < 0.001, "throttle={}", t.throttle);
    Ok(())
}

#[test]
fn dirt4_empty_packet_returns_error() -> TestResult {
    assert!(Dirt4Adapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn dirt4_short_packet_returns_error() -> TestResult {
    assert!(Dirt4Adapter::new().normalize(&[0u8; 100]).is_err());
    Ok(())
}

#[test]
fn dirt4_gear_zero_maps_to_reverse() -> TestResult {
    let pkt = make_dirt4_packet(); // gear field 0.0 → reverse
    let t = Dirt4Adapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1, "gear 0.0 must map to -1 (reverse)");
    Ok(())
}

#[test]
fn dirt4_in_pits_flag_set() -> TestResult {
    let mut pkt = make_dirt4_packet();
    write_f32_le(&mut pkt, 188, 1.0); // in_pit = 1.0
    let t = Dirt4Adapter::new().normalize(&pkt)?;
    assert!(t.flags.in_pits, "in_pits must be true when in_pit=1.0");
    Ok(())
}

// ─── Project CARS 2 (extended integration) ───────────────────────────────────
//
// UDP port 5606 / Windows shared memory, simplified 84-byte layout:
//   offset 40 f32 steering   offset 44 f32 throttle  offset 48 f32 brake
//   offset 52 f32 speed_ms   offset 56 f32 rpm        offset 60 f32 max_rpm
//   offset 80 u32 gear

fn make_pcars2_packet(
    steering: f32,
    throttle: f32,
    brake: f32,
    speed: f32,
    rpm: f32,
    max_rpm: f32,
    gear: u32,
) -> Vec<u8> {
    let mut data = vec![0u8; 46];
    data[44] = (steering.clamp(-1.0, 1.0) * 127.0) as i8 as u8; // steering i8
    data[30] = (throttle.clamp(0.0, 1.0) * 255.0) as u8; // throttle u8
    data[29] = (brake.clamp(0.0, 1.0) * 255.0) as u8; // brake u8
    write_f32_le(&mut data, 36, speed); // speed f32 m/s
    data[40..42].copy_from_slice(&(rpm as u16).to_le_bytes()); // rpm u16
    data[42..44].copy_from_slice(&(max_rpm as u16).to_le_bytes()); // max_rpm u16
    let gear_val: u8 = if gear > 14 { 15 } else { gear as u8 };
    data[45] = gear_val; // gear low nibble
    data
}

#[test]
fn pcars2_happy_path_parses_fields() -> TestResult {
    let pkt = make_pcars2_packet(-0.15, 0.9, 0.0, 45.0, 7000.0, 9000.0, 4);
    let t = PCars2Adapter::new().normalize(&pkt)?;
    assert!((t.rpm - 7000.0).abs() < 1.0, "rpm={}", t.rpm);
    assert!((t.speed_ms - 45.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert_eq!(t.gear, 4, "gear={}", t.gear);
    // u8 round-trip: (0.9 * 255) as u8 = 229, 229/255 ≈ 0.898
    assert!(
        (t.throttle - 229.0 / 255.0).abs() < 0.01,
        "throttle={}",
        t.throttle
    );
    Ok(())
}

#[test]
fn pcars2_empty_packet_returns_error() -> TestResult {
    assert!(PCars2Adapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn pcars2_short_packet_returns_error() -> TestResult {
    assert!(PCars2Adapter::new().normalize(&[0u8; 40]).is_err());
    Ok(())
}

#[test]
fn pcars2_steering_clamped_to_minus_one_plus_one() -> TestResult {
    let pkt = make_pcars2_packet(5.0, 0.5, 0.0, 30.0, 5000.0, 8000.0, 3);
    let t = PCars2Adapter::new().normalize(&pkt)?;
    assert!(
        t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
        "steering_angle={} must be clamped to [-1, 1]",
        t.steering_angle
    );
    Ok(())
}

// ─── Live For Speed ───────────────────────────────────────────────────────────
//
// OutGauge UDP port 30000, 92-byte packet (96 with optional OutGauge ID):
//   offset 10 u8  gear  (0=Reverse, 1=Neutral, 2=1st, 3=2nd, …)
//   offset 12 f32 speed_ms   offset 16 f32 rpm
//   offset 28 f32 fuel [0,1] offset 48 f32 throttle
//   offset 52 f32 brake      offset 56 f32 clutch

fn make_lfs_packet(speed: f32, rpm: f32, gear: u8, throttle: f32, brake: f32) -> Vec<u8> {
    let mut data = vec![0u8; 92];
    data[10] = gear;
    write_f32_le(&mut data, 12, speed);
    write_f32_le(&mut data, 16, rpm);
    write_f32_le(&mut data, 28, 0.7); // fuel
    write_f32_le(&mut data, 48, throttle);
    write_f32_le(&mut data, 52, brake);
    data
}

#[test]
fn lfs_happy_path_parses_fields() -> TestResult {
    // gear_raw=3 → normalized gear = 3-1 = 2
    let pkt = make_lfs_packet(30.0, 4500.0, 3, 0.7, 0.0);
    let t = LFSAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 4500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 2, "gear_raw 3 must normalize to 2");
    assert!((t.throttle - 0.7).abs() < 0.001, "throttle={}", t.throttle);
    Ok(())
}

#[test]
fn lfs_empty_packet_returns_error() -> TestResult {
    assert!(LFSAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn lfs_short_packet_returns_error() -> TestResult {
    assert!(LFSAdapter::new().normalize(&[0u8; 50]).is_err());
    Ok(())
}

#[test]
fn lfs_reverse_gear_maps_to_minus_one() -> TestResult {
    let pkt = make_lfs_packet(5.0, 2000.0, 0, 0.0, 0.5);
    let t = LFSAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1, "OutGauge gear 0 must be -1 (reverse)");
    Ok(())
}

#[test]
fn lfs_neutral_gear_maps_to_zero() -> TestResult {
    let pkt = make_lfs_packet(0.0, 800.0, 1, 0.0, 0.0);
    let t = LFSAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, 0, "OutGauge gear 1 must be 0 (neutral)");
    Ok(())
}

#[test]
fn lfs_first_gear_maps_to_one() -> TestResult {
    let pkt = make_lfs_packet(10.0, 3000.0, 2, 0.5, 0.0);
    let t = LFSAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, 1, "OutGauge gear 2 must be 1 (1st gear)");
    Ok(())
}

// ─── Property-based fuzz tests ────────────────────────────────────────────────

// ─── FlatOut UC / FlatOut 4 ──────────────────────────────────────────────────
//
// UDP port 7776, FOTC magic (4 bytes), then:
//   offset 4  u32  sequence
//   offset 8  f32  speed_ms
//   offset 12 f32  rpm
//   offset 16 u8   gear  (0=neutral, 1+=forward)
//   offset 20 f32  lateral_g
//   offset 24 f32  longitudinal_g
//   offset 28 f32  throttle [0, 1]
//   offset 32 f32  brake [0, 1]

const FOTC_MAGIC: [u8; 4] = [0x46, 0x4F, 0x54, 0x43];

fn make_flatout_packet(
    speed: f32,
    rpm: f32,
    gear: u8,
    lat_g: f32,
    lon_g: f32,
    throttle: f32,
    brake: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; 36];
    data[0..4].copy_from_slice(&FOTC_MAGIC);
    write_f32_le(&mut data, 8, speed);
    write_f32_le(&mut data, 12, rpm);
    data[16] = gear;
    write_f32_le(&mut data, 20, lat_g);
    write_f32_le(&mut data, 24, lon_g);
    write_f32_le(&mut data, 28, throttle);
    write_f32_le(&mut data, 32, brake);
    data
}

#[test]
fn flatout_happy_path_parses_fields() -> TestResult {
    let pkt = make_flatout_packet(30.0, 4000.0, 3, 0.5, 0.2, 0.8, 0.1);
    let t = FlatOutAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 30.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 4000.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 3, "gear={}", t.gear);
    assert!(
        (t.lateral_g - 0.5).abs() < 0.001,
        "lateral_g={}",
        t.lateral_g
    );
    assert!((t.throttle - 0.8).abs() < 0.001, "throttle={}", t.throttle);
    Ok(())
}

#[test]
fn flatout_empty_packet_returns_error() -> TestResult {
    assert!(FlatOutAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn flatout_short_packet_returns_error() -> TestResult {
    assert!(FlatOutAdapter::new().normalize(&[0u8; 10]).is_err());
    Ok(())
}

#[test]
fn flatout_bad_magic_returns_error() -> TestResult {
    let mut pkt = make_flatout_packet(10.0, 2000.0, 2, 0.0, 0.0, 0.5, 0.0);
    pkt[0] = 0xFF;
    assert!(FlatOutAdapter::new().normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn flatout_ffb_scalar_stays_in_range() -> TestResult {
    let pkt = make_flatout_packet(60.0, 7000.0, 5, 2.0, 1.5, 1.0, 0.0);
    let t = FlatOutAdapter::new().normalize(&pkt)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar={} must be in [-1, 1]",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn flatout_adapter_game_id() {
    assert_eq!(FlatOutAdapter::new().game_id(), "flatout");
}

// ─── Dakar Desert Rally ───────────────────────────────────────────────────────
//
// UDP port 7779, DAKR magic (4 bytes), then:
//   offset 4  u32  sequence
//   offset 8  f32  speed_ms
//   offset 12 f32  rpm
//   offset 16 u8   gear  (0=neutral, 255=reverse, 1+=forward)
//   offset 20 f32  lateral_g
//   offset 24 f32  longitudinal_g
//   offset 28 f32  throttle [0, 1]
//   offset 32 f32  brake [0, 1]
//   offset 36 f32  steering_angle [-1, 1]

const DAKR_MAGIC: [u8; 4] = [0x44, 0x41, 0x4B, 0x52];

fn make_dakar_packet(
    speed: f32,
    rpm: f32,
    gear: u8,
    lat_g: f32,
    lon_g: f32,
    throttle: f32,
    brake: f32,
    steering: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; 40];
    data[0..4].copy_from_slice(&DAKR_MAGIC);
    write_f32_le(&mut data, 8, speed);
    write_f32_le(&mut data, 12, rpm);
    data[16] = gear;
    write_f32_le(&mut data, 20, lat_g);
    write_f32_le(&mut data, 24, lon_g);
    write_f32_le(&mut data, 28, throttle);
    write_f32_le(&mut data, 32, brake);
    write_f32_le(&mut data, 36, steering);
    data
}

#[test]
fn dakar_happy_path_parses_fields() -> TestResult {
    let pkt = make_dakar_packet(25.0, 3500.0, 3, 0.4, 0.1, 0.7, 0.0, 0.2);
    let t = DakarDesertRallyAdapter::new().normalize(&pkt)?;
    assert!((t.speed_ms - 25.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 3500.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 3, "gear={}", t.gear);
    assert!(
        (t.lateral_g - 0.4).abs() < 0.001,
        "lateral_g={}",
        t.lateral_g
    );
    assert!((t.throttle - 0.7).abs() < 0.001, "throttle={}", t.throttle);
    assert!(
        (t.steering_angle - 0.2).abs() < 0.001,
        "steering_angle={}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn dakar_empty_packet_returns_error() -> TestResult {
    assert!(DakarDesertRallyAdapter::new().normalize(&[]).is_err());
    Ok(())
}

#[test]
fn dakar_short_packet_returns_error() -> TestResult {
    assert!(
        DakarDesertRallyAdapter::new()
            .normalize(&[0u8; 10])
            .is_err()
    );
    Ok(())
}

#[test]
fn dakar_bad_magic_returns_error() -> TestResult {
    let mut pkt = make_dakar_packet(10.0, 2000.0, 2, 0.0, 0.0, 0.5, 0.0, 0.0);
    pkt[0] = 0xFF;
    assert!(DakarDesertRallyAdapter::new().normalize(&pkt).is_err());
    Ok(())
}

#[test]
fn dakar_reverse_gear_maps_to_minus_one() -> TestResult {
    let pkt = make_dakar_packet(5.0, 1500.0, 255, 0.0, -0.3, 0.0, 0.3, 0.0);
    let t = DakarDesertRallyAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, -1, "gear 255 must map to -1 (reverse)");
    Ok(())
}

#[test]
fn dakar_neutral_gear_maps_to_zero() -> TestResult {
    let pkt = make_dakar_packet(0.0, 800.0, 0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = DakarDesertRallyAdapter::new().normalize(&pkt)?;
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn dakar_ffb_scalar_stays_in_range() -> TestResult {
    let pkt = make_dakar_packet(60.0, 6000.0, 5, 2.0, 1.5, 1.0, 0.0, 0.0);
    let t = DakarDesertRallyAdapter::new().normalize(&pkt)?;
    assert!(
        t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
        "ffb_scalar={} must be in [-1, 1]",
        t.ffb_scalar
    );
    Ok(())
}

#[test]
fn dakar_adapter_game_id() {
    assert_eq!(
        DakarDesertRallyAdapter::new().game_id(),
        "dakar_desert_rally"
    );
}

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    // ETS2 ────────────────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn ets2_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = Ets2Adapter::new().normalize(&data);
        }

        #[test]
        fn ets2_short_packet_always_errors(
            // 24 is the minimum required size (OFF_ENGINE_LOAD=20 + 4)
            data in proptest::collection::vec(any::<u8>(), 0..24usize)
        ) {
            prop_assert!(Ets2Adapter::new().normalize(&data).is_err());
        }
    }

    // Wreckfest ───────────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn wreckfest_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = WreckfestAdapter::new().normalize(&data);
        }

        #[test]
        fn wreckfest_short_packet_always_errors(
            // WRECKFEST_MIN_PACKET_SIZE = 28
            data in proptest::collection::vec(any::<u8>(), 0..28usize)
        ) {
            prop_assert!(WreckfestAdapter::new().normalize(&data).is_err());
        }
    }

    // Rennsport ───────────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn rennsport_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = RennsportAdapter::new().normalize(&data);
        }

        #[test]
        fn rennsport_short_packet_always_errors(
            // RENNSPORT_MIN_PACKET_SIZE = 24
            data in proptest::collection::vec(any::<u8>(), 0..24usize)
        ) {
            prop_assert!(RennsportAdapter::new().normalize(&data).is_err());
        }
    }

    // WRC Generations ─────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn wrc_generations_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = WrcGenerationsAdapter::new().normalize(&data);
        }

        #[test]
        fn wrc_generations_short_packet_always_errors(
            // MIN_PACKET_SIZE = 264
            data in proptest::collection::vec(any::<u8>(), 0..264usize)
        ) {
            prop_assert!(WrcGenerationsAdapter::new().normalize(&data).is_err());
        }
    }

    // Dirt 4 ──────────────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn dirt4_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = Dirt4Adapter::new().normalize(&data);
        }

        #[test]
        fn dirt4_short_packet_always_errors(
            // MIN_PACKET_SIZE = 264
            data in proptest::collection::vec(any::<u8>(), 0..264usize)
        ) {
            prop_assert!(Dirt4Adapter::new().normalize(&data).is_err());
        }
    }

    // Project CARS 2 ──────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn pcars2_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = PCars2Adapter::new().normalize(&data);
        }

        #[test]
        fn pcars2_short_packet_always_errors(
            // PCARS2_UDP_MIN_SIZE = 46
            data in proptest::collection::vec(any::<u8>(), 0..46usize)
        ) {
            prop_assert!(PCars2Adapter::new().normalize(&data).is_err());
        }

        #[test]
        fn pcars2_valid_packet_speed_nonnegative(
            steering in -1.0f32..=1.0f32,
            throttle in 0.0f32..1.0f32,
            brake   in 0.0f32..1.0f32,
            speed   in 0.0f32..250.0f32,
            rpm     in 0.0f32..12000.0f32,
            max_rpm in 5000.0f32..12000.0f32,
            gear    in 0u32..8u32,
        ) {
            let pkt = make_pcars2_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
            let result = PCars2Adapter::new().normalize(&pkt);
            let t = result.map_err(|e| TestCaseError::fail(format!("normalize failed: {e:?}")))?;
            prop_assert!(t.speed_ms >= 0.0, "speed_ms {} must be non-negative", t.speed_ms);
        }
    }

    // Live For Speed ──────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn lfs_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = LFSAdapter::new().normalize(&data);
        }

        #[test]
        fn lfs_short_packet_always_errors(
            // OUTGAUGE_PACKET_SIZE = 92
            data in proptest::collection::vec(any::<u8>(), 0..92usize)
        ) {
            prop_assert!(LFSAdapter::new().normalize(&data).is_err());
        }

        #[test]
        fn lfs_valid_packet_throttle_in_range(
            speed    in 0.0f32..200.0f32,
            rpm      in 0.0f32..12000.0f32,
            gear     in 0u8..8u8,
            throttle in 0.0f32..1.0f32,
            brake    in 0.0f32..1.0f32,
        ) {
            let pkt = make_lfs_packet(speed, rpm, gear, throttle, brake);
            let result = LFSAdapter::new().normalize(&pkt);
            prop_assert!(result.is_ok(), "expected normalize to succeed: {:?}", result.err());
            let t = result.map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.throttle >= 0.0 && t.throttle <= 1.0,
                "throttle {} must be in [0, 1]",
                t.throttle
            );
        }
    }

    // FlatOut ─────────────────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn flatout_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = FlatOutAdapter::new().normalize(&data);
        }

        #[test]
        fn flatout_short_packet_always_errors(
            // FLATOUT_MIN_PACKET_SIZE = 36
            data in proptest::collection::vec(any::<u8>(), 0..36usize)
        ) {
            prop_assert!(FlatOutAdapter::new().normalize(&data).is_err());
        }
    }

    // Dakar Desert Rally ──────────────────────────────────────────────────────

    proptest! {
        #[test]
        fn dakar_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = DakarDesertRallyAdapter::new().normalize(&data);
        }

        #[test]
        fn dakar_short_packet_always_errors(
            // DAKAR_MIN_PACKET_SIZE = 40
            data in proptest::collection::vec(any::<u8>(), 0..40usize)
        ) {
            prop_assert!(DakarDesertRallyAdapter::new().normalize(&data).is_err());
        }
    }
}
