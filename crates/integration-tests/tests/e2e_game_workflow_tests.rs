//! End-to-end game workflow integration tests.
//!
//! Complete game session workflows: game selection, adapter lifecycle,
//! telemetry processing, recording, multi-game switching.
//!
//! Cross-crate coverage: telemetry-adapters (TelemetryAdapter, MockAdapter,
//! adapter_factories) × schemas (NormalizedTelemetry, TelemetryFrame) ×
//! engine (Pipeline, SafetyService) × filters × telemetry-recorder ×
//! service (GameService, AutoProfileSwitchingService, ProfileService).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;

use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};
use openracing_telemetry_adapters::{MockAdapter, TelemetryAdapter, adapter_factories};
use openracing_telemetry_recorder::{TelemetryPlayer, TelemetryRecorder};
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::SafetyService;
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline, VirtualDevice};
use racing_wheel_schemas::prelude::*;
use racing_wheel_schemas::telemetry::TelemetryFrame;

use racing_wheel_service::{
    auto_profile_switching::AutoProfileSwitchingService,
    game_service::GameService,
    game_telemetry_bridge::TelemetryAdapterControl,
    process_detection::{ProcessEvent, ProcessInfo},
    profile_repository::ProfileRepositoryConfig,
    profile_service::ProfileService,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found"))?;
    Ok(factory())
}

fn assert_f32_near(actual: f32, expected: f32, tol: f32, label: &str) {
    assert!(
        (actual - expected).abs() < tol,
        "{label}: expected ~{expected}, got {actual} (tol={tol})"
    );
}

/// Build a minimal Forza Sled packet (232 bytes).
fn build_forza_packet(vel_x: f32, vel_z: f32, rpm: f32, max_rpm: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 232];
    buf[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
    buf[8..12].copy_from_slice(&max_rpm.to_le_bytes());
    buf[16..20].copy_from_slice(&rpm.to_le_bytes());
    buf[32..36].copy_from_slice(&vel_x.to_le_bytes());
    buf[40..44].copy_from_slice(&vel_z.to_le_bytes());
    buf
}

/// Build a minimal LFS OutGauge packet (96 bytes).
fn build_lfs_packet(speed: f32, rpm: f32, gear: u8, throttle: f32) -> Vec<u8> {
    let mut buf = vec![0u8; 96];
    buf[10] = gear;
    buf[12..16].copy_from_slice(&speed.to_le_bytes());
    buf[16..20].copy_from_slice(&rpm.to_le_bytes());
    buf[48..52].copy_from_slice(&throttle.to_le_bytes());
    buf
}

struct MockAdapterControl {
    starts: Arc<Mutex<Vec<String>>>,
    stops: Arc<Mutex<Vec<String>>>,
}

impl MockAdapterControl {
    fn new() -> Self {
        Self {
            starts: Arc::new(Mutex::new(Vec::new())),
            stops: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn started_games(&self) -> Vec<String> {
        self.starts.lock().await.clone()
    }

    async fn stopped_games(&self) -> Vec<String> {
        self.stops.lock().await.clone()
    }
}

#[async_trait]
impl TelemetryAdapterControl for MockAdapterControl {
    async fn start_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.starts.lock().await.push(game_id.to_string());
        Ok(())
    }

    async fn stop_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.stops.lock().await.push(game_id.to_string());
        Ok(())
    }
}

