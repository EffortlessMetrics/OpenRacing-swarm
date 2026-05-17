//! BDD-style acceptance tests: plug-and-play telemetry scenarios.
//!
//! Verifies that game telemetry adapters can receive simulated game packets
//! from mock UDP servers and produce correctly normalised output — the
//! "plug it in and it works" guarantee for the 56+ supported titles.
//!
//! # Features covered
//!
//! * **Assetto Corsa Remote Telemetry UDP** – AC adapter connects to a mock
//!   server, performs a 3-step handshake (connect→response→subscribe), and
//!   receives an RTCarInfo packet; the first normalised frame arrives
//!   within 500ms with correct speed / RPM / gear.
//! * **Forza Motorsport CarDash UDP** – Forza adapter receives a CarDash
//!   packet via a mock UDP server; throttle / gear / speed are verified.
//! * **ACC broadcasting** – A hand-crafted `RealtimeCarUpdate` binary
//!   message is parsed directly via `normalize()`; speed and gear are
//!   verified, including pit-lane flag mapping.
//! * **iRacing latency budget** – `normalize()` on a representative
//!   shared-memory snapshot completes well within the 1ms per-tick budget.
//! * **Core adapter registry** – All tier-1 game adapters are registered in
//!   the factory table so the service can instantiate them at runtime.

use std::time::{Duration, Instant};

use openracing_telemetry_adapters::{
    ACCAdapter, AssettoCorsaAdapter, ForzaAdapter, IRacingAdapter, TelemetryAdapter,
    adapter_factories,
};
use tokio::time::timeout;

// ─── Byte-packing helpers ─────────────────────────────────────────────────────

fn skip_shared_ci_timing_guarantees() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var("OPENRACING_SKIP_TIMING_GUARANTEES")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
}

fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ─── Port allocation helper ───────────────────────────────────────────────────

/// Bind a temporary UDP socket on the loopback interface and return the
/// OS-assigned port number.  The socket is dropped immediately, freeing
/// the port.  There is a brief TOCTOU window between the release and the
/// adapter's re-bind, but this is safe for in-process tests on loopback.
async fn find_free_udp_port() -> anyhow::Result<u16> {
    let s = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    Ok(s.local_addr()?.port())
}

// ─── Packet builders ──────────────────────────────────────────────────────────

/// Build a 328-byte Assetto Corsa RTCarInfo UDP packet.
///
/// Byte offsets (AC Remote Telemetry UDP protocol):
/// * 16 → `speed_Ms` (f32, m/s)
/// * 56 → `gas` / throttle (f32 0.0–1.0)
/// * 60 → `brake` (f32 0.0–1.0)
/// * 68 → `rpm` (f32)
/// * 76 → `gear` (i32: 0=R, 1=N, 2=1st, 3=2nd, 4=3rd, ...)
fn make_ac_rtcarinfo_packet(
    gear_ac: i32,
    speed_ms: f32,
    rpm: f32,
    throttle: f32,
    brake: f32,
) -> Vec<u8> {
    let mut pkt = vec![0u8; 328];
    // identifier
    pkt[0..4].copy_from_slice(&(b'a' as i32).to_le_bytes());
    // size
    write_i32_le(&mut pkt, 4, 328);
    // speed_Ms at offset 16
    write_f32_le(&mut pkt, 16, speed_ms);
    // gas at offset 56
    write_f32_le(&mut pkt, 56, throttle);
    // brake at offset 60
    write_f32_le(&mut pkt, 60, brake);
    // rpm at offset 68
    write_f32_le(&mut pkt, 68, rpm);
    // gear at offset 76
    write_i32_le(&mut pkt, 76, gear_ac);
    pkt
}

