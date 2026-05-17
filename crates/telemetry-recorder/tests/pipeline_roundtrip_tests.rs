//! Pipeline roundtrip tests for telemetry-recorder.
//!
//! Covers gaps in existing tests:
//! - Full save → load → verify data integrity roundtrip
//! - Empty recording edge cases
//! - Stop-without-start error path
//! - Recording frame without active session
//! - Timestamp monotonicity in generated recordings
//! - Player edge cases (empty recording, speed clamping)
//! - All TestScenario variants produce valid data
//! - Metadata field preservation across save/load

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Save/load roundtrip integrity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn save_and_load_roundtrip_preserves_frame_data() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("roundtrip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("iracing".to_string());

    let frames: Vec<TelemetryFrame> = (0..5)
        .map(|i| {
            let data = NormalizedTelemetry::builder()
                .rpm(3000.0 + i as f32 * 500.0)
                .speed_ms(20.0 + i as f32 * 5.0)
                .gear((i as i8 % 6) + 1)
                .car_id("bmw_m4_gt3")
                .track_id("spa")
                .build();
            TelemetryFrame::new(data, i as u64 * 1_000_000, i as u64, 128)
        })
        .collect();

    for frame in &frames {
        recorder.record_frame(frame.clone());
    }

    let recording = recorder.stop_recording(Some("roundtrip test".to_string()))?;
    assert_eq!(recording.frames.len(), 5);

    // Load from disk and verify
    let loaded = TelemetryRecorder::load_recording(&path)?;

    assert_eq!(loaded.metadata.game_id, "iracing");
    assert_eq!(loaded.metadata.frame_count, 5);
    assert_eq!(
        loaded.metadata.description.as_deref(),
        Some("roundtrip test")
    );
    assert_eq!(loaded.metadata.car_id.as_deref(), Some("bmw_m4_gt3"));
    assert_eq!(loaded.metadata.track_id.as_deref(), Some("spa"));
    assert_eq!(loaded.frames.len(), 5);

    for (i, (orig, loaded_frame)) in frames.iter().zip(loaded.frames.iter()).enumerate() {
        assert_eq!(
            loaded_frame.timestamp_ns, orig.timestamp_ns,
            "frame {i} timestamp mismatch"
        );
        assert_eq!(
            loaded_frame.sequence, orig.sequence,
            "frame {i} sequence mismatch"
        );
        assert_eq!(
            loaded_frame.data.rpm, orig.data.rpm,
            "frame {i} rpm mismatch"
        );
        assert_eq!(
            loaded_frame.data.speed_ms, orig.data.speed_ms,
            "frame {i} speed mismatch"
        );
        assert_eq!(
            loaded_frame.data.gear, orig.data.gear,
            "frame {i} gear mismatch"
        );
    }
    Ok(())
}

#[test]
fn save_and_load_roundtrip_with_flags() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("flags_roundtrip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("acc".to_string());

    let flags = TelemetryFlags {
        yellow_flag: true,
        in_pits: true,
        pit_limiter: true,
        abs_active: true,
        traction_control: true,
        ..TelemetryFlags::default()
    };
    let data = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .flags(flags)
        .build();
    let frame = TelemetryFrame::new(data, 100_000, 0, 64);
    recorder.record_frame(frame);

    let _recording = recorder.stop_recording(None)?;
    let loaded = TelemetryRecorder::load_recording(&path)?;

    assert_eq!(loaded.frames.len(), 1);
    let loaded_flags = &loaded.frames[0].data.flags;
    assert!(loaded_flags.yellow_flag);
    assert!(loaded_flags.in_pits);
    assert!(loaded_flags.pit_limiter);
    assert!(loaded_flags.abs_active);
    assert!(loaded_flags.traction_control);
    assert!(!loaded_flags.red_flag);
    assert!(!loaded_flags.drs_active);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error paths
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stop_without_start_returns_error() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("no_start.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    let result = recorder.stop_recording(None);
    assert!(result.is_err(), "stop without start should error");
    Ok(())
}

#[test]
fn record_frame_without_start_is_ignored() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("no_record.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    let data = NormalizedTelemetry::builder().rpm(1000.0).build();
    let frame = TelemetryFrame::new(data, 0, 0, 64);
    recorder.record_frame(frame);

    assert_eq!(
        recorder.frame_count(),
        0,
        "frames before start_recording should be dropped"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Empty recording
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_recording_save_and_load() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("empty.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("test_game".to_string());
    let recording = recorder.stop_recording(None)?;

    assert_eq!(recording.frames.len(), 0);
    assert_eq!(recording.metadata.frame_count, 0);

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 0);
    assert_eq!(loaded.metadata.game_id, "test_game");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Timestamp monotonicity in generated fixtures
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn generated_fixture_timestamps_are_monotonically_increasing() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 5.0, 60.0);

    assert!(!recording.frames.is_empty());

    let mut prev_ts = 0u64;
    for (i, frame) in recording.frames.iter().enumerate() {
        assert!(
            frame.timestamp_ns >= prev_ts,
            "frame {i}: timestamp {ts} < previous {prev_ts}",
            ts = frame.timestamp_ns
        );
        prev_ts = frame.timestamp_ns;
    }
    Ok(())
}

#[test]
fn generated_fixture_sequences_are_monotonically_increasing() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 120.0);

    let mut prev_seq = 0u64;
    for (i, frame) in recording.frames.iter().enumerate() {
        if i > 0 {
            assert!(
                frame.sequence > prev_seq,
                "frame {i}: sequence {seq} <= previous {prev_seq}",
                seq = frame.sequence
            );
        }
        prev_seq = frame.sequence;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// All TestScenario variants produce valid data
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_test_scenarios_produce_nonempty_recordings() -> TestResult {
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];

    for scenario in &scenarios {
        let recording = TestFixtureGenerator::generate_test_scenario(*scenario, 1.0, 30.0);
        assert!(
            !recording.frames.is_empty(),
            "{scenario:?} produced empty recording"
        );
        assert!(
            recording.metadata.description.is_some(),
            "{scenario:?} missing description"
        );

        for frame in &recording.frames {
            assert!(frame.data.rpm >= 0.0, "{scenario:?}: negative RPM");
            assert!(frame.data.speed_ms >= 0.0, "{scenario:?}: negative speed");
        }
    }
    Ok(())
}