fn game_started_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStarted {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

fn game_stopped_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStopped {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

async fn make_profile_service(tmp: &TempDir) -> anyhow::Result<Arc<ProfileService>> {
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    Ok(Arc::new(ProfileService::new_with_config(config).await?))
}

async fn seed_profile(service: &ProfileService, id: &str) -> anyhow::Result<ProfileId> {
    let profile_id: ProfileId = id.parse()?;
    let profile = Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        id.to_string(),
    );
    service.create_profile(profile).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Complete game session: select game → configure adapter → receive → process
// ═══════════════════════════════════════════════════════════════════════════════

/// Full game session: select Forza → normalize telemetry → filter → engine →
/// safety clamp. Validates the entire data path.
#[test]
fn game_session_forza_full_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("forza_motorsport")?;
    assert_eq!(adapter.game_id(), "forza_motorsport");

    let packet = build_forza_packet(20.0, 30.0, 7000.0, 9000.0);
    let telemetry = adapter.normalize(&packet)?;

    let expected_speed = (20.0f32.powi(2) + 30.0f32.powi(2)).sqrt();
    assert_f32_near(telemetry.speed_ms, expected_speed, 1.0, "Forza speed");
    assert_f32_near(telemetry.rpm, 7000.0, 1.0, "Forza RPM");

    // Filter stage
    let mut frame = FilterFrame {
        ffb_in: telemetry.ffb_scalar,
        torque_out: telemetry.ffb_scalar,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut frame, &DamperState::fixed(0.02));
    friction_filter(&mut frame, &FrictionState::fixed(0.01));
    torque_cap_filter(&mut frame, 1.0);

    assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);

    // Engine + safety
    let mut ef = EngineFrame {
        ffb_in: frame.torque_out,
        torque_out: frame.torque_out,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut ef)?;

    let safety = SafetyService::new(5.0, 20.0);
    let clamped = safety.clamp_torque_nm(ef.torque_out * 5.0);
    assert!(clamped.abs() <= 5.0);

    Ok(())
}

/// Full game session with LFS: raw packet → adapter → pipeline → output.
#[test]
fn game_session_lfs_full_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = get_adapter("live_for_speed")?;
    let packet = build_lfs_packet(35.0, 5500.0, 3, 0.8);
    let telemetry = adapter.normalize(&packet)?;

    assert_f32_near(telemetry.speed_ms, 35.0, 0.5, "LFS speed");

    let mut frame = FilterFrame {
        ffb_in: telemetry.ffb_scalar,
        torque_out: telemetry.ffb_scalar,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut frame, &DamperState::fixed(0.01));
    torque_cap_filter(&mut frame, 1.0);

    let mut ef = EngineFrame {
        ffb_in: frame.torque_out,
        torque_out: frame.torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut ef)?;

    let safety = SafetyService::new(5.0, 20.0);
    let clamped = safety.clamp_torque_nm(ef.torque_out * 5.0);
    assert!(clamped.is_finite());
    assert!(clamped.abs() <= 5.0);

    Ok(())
}

/// MockAdapter → full pipeline, verifying integration.
#[test]
fn game_session_mock_adapter_full_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = MockAdapter::new("game_session_test".to_string());
    let telemetry = adapter.normalize(&[])?;

    assert!(
        (telemetry.rpm - 5000.0).abs() < 1.0,
        "MockAdapter RPM should be ~5000"
    );

    let mut frame = FilterFrame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: telemetry.speed_ms,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut frame, &DamperState::fixed(0.01));
    friction_filter(&mut frame, &FrictionState::fixed(0.01));
    torque_cap_filter(&mut frame, 1.0);

    let mut ef = EngineFrame {
        ffb_in: frame.torque_out,
        torque_out: frame.torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let mut pipeline = EnginePipeline::new();
    pipeline.process(&mut ef)?;

    assert!(ef.torque_out.is_finite());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Game switching during active session
// ═══════════════════════════════════════════════════════════════════════════════

/// Switch between multiple games sequentially using adapter swap.
#[test]
fn game_switch_sequential_adapter_swap() -> Result<(), Box<dyn std::error::Error>> {
    let id: DeviceId = "gw-switch-001".parse()?;
    let mut device = VirtualDevice::new(id, "Game Switch Wheel".to_string());
    let mut pipeline = EnginePipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    let game_ids = ["forza_motorsport", "live_for_speed", "dirt_rally_2"];

    for (game_idx, game_id) in game_ids.iter().enumerate() {
        let adapter = get_adapter(game_id)?;
        assert_eq!(adapter.game_id(), *game_id);

        let large_buf = vec![0u8; 2048];
        let ffb_scalar = match adapter.normalize(&large_buf) {
            Ok(telem) => telem.ffb_scalar,
            Err(_) => 0.0,
        };

        for tick in 0u16..10 {
            let seq = (game_idx as u16) * 100 + tick;
            let mut ef = EngineFrame {
                ffb_in: ffb_scalar,
                torque_out: ffb_scalar,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: u64::from(seq) * 1_000_000,
                seq,
            };
            pipeline.process(&mut ef)?;
            let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
            device.write_ffb_report(torque, seq)?;
        }
    }

    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Game switch: start → stop → start different game via auto-profile switching.
#[tokio::test]
async fn game_switch_auto_profile_start_stop_start() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "acc_gt3").await?;
    seed_profile(&profile_service, "iracing_gt3").await?;
    seed_profile(&profile_service, "global").await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("acc".to_string(), "acc_gt3".to_string())
        .await?;
    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    // Start ACC
    svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("acc_gt3"));
    assert_eq!(mock.started_games().await, vec!["acc"]);

    // Stop ACC
    svc.handle_event(game_stopped_event("acc", "AC2-Win64-Shipping.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("global"));

    // Start iRacing
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );

    let starts = mock.started_games().await;
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[1], "iracing");

    Ok(())
}