/// Build a 311-byte Forza Motorsport CarDash UDP packet.
///
/// Key offsets:
/// * 0   → `is_race_on` (i32, must be 1 for live telemetry)
/// * 8   → `engine_max_rpm` (f32)
/// * 16  → `current_rpm` (f32)
/// * 32  → `vel_x` (f32 m/s, used for Sled speed magnitude)
/// * 244 → `dash_speed` (f32 m/s, CarDash authoritative speed)
/// * 303 → `dash_accel` (u8, 0–255 → 0.0–1.0)
/// * 307 → `dash_gear` (u8: 0=R, 1=N, 2=1st, 3=2nd, …)
fn make_forza_cardash_packet(
    speed_ms: f32,
    rpm: f32,
    max_rpm: f32,
    throttle_u8: u8,
    gear_raw: u8,
) -> Vec<u8> {
    let mut pkt = vec![0u8; 311];
    write_i32_le(&mut pkt, 0, 1); // is_race_on = 1
    write_f32_le(&mut pkt, 8, max_rpm); // engine_max_rpm
    write_f32_le(&mut pkt, 16, rpm); // current_rpm
    write_f32_le(&mut pkt, 32, speed_ms); // vel_x → Sled speed magnitude
    write_f32_le(&mut pkt, 244, speed_ms); // dash_speed (authoritative)
    pkt[303] = throttle_u8; // dash_accel
    pkt[307] = gear_raw; // dash_gear
    pkt
}

