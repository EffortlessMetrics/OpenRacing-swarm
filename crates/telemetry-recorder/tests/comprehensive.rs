//! Comprehensive integration tests for the openracing-telemetry-recorder crate.
//!
//! Exercises recording session creation, data persistence, playback,
//! fixture generation, and edge cases.

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryRecorder — creation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recorder_creation_succeeds() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("test.json");
    let recorder = TelemetryRecorder::new(path)?;
    assert!(!recorder.is_recording());
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn recorder_creates_parent_directories() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("nested").join("deep").join("test.json");
    let _recorder = TelemetryRecorder::new(path.clone())?;
    assert!(
        path.parent().is_some_and(|p| p.exists()),
        "parent directories should be created"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryRecorder — lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_lifecycle_basic() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("recording.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());
    assert_eq!(recorder.frame_count(), 0);

    let telemetry = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .speed_ms(30.0)
        .build();
    let frame = TelemetryFrame::new(telemetry, 1_000_000, 0, 64);
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 1);

    let recording = recorder.stop_recording(Some("Test".to_string()))?;
    assert!(!recorder.is_recording());
    assert_eq!(recording.frames.len(), 1);
    assert_eq!(recording.metadata.game_id, "test_game");
    assert_eq!(recording.metadata.frame_count, 1);
    assert!(recording.metadata.description.as_deref() == Some("Test"));
    Ok(())
}

#[test]
fn recording_multiple_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("multi.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("multi_test".to_string());
    for i in 0..10 {
        let t = NormalizedTelemetry::builder()
            .rpm(1000.0 + i as f32 * 500.0)
            .speed_ms(10.0 + i as f32 * 2.0)
            .gear((i as i8 % 6) + 1)
            .build();
        let frame = TelemetryFrame::new(t, i as u64 * 16_000_000, i as u64, 64);
        recorder.record_frame(frame);
    }
    assert_eq!(recorder.frame_count(), 10);

    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 10);
    assert_eq!(recording.metadata.frame_count, 10);
    Ok(())
}

#[test]
fn recording_frames_before_start_are_ignored() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("ignore.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    // Record frame before start — should be ignored
    let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn stop_recording_without_start_returns_error() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("no_start.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    let result = recorder.stop_recording(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn start_recording_clears_previous_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("clear.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    // First session: record some frames
    recorder.start_recording("game1".to_string());
    let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 1);

    // Start new session — should clear previous frames
    recorder.start_recording("game2".to_string());
    assert_eq!(recorder.frame_count(), 0);

    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.metadata.game_id, "game2");
    assert_eq!(recording.frames.len(), 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryRecorder — persistence (save + load)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn save_and_load_recording_roundtrip() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("roundtrip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("roundtrip_game".to_string());
    for i in 0..5 {
        let t = NormalizedTelemetry::builder()
            .rpm(3000.0 + i as f32 * 100.0)
            .speed_ms(20.0)
            .gear(3)
            .build();
        let frame = TelemetryFrame::new(t, i as u64 * 16_000_000, i as u64, 64);
        recorder.record_frame(frame);
    }
    let original = recorder.stop_recording(Some("Roundtrip test".to_string()))?;

    // Load from disk
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), original.frames.len());
    assert_eq!(loaded.metadata.game_id, "roundtrip_game");
    assert_eq!(loaded.metadata.frame_count, 5);
    assert!(loaded.metadata.description.as_deref() == Some("Roundtrip test"));

    // Verify frame data survived serialization
    for (orig, load) in original.frames.iter().zip(loaded.frames.iter()) {
        assert!((orig.data.rpm - load.data.rpm).abs() < 0.01);
        assert_eq!(orig.timestamp_ns, load.timestamp_ns);
        assert_eq!(orig.sequence, load.sequence);
    }
    Ok(())
}

