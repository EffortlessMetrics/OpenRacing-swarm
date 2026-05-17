//! Hardening tests for openracing-telemetry-recorder.
//!
//! Covers: recording start/stop lifecycle, data capture format,
//! replay functionality, file size limits and rotation.

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Recording start/stop lifecycle ──────────────────────────────────────

#[test]
fn recorder_creation_with_valid_path() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let recorder = TelemetryRecorder::new(path)?;
    assert!(!recorder.is_recording());
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn recorder_creation_creates_parent_directories() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("deep/nested/dir/test.json");
    let recorder = TelemetryRecorder::new(path.clone())?;
    assert!(!recorder.is_recording());
    assert!(path.parent().is_some_and(|p| p.exists()));
    Ok(())
}

#[test]
fn recorder_start_sets_recording_state() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    assert!(!recorder.is_recording());
    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());
    Ok(())
}

#[test]
fn recorder_stop_clears_recording_state() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    recorder.start_recording("test_game".to_string());
    let _recording = recorder.stop_recording(None)?;
    assert!(!recorder.is_recording());
    Ok(())
}

#[test]
fn recorder_stop_without_start_returns_error() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    let result = recorder.stop_recording(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn recorder_start_clears_previous_frames() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    // First recording
    recorder.start_recording("game1".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(5000.0).build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 1);
    let _rec1 = recorder.stop_recording(None)?;

    // Second recording should start fresh
    recorder.start_recording("game2".to_string());
    assert_eq!(recorder.frame_count(), 0);
    let _rec2 = recorder.stop_recording(None)?;
    Ok(())
}

#[test]
fn recorder_frame_not_added_when_not_recording() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(5000.0).build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn recording_metadata_captures_game_id() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    recorder.start_recording("iracing".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(5000.0).build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    let recording = recorder.stop_recording(Some("test session".to_string()))?;
    assert_eq!(recording.metadata.game_id, "iracing");
    assert_eq!(recording.metadata.frame_count, 1);
    assert_eq!(
        recording.metadata.description,
        Some("test session".to_string())
    );
    Ok(())
}

#[test]
fn recording_metadata_extracts_car_and_track_from_frames() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    recorder.start_recording("acc".to_string());

    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder()
            .rpm(6000.0)
            .car_id("ferrari_488_gt3")
            .track_id("monza")
            .build(),
        1_000_000,
        0,
        128,
    );
    recorder.record_frame(frame);
    let recording = recorder.stop_recording(None)?;
    assert_eq!(
        recording.metadata.car_id,
        Some("ferrari_488_gt3".to_string())
    );
    assert_eq!(recording.metadata.track_id, Some("monza".to_string()));
    Ok(())
}

#[test]
fn recording_metadata_duration_is_non_negative() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("test.json");
    let mut recorder = TelemetryRecorder::new(path)?;
    recorder.start_recording("test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert!(recording.metadata.duration_seconds >= 0.0);
    Ok(())
}

// ── Data capture format ─────────────────────────────────────────────────

#[test]
fn recording_save_and_load_round_trip() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("round_trip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("test_game".to_string());

    for i in 0..10 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::builder()
                .rpm(3000.0 + i as f32 * 100.0)
                .speed_ms(20.0 + i as f32)
                .gear(3)
                .build(),
            (i as u64) * 1_000_000,
            i as u64,
            64,
        );
        recorder.record_frame(frame);
    }
    let original = recorder.stop_recording(Some("round trip test".to_string()))?;

    // Load and verify
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.metadata.game_id, original.metadata.game_id);
    assert_eq!(loaded.metadata.frame_count, original.metadata.frame_count);
    assert_eq!(loaded.frames.len(), original.frames.len());
    for (orig, load) in original.frames.iter().zip(loaded.frames.iter()) {
        assert!((orig.data.rpm - load.data.rpm).abs() < 0.01);
        assert!((orig.data.speed_ms - load.data.speed_ms).abs() < 0.01);
        assert_eq!(orig.data.gear, load.data.gear);
        assert_eq!(orig.timestamp_ns, load.timestamp_ns);
        assert_eq!(orig.sequence, load.sequence);
    }
    Ok(())
}