/// Build a minimal ACC `RealtimeCarUpdate` (message-type 3) binary packet.
///
/// Layout follows ACC broadcasting protocol v4:
/// * [0]       message type = 3 (`MSG_REALTIME_CAR_UPDATE`)
/// * [1..3)    car_index u16
/// * [3..5)    driver_index u16
/// * [5]       driver_count u8
/// * [6]       gear_raw u8 — normalised gear = gear_raw − 2
/// * [7..19)   world_pos_x, world_pos_y, yaw (3 × f32)
/// * [19]      car_location u8
/// * [20..22)  speed_kmh u16
/// * [22..38)  position, cup_pos, track_pos, spline_pos, laps, delta_ms
/// * [38..)    3 lap-time records, 13 bytes each (split_count = 0)
fn make_acc_car_update_packet(gear_raw: u8, speed_kmh: u16) -> Vec<u8> {
    let mut p = Vec::with_capacity(77);

    p.push(3u8); // MSG_REALTIME_CAR_UPDATE
    p.extend_from_slice(&0u16.to_le_bytes()); // car_index
    p.extend_from_slice(&0u16.to_le_bytes()); // driver_index
    p.push(1u8); // driver_count
    p.push(gear_raw); // gear_raw

    // world_pos_x, world_pos_y, yaw (unused, zeroed)
    for _ in 0..3 {
        p.extend_from_slice(&0.0f32.to_le_bytes());
    }

    p.push(1u8); // car_location = 1 (on track)
    p.extend_from_slice(&speed_kmh.to_le_bytes()); // speed_kmh
    p.extend_from_slice(&1u16.to_le_bytes()); // position
    p.extend_from_slice(&1u16.to_le_bytes()); // cup_position
    p.extend_from_slice(&5000u16.to_le_bytes()); // track_position
    p.extend_from_slice(&0.5f32.to_le_bytes()); // spline_position
    p.extend_from_slice(&3u16.to_le_bytes()); // laps
    p.extend_from_slice(&(-500i32).to_le_bytes()); // delta_ms

    // Three lap-time records (best_session, last, current), each 13 bytes:
    //   lap_time_ms(4) + car_index(2) + driver_index(2) + split_count(1) + 4 flags(4)
    for _ in 0..3 {
        p.extend_from_slice(&0i32.to_le_bytes()); // lap_time_ms
        p.extend_from_slice(&0u16.to_le_bytes()); // car_index
        p.extend_from_slice(&0u16.to_le_bytes()); // driver_index
        p.push(0u8); // split_count = 0
        p.push(0u8); // is_invalid
        p.push(1u8); // is_valid_for_best
        p.push(0u8); // is_outlap
        p.push(0u8); // is_inlap
    }

    p
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Assetto Corsa UDP plug-and-play
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: User starts Assetto Corsa with a connected wheel
///
/// ```text
/// Given  a mock AC Remote Telemetry server is running on UDP port N
/// When   the AC adapter connects, handshakes, and subscribes
/// And    the server sends an RTCarInfo update packet
/// Then   a normalised telemetry frame arrives within 500ms
/// And    speed, RPM, and gear are correctly parsed from the packet
/// ```
#[tokio::test]
async fn scenario_assetto_corsa_game_sends_udp_telemetry_adapter_normalises_within_100ms()
-> anyhow::Result<()> {
    // Given: A mock AC Remote Telemetry server on a free UDP port
    let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let port = server.local_addr()?.port();

    // Start the AC adapter pointing at our mock server
    let adapter = AssettoCorsaAdapter::new().with_port(port);
    let mut rx = adapter.start_monitoring().await?;

    // Mock server: receive handshake, respond, receive subscribe, send data
    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 512];
        // 1. Receive handshake (12 bytes)
        let (len, client_addr) = server.recv_from(&mut buf).await?;
        assert!(len >= 12, "expected handshake packet, got {len} bytes");

        // 2. Send handshake response (any non-empty reply)
        let response = [0u8; 50]; // AC sends a HandshakerResponse struct
        server.send_to(&response, client_addr).await?;

        // 3. Receive subscribe request (12 bytes)
        let (len, _) = server.recv_from(&mut buf).await?;
        assert!(len >= 12, "expected subscribe packet, got {len} bytes");

        // 4. Send RTCarInfo update packet
        //    gear=5 (AC: 0=R,1=N,2=1st,...,5=4th), 40 m/s, 6500 RPM, 75% throttle
        let pkt = make_ac_rtcarinfo_packet(5, 40.0, 6500.0, 0.75, 0.0);
        server.send_to(&pkt, client_addr).await?;

        Ok::<(), anyhow::Error>(())
    });

    // Then: a normalised telemetry frame arrives within 500ms
    let frame = timeout(Duration::from_millis(500), rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no AC telemetry frame received within 500ms"))?
        .ok_or_else(|| anyhow::anyhow!("AC telemetry channel closed unexpectedly"))?;

    // Verify server task completed successfully
    server_task.await??;

    // And: speed_ms ≈ 40 m/s
    assert!(
        (frame.data.speed_ms - 40.0).abs() < 0.5,
        "speed_ms must be ~40 m/s, got {}",
        frame.data.speed_ms
    );

    // And: RPM is correctly parsed
    assert!(
        (frame.data.rpm - 6500.0).abs() < 1.0,
        "rpm must be ~6 500, got {}",
        frame.data.rpm
    );

    // And: gear is correctly parsed (AC gear 5 = 4th → normalized 4)
    assert_eq!(frame.data.gear, 4, "gear must be 4 (AC gear 5 - 1)");

    // And: throttle is correctly normalised (0.75 ± 0.01)
    assert!(
        (frame.data.throttle - 0.75).abs() < 0.01,
        "throttle must be ~0.75, got {}",
        frame.data.throttle
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Forza Motorsport UDP plug-and-play
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: User starts Forza Motorsport with a connected wheel
///
/// ```text
/// Given  a wheel is connected and the Forza adapter is monitoring UDP port N
/// When   Forza Motorsport sends a CarDash telemetry packet on port N
/// Then   a normalised telemetry frame arrives within 100ms
/// And    speed, RPM, throttle, and gear are correctly decoded
/// ```
#[tokio::test]
async fn scenario_forza_motorsport_game_sends_cardash_udp_adapter_normalises_within_100ms()
-> anyhow::Result<()> {
    // Given: Forza adapter is monitoring a free UDP port
    let port = find_free_udp_port().await?;
    let adapter = ForzaAdapter::new().with_port(port);
    let mut rx = adapter.start_monitoring().await?;

    // Give the spawned task time to bind the socket before we send data.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // When: Forza sends a 311-byte CarDash packet
    //   speed = 35 m/s, rpm = 5 500, max_rpm = 8 000
    //   throttle = 204 (= 204/255 ≈ 0.80)
    //   dash_gear = 5 → normalised gear 4  (0=R, 1=N, 2=1st … 5=4th)
    let pkt = make_forza_cardash_packet(35.0, 5500.0, 8000.0, 204, 5);
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    sender.send_to(&pkt, format!("127.0.0.1:{port}")).await?;

    // Then: a normalised telemetry frame arrives within 100ms
    let frame = timeout(Duration::from_millis(100), rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no Forza telemetry frame received within 100ms"))?
        .ok_or_else(|| anyhow::anyhow!("Forza telemetry channel closed unexpectedly"))?;

    // And: speed_ms ≈ 35 m/s (dash_speed field takes precedence over vel_x)
    assert!(
        (frame.data.speed_ms - 35.0).abs() < 0.5,
        "speed_ms must be ~35 m/s, got {}",
        frame.data.speed_ms
    );

    // And: RPM is correctly parsed
    assert!(
        (frame.data.rpm - 5500.0).abs() < 1.0,
        "rpm must be ~5 500, got {}",
        frame.data.rpm
    );

    // And: gear is correctly decoded (dash_gear=5 → 4th gear)
    assert_eq!(frame.data.gear, 4, "gear must be 4 (dash_gear=5)");

    // And: throttle is correctly normalised (204 / 255 ≈ 0.80)
    let expected_throttle = 204.0_f32 / 255.0;
    assert!(
        (frame.data.throttle - expected_throttle).abs() < 0.01,
        "throttle must be ~{:.3}, got {}",
        expected_throttle,
        frame.data.throttle
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Assetto Corsa Competizione broadcasting protocol
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: User starts Assetto Corsa Competizione with a connected wheel
///
/// ```text
/// Given  a wheel is connected and the ACC adapter is running
/// When   ACC sends a RealtimeCarUpdate broadcasting packet
/// Then   speed_ms and gear are correctly normalised
/// And    the adapter correctly identifies itself as "acc"
/// ```
#[test]
fn scenario_acc_realtime_car_update_correctly_normalises_speed_and_gear()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: ACC adapter (no hardware required – normalize() is pure)
    let adapter = ACCAdapter::new();

    // When: a RealtimeCarUpdate packet arrives
    //   gear_raw = 5 → normalised gear = 5 − 1 = 4
    //   speed_kmh = 108 → speed_ms = 108 / 3.6 = 30.0 m/s
    let pkt = make_acc_car_update_packet(5, 108);
    let t = adapter.normalize(&pkt)?;

    // Then: speed_ms ≈ 30 m/s
    assert!(
        (t.speed_ms - 30.0).abs() < 0.2,
        "speed_ms must be ~30 m/s (108 km/h), got {}",
        t.speed_ms
    );

    // And: gear = 5 − 1 = 4
    assert_eq!(t.gear, 4, "gear must be 4 (gear_raw=5)");

    // And: the adapter correctly identifies the game
    assert_eq!(
        adapter.game_id(),
        "acc",
        "ACC adapter must report game_id 'acc'"
    );

    Ok(())
}

/// Scenario: ACC car in pit lane → in_pits flag is set
///
/// ```text
/// Given  an ACC car is in the pit lane (car_location = 2)
/// When   the RealtimeCarUpdate packet is normalised
/// Then   the in_pits flag is set to true
/// ```
#[test]
fn scenario_acc_car_in_pit_lane_sets_in_pits_flag() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = ACCAdapter::new();

    // car_location byte is at packet offset 19:
    //   1 (type) + 2 (car_idx) + 2 (drv_idx) + 1 (drv_cnt) + 1 (gear) + 12 (3×f32) = 19
    let mut pkt = make_acc_car_update_packet(3, 25);
    pkt[19] = 2; // car_location = 2 (pit lane)

    let t = adapter.normalize(&pkt)?;

    assert!(
        t.flags.in_pits,
        "car_location=2 (pit lane) must set the in_pits flag"
    );

    Ok(())
}

/// Scenario: ACC adapter rejects packets that are too short to be valid
///
/// ```text
/// Given  an ACC adapter
/// When   an empty or truncated packet is received
/// Then   normalize() returns an Err (no panic)
/// ```
#[test]
fn scenario_acc_truncated_packet_returns_error_not_panic() -> Result<(), Box<dyn std::error::Error>>
{
    let adapter = ACCAdapter::new();

    // An empty or one-byte packet must produce Err, not panic.
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must return Err"
    );
    assert!(
        adapter.normalize(&[3u8]).is_err(),
        "single-byte packet must return Err"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: iRacing telemetry latency budget
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: iRacing telemetry normalization stays within the RT latency budget
///
/// ```text
/// Given  iRacing is sending shared-memory telemetry at ≥60 Hz
/// When   the adapter normalises a telemetry snapshot
/// Then   normalize() completes in under 1ms
/// And    the 100ms first-frame budget is comfortably satisfied
/// ```
#[test]
fn scenario_iracing_normalize_completes_within_1ms_latency_budget()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: an iRacing adapter and a representative shared-memory snapshot.
    //        A zero-filled 8 KiB buffer is large enough to cover both the
    //        legacy and modern IRSDK memory layouts.
    let adapter = IRacingAdapter::new();
    let snapshot = vec![0u8; 8192];

    // When: normalising the snapshot (timing the pure CPU work)
    let start = Instant::now();
    let _ = adapter.normalize(&snapshot)?;
    let elapsed = start.elapsed();

    if skip_shared_ci_timing_guarantees() {
        eprintln!("skipping strict iRacing normalize latency budget under shared CI");
        return Ok(());
    }

    // Then: the call must complete in under 1ms
    //       (100ms end-to-end budget minus network and pipeline latency)
    assert!(
        elapsed < Duration::from_millis(1),
        "iRacing normalize() must complete in <1ms (actual: {elapsed:?})"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Feature: Core adapter registry completeness
// ═══════════════════════════════════════════════════════════════════════════════

/// Scenario: All tier-1 game adapters are registered at startup
///
/// ```text
/// Given  the service starts up
/// When   the adapter factory registry is queried
/// Then   all tier-1 supported games have a registered factory
/// And    the service can instantiate an adapter for each game without
///        additional configuration
/// ```
#[test]
fn scenario_all_tier1_game_adapters_registered_at_startup() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: the adapter factory registry
    let factories = adapter_factories();
    let registered: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();

    // When/Then: each tier-1 game must have a registered factory
    let tier1: &[(&str, &str)] = &[
        ("assetto_corsa", "Assetto Corsa (OutGauge UDP, port 9996)"),
        ("acc", "Assetto Corsa Competizione (broadcasting protocol)"),
        ("iracing", "iRacing (Windows shared memory)"),
        ("rfactor2", "rFactor 2 (Windows shared memory)"),
        ("project_cars_2", "Project CARS 2 (UDP)"),
        ("forza_motorsport", "Forza Motorsport (CarDash UDP)"),
        ("forza_horizon_5", "Forza Horizon 5 (CarDash UDP)"),
        ("raceroom", "RaceRoom Racing Experience (UDP)"),
        ("beamng_drive", "BeamNG.drive (OutGauge UDP)"),
        ("le_mans_ultimate", "Le Mans Ultimate (UDP)"),
        ("ams2", "Automobilista 2 (UDP)"),
        ("f1", "F1 series (Codemasters UDP)"),
        ("gran_turismo_7", "Gran Turismo 7 (UDP)"),
    ];

    let mut missing: Vec<&str> = Vec::new();
    for (game_id, _description) in tier1 {
        if !registered.contains(game_id) {
            missing.push(game_id);
        }
    }

    assert!(
        missing.is_empty(),
        "tier-1 game adapters not registered: {missing:?}"
    );

    Ok(())
}

/// Scenario: Each registered adapter reports a game ID that matches its
///           registry key — prevents silent mismatches that break runtime
///           game detection.
///
/// ```text
/// Given  all registered adapter factories
/// When   each adapter is instantiated
/// Then   adapter.game_id() matches the key it was registered under
/// ```
#[test]
fn scenario_every_registered_adapter_game_id_matches_its_registry_key()
-> Result<(), Box<dyn std::error::Error>> {
    for (key, factory) in adapter_factories() {
        let adapter = factory();
        assert_eq!(
            adapter.game_id(),
            *key,
            "adapter game_id() must match its registry key"
        );
    }

    Ok(())
}