#[test]
fn load_nonexistent_file_returns_error() -> TestResult {
    let result = TelemetryRecorder::load_recording("/nonexistent/path/recording.json");
    assert!(result.is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryRecording serialization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_serialization_roundtrip() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("serde_test".to_string(), 1.0, 10.0);
    let json = serde_json::to_string(&recording)?;
    let deserialized: TelemetryRecording = serde_json::from_str(&json)?;
    assert_eq!(deserialized.frames.len(), recording.frames.len());
    assert_eq!(deserialized.metadata.game_id, "serde_test");
    Ok(())
}

#[test]
fn recording_metadata_fields_preserved() -> TestResult {
    let metadata = RecordingMetadata {
        game_id: "test".to_string(),
        timestamp: 1700000000,
        duration_seconds: 10.5,
        frame_count: 42,
        average_fps: 60.0,
        car_id: Some("ferrari_488".to_string()),
        track_id: Some("spa".to_string()),
        description: Some("Test description".to_string()),
    };
    let json = serde_json::to_string(&metadata)?;
    let loaded: RecordingMetadata = serde_json::from_str(&json)?;
    assert_eq!(loaded.game_id, "test");
    assert_eq!(loaded.timestamp, 1700000000);
    assert!((loaded.duration_seconds - 10.5).abs() < 0.01);
    assert_eq!(loaded.frame_count, 42);
    assert!(loaded.car_id.as_deref() == Some("ferrari_488"));
    assert!(loaded.track_id.as_deref() == Some("spa"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryPlayer — playback
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn player_initial_state() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let player = TelemetryPlayer::new(recording);
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
    Ok(())
}

#[test]
fn player_empty_recording_is_finished() -> TestResult {
    let recording = TelemetryRecording {
        metadata: RecordingMetadata {
            game_id: "empty".to_string(),
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
    let player = TelemetryPlayer::new(recording);
    assert!(player.is_finished());
    assert_eq!(player.progress(), 1.0);
    Ok(())
}

#[test]
fn player_start_playback_and_get_first_frame() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 60.0);
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    // First frame should be available immediately (timestamp 0)
    let frame = player.get_next_frame();
    assert!(frame.is_some());
    Ok(())
}

#[test]
fn player_reset_restarts_playback() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    let _ = player.get_next_frame();
    player.reset();
    assert_eq!(player.progress(), 0.0);
    Ok(())
}

#[test]
fn player_set_playback_speed_clamped() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    // Below min → clamped to 0.1
    player.set_playback_speed(0.01);
    // Above max → clamped to 10.0
    player.set_playback_speed(100.0);
    // Valid speed
    player.set_playback_speed(2.0);
    Ok(())
}

#[test]
fn player_metadata_accessible() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("metadata_test".to_string(), 2.0, 30.0);
    let player = TelemetryPlayer::new(recording);
    let meta = player.metadata();
    assert_eq!(meta.game_id, "metadata_test");
    assert_eq!(meta.frame_count, 60);
    assert!((meta.average_fps - 30.0).abs() < 0.01);
    Ok(())
}

#[test]
fn player_get_next_frame_without_start_returns_none() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    // Not started yet
    let frame = player.get_next_frame();
    assert!(frame.is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TestFixtureGenerator — synthetic data
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fixture_racing_session_frame_count() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    assert_eq!(recording.metadata.frame_count, 120);
    assert_eq!(recording.frames.len(), 120);
    Ok(())
}

#[test]
fn fixture_racing_session_frames_have_positive_values() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    for frame in &recording.frames {
        assert!(frame.data.rpm > 0.0, "rpm should be positive");
        assert!(frame.data.speed_ms > 0.0, "speed should be positive");
    }
    Ok(())
}

#[test]
fn fixture_racing_session_timestamps_monotonic() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    for pair in recording.frames.windows(2) {
        assert!(
            pair[1].timestamp_ns >= pair[0].timestamp_ns,
            "timestamps should be monotonically increasing"
        );
    }
    Ok(())
}

#[test]
fn fixture_racing_session_sequences_monotonic() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    for pair in recording.frames.windows(2) {
        assert!(
            pair[1].sequence > pair[0].sequence,
            "sequences should be strictly increasing"
        );
    }
    Ok(())
}