/// Adapter registry consistency: all high-priority games can be instantiated.
#[test]
fn game_switch_adapter_registry_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let required = ["acc", "iracing", "gran_turismo_7", "f1_25", "rfactor2"];
    let factories = adapter_factories();

    for game_id in &required {
        let (_, factory) = factories
            .iter()
            .find(|(id, _)| id == game_id)
            .ok_or_else(|| format!("adapter '{game_id}' not in registry"))?;

        let a1 = factory();
        let a2 = factory();
        assert_eq!(a1.game_id(), a2.game_id(), "game_id must be stable");
    }
    Ok(())
}

/// All registered adapters produce valid NormalizedTelemetry for empty input
/// or return a meaningful error without panicking.
#[test]
fn game_switch_all_adapters_normalize_safely() -> Result<(), Box<dyn std::error::Error>> {
    let factories = adapter_factories();
    let empty_buf = vec![0u8; 4096];

    for (game_id, factory) in factories {
        let adapter = factory();
        // Must not panic regardless of outcome
        let result = adapter.normalize(&empty_buf);
        if let Ok(telem) = result {
            assert!(
                telem.speed_ms.is_finite(),
                "{game_id}: speed must be finite"
            );
            assert!(telem.rpm.is_finite(), "{game_id}: rpm must be finite");
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Telemetry recording during game session
// ═══════════════════════════════════════════════════════════════════════════════

/// Record telemetry frames during a session, then play back.
#[test]
fn game_telemetry_record_and_playback() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut recorder = TelemetryRecorder::new(tmp.path().join("recording.json"))?;

    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());

    // Record 50 frames
    for i in 0u64..50 {
        let frame = TelemetryFrame {
            data: NormalizedTelemetry {
                speed_ms: i as f32 * 0.5,
                rpm: 3000.0 + i as f32 * 50.0,
                ..Default::default()
            },
            timestamp_ns: i * 16_666_667, // ~60Hz
            sequence: i,
            raw_size: 0,
        };
        recorder.record_frame(frame);
    }

    assert_eq!(recorder.frame_count(), 50);
    let recording = recorder.stop_recording(Some("Test recording".to_string()))?;
    assert!(!recorder.is_recording());

    // Playback
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(10.0); // max speed for testing
    player.start_playback();

    // First frame (timestamp 0) is immediately available
    let first = player.get_next_frame();
    assert!(first.is_some(), "first frame must be available immediately");

    // Wait for remaining frames to become available at max playback speed
    std::thread::sleep(std::time::Duration::from_millis(200));
    let mut count = 1u64;
    while let Some(frame) = player.get_next_frame() {
        assert!(frame.data.rpm.is_finite());
        count += 1;
    }
    assert_eq!(count, 50, "all 50 frames must be played back");
    assert!(player.is_finished());

    Ok(())
}

/// Record with MockAdapter-generated telemetry.
#[test]
fn game_telemetry_record_mock_adapter_frames() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let adapter = MockAdapter::new("recording_test".to_string());
    let mut recorder = TelemetryRecorder::new(tmp.path().join("mock_recording.json"))?;

    recorder.start_recording("recording_test".to_string());

    for seq in 0u64..30 {
        let telemetry = adapter.normalize(&[])?;
        let frame = TelemetryFrame {
            data: telemetry,
            timestamp_ns: seq * 16_666_667,
            sequence: seq,
            raw_size: 0,
        };
        recorder.record_frame(frame);
    }

    assert_eq!(recorder.frame_count(), 30);
    let recording = recorder.stop_recording(Some("Mock recording".to_string()))?;

    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();

    let first = player.get_next_frame();
    assert!(first.is_some());

    Ok(())
}

