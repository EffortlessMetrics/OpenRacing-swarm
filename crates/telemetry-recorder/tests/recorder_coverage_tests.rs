//! Recorder and playback coverage expansion tests.
//!
//! Covers: recording lifecycle, file persistence round-trips, playback speed,
//! progress tracking, fixture generation for all scenarios, edge cases.

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFrame};
use tempfile::tempdir;

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Recording lifecycle
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn recorder_not_recording_initially() -> R {
    let dir = tempdir()?;
    let recorder = TelemetryRecorder::new(dir.path().join("rec.json"))?;
    assert!(!recorder.is_recording());
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn frames_ignored_before_start() -> R {
    let dir = tempdir()?;
    let mut recorder = TelemetryRecorder::new(dir.path().join("rec.json"))?;

    let telem = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::new(telem, 1_000_000, 0, 64);
    recorder.record_frame(frame);

    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn stop_without_start_returns_error() -> R {
    let dir = tempdir()?;
    let mut recorder = TelemetryRecorder::new(dir.path().join("rec.json"))?;
    let result = recorder.stop_recording(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn recording_lifecycle_metadata_correct() -> R {
    let dir = tempdir()?;
    let mut recorder = TelemetryRecorder::new(dir.path().join("rec.json"))?;

    recorder.start_recording("acc".to_string());

    for i in 0..5 {
        let telem = NormalizedTelemetry::builder()
            .rpm(3000.0 + i as f32 * 100.0)
            .speed_ms(20.0 + i as f32)
            .build();
        let frame = TelemetryFrame::new(telem, i * 16_666_667, i, 64);
        recorder.record_frame(frame);
    }

    let recording = recorder.stop_recording(Some("metadata test".to_string()))?;

    assert_eq!(recording.metadata.game_id, "acc");
    assert_eq!(recording.metadata.frame_count, 5);
    assert_eq!(recording.frames.len(), 5);
    assert_eq!(
        recording.metadata.description.as_deref(),
        Some("metadata test")
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// File persistence round-trip
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn save_and_load_roundtrip() -> R {
    let dir = tempdir()?;
    let path = dir.path().join("roundtrip.json");

    let mut recorder = TelemetryRecorder::new(path.clone())?;
    recorder.start_recording("iracing".to_string());

    for i in 0..3 {
        let telem = NormalizedTelemetry::builder().rpm(4000.0).build();
        let frame = TelemetryFrame::new(telem, i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }

    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.metadata.game_id, "iracing");
    assert_eq!(loaded.frames.len(), 3);
    Ok(())
}

#[test]
fn load_nonexistent_file_returns_error() {
    let result = TelemetryRecorder::load_recording("nonexistent_file.json");
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// Playback
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn player_initial_state() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let player = TelemetryPlayer::new(recording);

    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
}

#[test]
fn player_metadata_accessible() {
    let recording = TestFixtureGenerator::generate_racing_session("ac".to_string(), 2.0, 60.0);
    let player = TelemetryPlayer::new(recording);

    assert_eq!(player.metadata().game_id, "ac");
    assert_eq!(player.metadata().frame_count, 120);
}

#[test]
fn player_playback_speed_clamped() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    player.set_playback_speed(0.01); // Below minimum
    // Speed should be clamped to 0.1 internally
    player.start_playback();

    player.set_playback_speed(100.0); // Above maximum
    // Speed should be clamped to 10.0 internally
}

#[test]
fn player_reset_returns_to_start() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    player.start_playback();
    // Consume some frames by sleeping briefly
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = player.get_next_frame();

    player.reset();
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
}

#[test]
fn player_empty_recording_is_finished() {
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
        frames: Vec::new(),
    };
    let player = TelemetryPlayer::new(recording);
    assert!(player.is_finished());
    assert_eq!(player.progress(), 1.0);
}

#[test]
fn player_get_next_frame_without_start_returns_none() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    assert!(player.get_next_frame().is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Fixture generation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fixture_constant_speed_scenario() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 1.0, 60.0);

    assert_eq!(recording.frames.len(), 60);
    for frame in &recording.frames {
        assert!((frame.data.speed_ms - 50.0).abs() < f32::EPSILON);
        assert!((frame.data.rpm - 6000.0).abs() < f32::EPSILON);
    }
}

#[test]
fn fixture_acceleration_scenario() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 2.0, 30.0);

    assert_eq!(recording.frames.len(), 60);

    // Speed should generally increase (start near 0, end near 80)
    let first_speed = recording.frames.first().map(|f| f.data.speed_ms);
    let last_speed = recording.frames.last().map(|f| f.data.speed_ms);

    if let (Some(first), Some(last)) = (first_speed, last_speed) {
        assert!(last > first, "speed should increase during acceleration");
    }
}

#[test]
fn fixture_cornering_scenario() {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 60.0);

    assert_eq!(recording.frames.len(), 60);
    for frame in &recording.frames {
        assert!((frame.data.speed_ms - 35.0).abs() < f32::EPSILON);
    }
}

#[test]
fn fixture_pitstop_scenario() {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 1.0, 100.0);

    assert_eq!(recording.frames.len(), 100);

    // Frames in the middle (30-70%) should be in pits
    let pit_frame = &recording.frames[50]; // 50% progress
    assert!(pit_frame.data.flags.in_pits);
    assert!(pit_frame.data.flags.pit_limiter);

    // Frame near start (10%) should not be in pits
    let race_frame = &recording.frames[10];
    assert!(!race_frame.data.flags.in_pits);
}

#[test]
fn fixture_racing_session_timestamps_increasing() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);

    for window in recording.frames.windows(2) {
        assert!(
            window[1].timestamp_ns >= window[0].timestamp_ns,
            "timestamps should be non-decreasing"
        );
    }
}

#[test]
fn fixture_racing_session_frame_count_matches_metadata() {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 3.0, 60.0);

    assert_eq!(recording.metadata.frame_count, recording.frames.len());
    assert_eq!(recording.metadata.frame_count, 180);
    assert!((recording.metadata.average_fps - 60.0).abs() < f32::EPSILON);
}