#[test]
fn fixture_racing_session_has_car_and_track() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    assert!(recording.metadata.car_id.is_some());
    assert!(recording.metadata.track_id.is_some());
    Ok(())
}

#[test]
fn fixture_zero_duration_produces_no_frames() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 0.0, 60.0);
    assert_eq!(recording.frames.len(), 0);
    assert_eq!(recording.metadata.frame_count, 0);
    Ok(())
}

#[test]
fn fixture_zero_fps_produces_no_frames() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 5.0, 0.0);
    assert_eq!(recording.frames.len(), 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TestFixtureGenerator — test scenarios
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_constant_speed_frames_have_uniform_speed() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 1.0, 10.0);
    for frame in &recording.frames {
        assert!(
            (frame.data.speed_ms - 50.0).abs() < 0.01,
            "constant speed should be 50.0 m/s"
        );
    }
    Ok(())
}

#[test]
fn scenario_acceleration_speed_increases() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 2.0, 30.0);
    assert!(!recording.frames.is_empty());
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
        "speed should increase during acceleration: first={first_speed}, last={last_speed}"
    );
    Ok(())
}

#[test]
fn scenario_cornering_high_ffb() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 10.0);
    for frame in &recording.frames {
        assert!(
            (frame.data.ffb_scalar - 0.9).abs() < 0.01,
            "cornering should have high FFB"
        );
    }
    Ok(())
}

#[test]
fn scenario_pitstop_has_pit_flags() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);
    let in_pit_count = recording
        .frames
        .iter()
        .filter(|f| f.data.flags.in_pits)
        .count();
    assert!(
        in_pit_count > 0,
        "pit stop scenario should have frames with in_pits flag"
    );
    let not_in_pit = recording
        .frames
        .iter()
        .filter(|f| !f.data.flags.in_pits)
        .count();
    assert!(
        not_in_pit > 0,
        "pit stop scenario should have frames outside pits too"
    );
    Ok(())
}

#[test]
fn scenario_pitstop_pit_limiter_matches_in_pits() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);
    for frame in &recording.frames {
        assert_eq!(
            frame.data.flags.pit_limiter, frame.data.flags.in_pits,
            "pit_limiter should match in_pits"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge cases: NormalizedTelemetry builder in recorder context
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_with_flags() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("flags.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("flags_test".to_string());
    let flags = TelemetryFlags {
        yellow_flag: true,
        in_pits: true,
        ..TelemetryFlags::default()
    };
    let t = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .flags(flags)
        .build();
    let frame = TelemetryFrame::new(t, 0, 0, 64);
    recorder.record_frame(frame);
    let recording = recorder.stop_recording(None)?;

    // Verify flags survive save/load
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert!(loaded.frames[0].data.flags.yellow_flag);
    assert!(loaded.frames[0].data.flags.in_pits);
    assert!(recording.frames[0].data.flags.yellow_flag);
    Ok(())
}

#[test]
fn recording_with_car_and_track_metadata() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("meta_test".to_string());
    let t = NormalizedTelemetry::builder()
        .rpm(3000.0)
        .car_id("porsche_911_gt3")
        .track_id("nurburgring_gp")
        .build();
    let frame = TelemetryFrame::new(t, 0, 0, 64);
    recorder.record_frame(frame);
    let recording = recorder.stop_recording(None)?;

    assert!(recording.metadata.car_id.as_deref() == Some("porsche_911_gt3"));
    assert!(recording.metadata.track_id.as_deref() == Some("nurburgring_gp"));

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert!(loaded.metadata.car_id.as_deref() == Some("porsche_911_gt3"));
    assert!(loaded.metadata.track_id.as_deref() == Some("nurburgring_gp"));
    Ok(())
}

#[test]
fn recording_metadata_without_description() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("no_desc.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert!(recording.metadata.description.is_none());
    Ok(())
}