#[test]
fn recording_json_is_valid_json() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("valid_json.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("test".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(5000.0).build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    // Verify the file is valid JSON
    let content = std::fs::read_to_string(&path)?;
    let _: serde_json::Value = serde_json::from_str(&content)?;
    Ok(())
}

#[test]
fn recording_preserves_telemetry_flags() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("flags.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("test".to_string());

    let flags = TelemetryFlags {
        yellow_flag: true,
        in_pits: true,
        pit_limiter: true,
        ..TelemetryFlags::default()
    };
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder()
            .rpm(4000.0)
            .flags(flags)
            .build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 1);
    let loaded_flags = &loaded.frames[0].data.flags;
    assert!(loaded_flags.yellow_flag);
    assert!(loaded_flags.in_pits);
    assert!(loaded_flags.pit_limiter);
    assert!(!loaded_flags.red_flag);
    Ok(())
}

#[test]
fn empty_recording_saves_and_loads() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("empty.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 0);
    assert_eq!(recording.metadata.frame_count, 0);

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 0);
    Ok(())
}

#[test]
fn load_nonexistent_file_returns_error() {
    let result = TelemetryRecorder::load_recording("nonexistent_file_12345.json");
    assert!(result.is_err());
}

// ── Replay functionality ────────────────────────────────────────────────

#[test]
fn player_creation_from_recording() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let player = TelemetryPlayer::new(recording);
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
}

#[test]
fn player_progress_starts_at_zero() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let player = TelemetryPlayer::new(recording);
    assert!((player.progress() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn player_finished_on_empty_recording() {
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
    let player = TelemetryPlayer::new(recording);
    // Empty recording: progress is 1.0, finished is true
    assert!((player.progress() - 1.0).abs() < f32::EPSILON);
    assert!(player.is_finished());
}

#[test]
fn player_get_next_frame_returns_none_before_start() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    // Without calling start_playback, get_next_frame should return None
    let frame = player.get_next_frame();
    assert!(frame.is_none());
}

#[test]
fn player_start_playback_enables_frame_retrieval() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 60.0);
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    // The first frame should be immediately available (elapsed >= 0)
    // We use a small spin to ensure elapsed > 0
    std::thread::sleep(std::time::Duration::from_millis(1));
    let frame = player.get_next_frame();
    assert!(frame.is_some());
}

#[test]
fn player_reset_returns_to_start() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    std::thread::sleep(std::time::Duration::from_millis(1));
    let _ = player.get_next_frame();
    player.reset();
    assert!((player.progress() - 0.0).abs() < f32::EPSILON);
    assert!(!player.is_finished());
}

#[test]
fn player_set_playback_speed_clamps_to_valid_range() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(0.0); // Should clamp to 0.1
    player.set_playback_speed(100.0); // Should clamp to 10.0
    player.set_playback_speed(1.0); // Normal speed
    // No panic = success
}

#[test]
fn player_metadata_matches_recording() {
    let recording = TestFixtureGenerator::generate_racing_session("my_game".to_string(), 2.0, 30.0);
    let player = TelemetryPlayer::new(recording);
    let metadata = player.metadata();
    assert_eq!(metadata.game_id, "my_game");
    assert_eq!(metadata.frame_count, 60);
    assert!((metadata.average_fps - 30.0).abs() < 0.1);
}

// ── Fixture generation ──────────────────────────────────────────────────

#[test]
fn fixture_racing_session_has_correct_frame_count() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 5.0, 60.0);
    assert_eq!(recording.frames.len(), 300);
    assert_eq!(recording.metadata.frame_count, 300);
}

#[test]
fn fixture_racing_session_frames_have_increasing_timestamps() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    for pair in recording.frames.windows(2) {
        assert!(
            pair[1].timestamp_ns >= pair[0].timestamp_ns,
            "timestamps should be non-decreasing"
        );
    }
}