/// Player reset allows re-reading the same recording.
#[test]
fn game_telemetry_player_reset_replays() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut recorder = TelemetryRecorder::new(tmp.path().join("reset_recording.json"))?;

    recorder.start_recording("reset_test".to_string());
    for i in 0u64..10 {
        let frame = TelemetryFrame {
            data: NormalizedTelemetry {
                speed_ms: i as f32,
                ..Default::default()
            },
            timestamp_ns: i * 16_666_667,
            sequence: i,
            raw_size: 0,
        };
        recorder.record_frame(frame);
    }
    let recording = recorder.stop_recording(None)?;

    let mut player = TelemetryPlayer::new(recording);

    // First pass: use max speed and wait for all frames
    player.set_playback_speed(10.0);
    player.start_playback();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let mut count = 0u64;
    while player.get_next_frame().is_some() {
        count += 1;
    }
    assert_eq!(count, 10);
    assert!(player.is_finished());

    // Reset and replay
    player.reset();
    player.set_playback_speed(10.0);
    player.start_playback();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let mut count2 = 0u64;
    while player.get_next_frame().is_some() {
        count2 += 1;
    }
    assert_eq!(count2, 10);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Multiple games trying to connect simultaneously
// ═══════════════════════════════════════════════════════════════════════════════

/// Two games detected in rapid succession; second takes over.
#[tokio::test]
async fn game_multi_simultaneous_second_takes_over() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "acc_gt3").await?;
    seed_profile(&profile_service, "iracing_gt3").await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("acc".to_string(), "acc_gt3".to_string())
        .await?;
    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    // Both games start
    svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
        .await;
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    // The last one should be active
    let active = svc.get_active_profile().await;
    assert_eq!(active.as_deref(), Some("iracing_gt3"));

    Ok(())
}

/// Start game → adapter starts → stop game → adapter stops cleanly.
#[tokio::test]
async fn game_multi_start_stop_adapter_lifecycle() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.handle_event(game_started_event("rf2", "rFactor2.exe"))
        .await;
    svc.handle_event(game_stopped_event("rf2", "rFactor2.exe"))
        .await;

    let starts = mock.started_games().await;
    let stops = mock.stopped_games().await;
    assert_eq!(starts, vec!["rf2"]);
    assert_eq!(stops, vec!["rf2"]);

    Ok(())
}

/// Three games in sequence: start→stop→start→stop→start→stop.
#[tokio::test]
async fn game_multi_three_sequential_sessions() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "global").await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    let sessions = [
        ("acc", "AC2-Win64-Shipping.exe"),
        ("iracing", "iRacingSim64DX11.exe"),
        ("rf2", "rFactor2.exe"),
    ];

    for (game_id, exe) in &sessions {
        svc.handle_event(game_started_event(game_id, exe)).await;
        svc.handle_event(game_stopped_event(game_id, exe)).await;
    }

    let starts = mock.started_games().await;
    let stops = mock.stopped_games().await;
    assert_eq!(starts.len(), 3);
    assert_eq!(stops.len(), 3);

    // After all sessions end, profile reverts to global
    let active = svc.get_active_profile().await;
    assert_eq!(active.as_deref(), Some("global"));

    Ok(())
}

/// GameService returns list of supported games.
#[tokio::test]
async fn game_service_lists_supported_games() -> anyhow::Result<()> {
    let service = GameService::new().await?;
    let supported = service.get_supported_games().await;
    assert!(
        !supported.is_empty(),
        "must have at least one supported game"
    );

    let stable = service.get_stable_games().await;
    // Stable games should be a subset
    for game in &stable {
        assert!(
            supported.contains(game),
            "{game} in stable but not in supported"
        );
    }

    Ok(())
}

/// GameService: setting and getting active game.
#[tokio::test]
async fn game_service_active_game_lifecycle() -> anyhow::Result<()> {
    let service = GameService::new().await?;

    assert!(service.get_active_game().await.is_none());

    service.set_active_game(Some("iracing".to_string())).await?;
    assert_eq!(service.get_active_game().await.as_deref(), Some("iracing"));

    service.set_active_game(None).await?;
    assert!(service.get_active_game().await.is_none());

    Ok(())
}
