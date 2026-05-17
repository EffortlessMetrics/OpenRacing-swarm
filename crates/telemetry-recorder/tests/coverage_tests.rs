//! Additional coverage tests for openracing-telemetry-recorder.
//!
//! Targets edge cases in recording, playback, fixture generation,
//! and serialization not covered by unit tests or comprehensive.rs.

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryPlayer — advanced playback
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn player_with_single_frame_finishes_after_one_get() -> TestResult {
    let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    let recording = TelemetryRecording {
        metadata: RecordingMetadata {
            game_id: "single".to_string(),
            timestamp: 0,
            duration_seconds: 0.0,
            frame_count: 1,
            average_fps: 1.0,
            car_id: None,
            track_id: None,
            description: None,
        },
        frames: vec![frame],
    };
    let mut player = TelemetryPlayer::new(recording);
    assert!(!player.is_finished());
    player.start_playback();
    // First frame at timestamp 0 should be immediately available.
    let got = player.get_next_frame();
    assert!(got.is_some());
    assert!(player.is_finished());
    Ok(())
}

#[test]
fn player_progress_advances_through_playback() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("progress_test".to_string(), 0.1, 10.0);
    let total = recording.frames.len();
    assert!(total > 0);

    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    player.set_playback_speed(10.0); // speed up to drain quickly

    // Drain all available frames with a busy loop (bounded).
    let mut consumed = 0;
    for _ in 0..total + 10 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        while let Some(_frame) = player.get_next_frame() {
            consumed += 1;
        }
        if player.is_finished() {
            break;
        }
    }
    assert!(consumed > 0);
    assert!(player.is_finished());
    assert!((player.progress() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn player_metadata_matches_recording() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("meta_check".to_string(), 2.0, 30.0);
    let expected_frames = recording.frames.len();
    let player = TelemetryPlayer::new(recording);
    let meta = player.metadata();
    assert_eq!(meta.game_id, "meta_check");
    assert_eq!(meta.frame_count, expected_frames);
    Ok(())
}

#[test]
fn player_set_playback_speed_clamps_low() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("speed_low".to_string(), 0.1, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(0.001); // below 0.1 min
    // Just verify no panic; the internal speed is clamped.
    player.start_playback();
    Ok(())
}

#[test]
fn player_set_playback_speed_clamps_high() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("speed_high".to_string(), 0.1, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(999.0); // above 10.0 max
    player.start_playback();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fixture generation — scenarios
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_scenarios_produce_nonempty_recordings() -> TestResult {
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];
    for scenario in scenarios {
        let recording = TestFixtureGenerator::generate_test_scenario(scenario, 1.0, 10.0);
        assert!(
            !recording.frames.is_empty(),
            "{scenario:?} should produce frames"
        );
        assert!(
            recording.metadata.description.is_some(),
            "{scenario:?} should have description"
        );
    }
    Ok(())
}

#[test]
fn fixture_with_very_high_fps() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("high_fps".to_string(), 0.1, 1000.0);
    assert_eq!(recording.frames.len(), 100);
    assert_eq!(recording.metadata.frame_count, 100);
    Ok(())
}

#[test]
fn fixture_with_fractional_duration() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("frac".to_string(), 0.5, 20.0);
    assert_eq!(recording.frames.len(), 10);
    Ok(())
}

#[test]
fn fixture_pitstop_speed_is_low_in_pits() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);
    let in_pit_frames: Vec<_> = recording
        .frames
        .iter()
        .filter(|f| f.data.flags.in_pits)
        .collect();
    assert!(!in_pit_frames.is_empty());
    for frame in &in_pit_frames {
        assert!(
            frame.data.speed_ms <= 20.0,
            "speed in pits should be low, got {}",
            frame.data.speed_ms
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recorder — persistence edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn save_and_load_empty_recording() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("empty.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("empty_test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert!(recording.frames.is_empty());

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert!(loaded.frames.is_empty());
    assert_eq!(loaded.metadata.game_id, "empty_test");
    assert_eq!(loaded.metadata.frame_count, 0);
    Ok(())
}

#[test]
fn recording_preserves_frame_sequence_numbers() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("seq.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("seq_test".to_string());
    for i in 0..5u64 {
        let t = NormalizedTelemetry::builder().rpm(3000.0).build();
        let frame = TelemetryFrame::new(t, i * 16_000_000, i * 10, 128);
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    for (i, frame) in loaded.frames.iter().enumerate() {
        assert_eq!(frame.sequence, i as u64 * 10);
    }
    Ok(())
}

#[test]
fn recording_preserves_raw_size() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("rawsize.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("rawsize_test".to_string());
    let t = NormalizedTelemetry::builder().rpm(5000.0).build();
    let frame = TelemetryFrame::new(t, 0, 0, 256);
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames[0].raw_size, 256);
    Ok(())
}

#[test]
fn recording_with_flags_survives_roundtrip() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("flags_rt.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("flags_roundtrip".to_string());
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        drs_active: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };
    let t = NormalizedTelemetry::builder()
        .rpm(4000.0)
        .flags(flags)
        .build();
    let frame = TelemetryFrame::new(t, 0, 0, 64);
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert!(loaded.frames[0].data.flags.yellow_flag);
    assert!(loaded.frames[0].data.flags.blue_flag);
    assert!(loaded.frames[0].data.flags.drs_active);
    assert!(loaded.frames[0].data.flags.abs_active);
    assert!(!loaded.frames[0].data.flags.red_flag);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Metadata edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_metadata_serde_with_none_fields() -> TestResult {
    let metadata = RecordingMetadata {
        game_id: "none_test".to_string(),
        timestamp: 0,
        duration_seconds: 0.0,
        frame_count: 0,
        average_fps: 0.0,
        car_id: None,
        track_id: None,
        description: None,
    };
    let json = serde_json::to_string(&metadata)?;
    let loaded: RecordingMetadata = serde_json::from_str(&json)?;
    assert_eq!(loaded.game_id, "none_test");
    assert!(loaded.car_id.is_none());
    assert!(loaded.track_id.is_none());
    assert!(loaded.description.is_none());
    Ok(())
}

#[test]
fn fixture_racing_session_metadata_has_description() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("desc_test".to_string(), 1.0, 10.0);
    assert!(recording.metadata.description.is_some());
    Ok(())
}