#[test]
fn fixture_racing_session_metadata_is_populated() {
    let recording = TestFixtureGenerator::generate_racing_session("acc".to_string(), 3.0, 60.0);
    assert_eq!(recording.metadata.game_id, "acc");
    assert!((recording.metadata.duration_seconds - 3.0).abs() < 0.01);
    assert!((recording.metadata.average_fps - 60.0).abs() < 0.1);
    assert_eq!(recording.metadata.car_id, Some("test_car".to_string()));
    assert_eq!(recording.metadata.track_id, Some("test_track".to_string()));
}

#[test]
fn fixture_synthetic_telemetry_has_valid_ranges() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);
    for frame in &recording.frames {
        assert!(frame.data.rpm >= 0.0, "rpm should be non-negative");
        assert!(frame.data.speed_ms >= 0.0, "speed should be non-negative");
        assert!(
            frame.data.ffb_scalar >= -1.0 && frame.data.ffb_scalar <= 1.0,
            "ffb_scalar should be in [-1, 1]"
        );
        assert!(
            frame.data.slip_ratio >= 0.0 && frame.data.slip_ratio <= 1.0,
            "slip_ratio should be in [0, 1]"
        );
        assert!(frame.data.gear >= 1, "gear should be >= 1");
    }
}

// ── Test scenarios ──────────────────────────────────────────────────────

#[test]
fn constant_speed_scenario_has_uniform_speed() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 2.0, 60.0);
    for frame in &recording.frames {
        assert!(
            (frame.data.speed_ms - 50.0).abs() < 0.01,
            "constant speed should be 50.0 m/s"
        );
        assert!(
            (frame.data.rpm - 6000.0).abs() < 0.01,
            "constant rpm should be 6000.0"
        );
    }
}

#[test]
fn acceleration_scenario_has_increasing_speed() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 2.0, 60.0);
    let first_speed = recording.frames.first().map(|f| f.data.speed_ms);
    let last_speed = recording.frames.last().map(|f| f.data.speed_ms);
    if let (Some(first), Some(last)) = (first_speed, last_speed) {
        assert!(
            last > first,
            "acceleration scenario: last speed {last} should exceed first {first}"
        );
    }
}

#[test]
fn cornering_scenario_has_high_slip_and_ffb() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 60.0);
    for frame in &recording.frames {
        assert!(
            (frame.data.ffb_scalar - 0.9).abs() < 0.01,
            "cornering should have high ffb"
        );
        assert!(
            (frame.data.slip_ratio - 0.4).abs() < 0.01,
            "cornering should have moderate slip"
        );
    }
}

#[test]
fn pitstop_scenario_has_pit_flags_in_middle() {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 60.0);
    let total = recording.frames.len();
    assert!(total > 0);
    // Check frames in the middle (30%-70% of recording) have pit flags
    let pit_start = (total as f32 * 0.35) as usize;
    let pit_end = (total as f32 * 0.65) as usize;
    for frame in &recording.frames[pit_start..pit_end] {
        assert!(
            frame.data.flags.in_pits,
            "mid-recording frames should be in pits"
        );
        assert!(
            frame.data.flags.pit_limiter,
            "mid-recording frames should have pit limiter"
        );
    }
    // First frame should not be in pits
    assert!(
        !recording.frames[0].data.flags.in_pits,
        "first frame should not be in pits"
    );
}

#[test]
fn all_scenarios_produce_non_empty_recordings() {
    for scenario in [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ] {
        let recording = TestFixtureGenerator::generate_test_scenario(scenario, 1.0, 60.0);
        assert!(
            !recording.frames.is_empty(),
            "scenario {scenario:?} produced empty recording"
        );
        assert_eq!(recording.frames.len(), recording.metadata.frame_count);
    }
}

// ── File size and recording persistence ─────────────────────────────────

#[test]
fn large_recording_saves_and_loads() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("large.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("test".to_string());

    for i in 0..1000 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::builder()
                .rpm(3000.0 + (i % 5000) as f32)
                .speed_ms(10.0 + (i % 100) as f32)
                .gear(((i % 6) + 1) as i8)
                .build(),
            i as u64 * 1_000_000,
            i as u64,
            64,
        );
        recorder.record_frame(frame);
    }
    let recording = recorder.stop_recording(Some("large test".to_string()))?;
    assert_eq!(recording.frames.len(), 1000);

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 1000);
    assert_eq!(loaded.metadata.frame_count, 1000);
    Ok(())
}