#[test]
fn pitstop_scenario_has_in_pits_flag_set() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);

    let in_pit_frames = recording
        .frames
        .iter()
        .filter(|f| f.data.flags.in_pits)
        .count();

    assert!(
        in_pit_frames > 0,
        "pit stop scenario should have at least one frame with in_pits=true"
    );

    let not_in_pit_frames = recording
        .frames
        .iter()
        .filter(|f| !f.data.flags.in_pits)
        .count();

    assert!(
        not_in_pit_frames > 0,
        "pit stop scenario should have frames outside pits"
    );
    Ok(())
}

#[test]
fn constant_speed_scenario_has_uniform_speed() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 1.0, 30.0);

    let speeds: Vec<f32> = recording.frames.iter().map(|f| f.data.speed_ms).collect();

    // All frames should have identical speed (50.0)
    for (i, speed) in speeds.iter().enumerate() {
        assert!(
            (*speed - 50.0).abs() < 0.01,
            "frame {i}: speed {speed} != 50.0"
        );
    }
    Ok(())
}

#[test]
fn acceleration_scenario_speed_increases() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 2.0, 30.0);

    let first_speed = recording
        .frames
        .first()
        .map(|f| f.data.speed_ms)
        .unwrap_or(0.0);
    let last_speed = recording
        .frames
        .last()
        .map(|f| f.data.speed_ms)
        .unwrap_or(0.0);

    assert!(
        last_speed > first_speed,
        "acceleration scenario should increase speed: first={first_speed}, last={last_speed}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryPlayer edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn player_empty_recording_is_immediately_finished() -> TestResult {
    let recording = TelemetryRecording {
        metadata: RecordingMetadata {
            game_id: "test".to_string(),
            timestamp: 0,
            duration_seconds: 0.0,
            frame_count: 0,
            average_fps: 0.0,
            car_id: None,
            track_id: None,
            description: None,
        },
        frames: vec![],
    };

    let mut player = TelemetryPlayer::new(recording);
    // progress() should be 1.0 for empty recording
    assert_eq!(player.progress(), 1.0);
    assert!(player.is_finished());

    player.start_playback();
    assert!(player.get_next_frame().is_none());
    Ok(())
}

#[test]
fn player_speed_clamping() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    player.set_playback_speed(0.01); // below min
    player.set_playback_speed(100.0); // above max

    // Just verify it doesn't panic and metadata is accessible
    let _ = player.metadata();
    Ok(())
}

#[test]
fn player_reset_restarts_progress() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    player.start_playback();
    // Allow some frames to play
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = player.get_next_frame();

    player.reset();
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
    Ok(())
}

#[test]
fn player_metadata_matches_recording() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("f1_2024".to_string(), 3.0, 60.0);
    let player = TelemetryPlayer::new(recording);

    let meta = player.metadata();
    assert_eq!(meta.game_id, "f1_2024");
    assert_eq!(meta.frame_count, 180);
    assert!((meta.duration_seconds - 3.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Large recording save/load
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn large_recording_save_and_load() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("large.json");

    // Generate a 30-second recording at 60fps = 1800 frames
    let recording = TestFixtureGenerator::generate_racing_session("acc".to_string(), 30.0, 60.0);
    assert_eq!(recording.frames.len(), 1800);

    // Save manually using JSON
    let file = std::fs::File::create(&path)?;
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer(writer, &recording)?;

    // Load and verify
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 1800);
    assert_eq!(loaded.metadata.game_id, "acc");

    // Spot-check first and last frame
    assert_eq!(
        loaded.frames[0].timestamp_ns,
        recording.frames[0].timestamp_ns
    );
    assert_eq!(
        loaded.frames[1799].timestamp_ns,
        recording.frames[1799].timestamp_ns
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording JSON structure snapshot
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_json_has_expected_top_level_keys() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 0.1, 10.0);

    let json_value: serde_json::Value = serde_json::to_value(&recording)?;
    let obj = json_value
        .as_object()
        .ok_or("top-level should be an object")?;

    assert!(obj.contains_key("metadata"), "missing 'metadata' key");
    assert!(obj.contains_key("frames"), "missing 'frames' key");

    let metadata = obj["metadata"]
        .as_object()
        .ok_or("metadata should be an object")?;
    assert!(metadata.contains_key("game_id"));
    assert!(metadata.contains_key("timestamp"));
    assert!(metadata.contains_key("frame_count"));
    assert!(metadata.contains_key("average_fps"));
    Ok(())
}