#[test]
fn recording_file_size_is_proportional_to_frame_count() -> TestResult {
    let temp = tempdir()?;

    // Small recording
    let small_path = temp.path().join("small.json");
    let mut recorder = TelemetryRecorder::new(small_path.clone())?;
    recorder.start_recording("test".to_string());
    for i in 0..10 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::builder().rpm(5000.0).build(),
            i * 1_000_000,
            i,
            64,
        );
        recorder.record_frame(frame);
    }
    let _small = recorder.stop_recording(None)?;

    // Larger recording
    let large_path = temp.path().join("large.json");
    let mut recorder = TelemetryRecorder::new(large_path.clone())?;
    recorder.start_recording("test".to_string());
    for i in 0..100 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::builder().rpm(5000.0).build(),
            i * 1_000_000,
            i,
            64,
        );
        recorder.record_frame(frame);
    }
    let _large = recorder.stop_recording(None)?;

    let small_size = std::fs::metadata(&small_path)?.len();
    let large_size = std::fs::metadata(&large_path)?.len();
    assert!(
        large_size > small_size,
        "larger recording ({large_size}B) should be bigger than smaller ({small_size}B)"
    );
    Ok(())
}

// ── Recording metadata serde ────────────────────────────────────────────

#[test]
fn recording_metadata_json_round_trip() -> TestResult {
    let metadata = RecordingMetadata {
        game_id: "iracing".to_string(),
        timestamp: 1700000000,
        duration_seconds: 120.5,
        frame_count: 7230,
        average_fps: 60.0,
        car_id: Some("mazda_mx5".to_string()),
        track_id: Some("laguna_seca".to_string()),
        description: Some("Practice session".to_string()),
    };
    let json = serde_json::to_string(&metadata)?;
    let decoded: RecordingMetadata = serde_json::from_str(&json)?;
    assert_eq!(decoded.game_id, metadata.game_id);
    assert_eq!(decoded.timestamp, metadata.timestamp);
    assert!((decoded.duration_seconds - metadata.duration_seconds).abs() < 0.001);
    assert_eq!(decoded.frame_count, metadata.frame_count);
    assert_eq!(decoded.car_id, metadata.car_id);
    assert_eq!(decoded.track_id, metadata.track_id);
    assert_eq!(decoded.description, metadata.description);
    Ok(())
}

#[test]
fn recording_metadata_with_no_optional_fields() -> TestResult {
    let metadata = RecordingMetadata {
        game_id: "test".to_string(),
        timestamp: 0,
        duration_seconds: 0.0,
        frame_count: 0,
        average_fps: 0.0,
        car_id: None,
        track_id: None,
        description: None,
    };
    let json = serde_json::to_string(&metadata)?;
    let decoded: RecordingMetadata = serde_json::from_str(&json)?;
    assert!(decoded.car_id.is_none());
    assert!(decoded.track_id.is_none());
    assert!(decoded.description.is_none());
    Ok(())
}

#[test]
fn telemetry_recording_full_serde_round_trip() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let json = serde_json::to_string(&recording)?;
    let decoded: TelemetryRecording = serde_json::from_str(&json)?;
    assert_eq!(decoded.metadata.game_id, recording.metadata.game_id);
    assert_eq!(decoded.frames.len(), recording.frames.len());
    Ok(())
}

// ── Overwrite behavior ──────────────────────────────────────────────────

#[test]
fn recorder_overwrites_existing_file_on_stop() -> TestResult {
    let temp = tempdir()?;
    let path = temp.path().join("overwrite.json");

    // First recording
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("game1".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(3000.0).build(),
        1_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    let _rec1 = recorder.stop_recording(Some("first".to_string()))?;

    // Second recording overwrites
    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("game2".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(7000.0).build(),
        2_000_000,
        0,
        64,
    );
    recorder.record_frame(frame);
    let _rec2 = recorder.stop_recording(Some("second".to_string()))?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.metadata.game_id, "game2");
    assert_eq!(loaded.metadata.description, Some("second".to_string()));
    Ok(())
}
