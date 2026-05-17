//! Deep integration tests for the openracing-telemetry-recorder crate.
//!
//! Exercises recording start/stop, file format roundtrip, recording limits,
//! metadata correctness, concurrent sessions, replay fidelity,
//! and file integrity verification.

use openracing_telemetry_recorder::{
    RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Recording start/stop
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn start_recording_sets_recording_flag() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("start.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    assert!(!recorder.is_recording());
    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());
    Ok(())
}

#[test]
fn stop_recording_clears_recording_flag() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("stop.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("test_game".to_string());
    assert!(recorder.is_recording());
    let _recording = recorder.stop_recording(None)?;
    assert!(!recorder.is_recording());
    Ok(())
}

#[test]
fn stop_without_start_returns_error() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("no_start.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    let result = recorder.stop_recording(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn start_recording_resets_frame_buffer() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("reset.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("game1".to_string());
    let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 1);

    // Second start clears buffer
    recorder.start_recording("game2".to_string());
    assert_eq!(recorder.frame_count(), 0);

    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.metadata.game_id, "game2");
    assert_eq!(recording.frames.len(), 0);
    Ok(())
}

#[test]
fn record_frame_before_start_ignored() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("ignore.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    recorder.record_frame(frame);
    assert_eq!(recorder.frame_count(), 0);
    Ok(())
}

#[test]
fn multiple_start_stop_cycles() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("cycles.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    for i in 0..3 {
        recorder.start_recording(format!("game_{i}"));
        for j in 0..5 {
            let t = NormalizedTelemetry::builder()
                .rpm(1000.0 + j as f32 * 100.0)
                .build();
            let frame = TelemetryFrame::new(t, j as u64 * 16_000_000, j as u64, 64);
            recorder.record_frame(frame);
        }
        let recording = recorder.stop_recording(Some(format!("cycle {i}")))?;
        assert_eq!(recording.frames.len(), 5);
        assert_eq!(recording.metadata.game_id, format!("game_{i}"));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// File format: recorded data can be replayed
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn save_load_roundtrip_preserves_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("roundtrip.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("roundtrip_game".to_string());
    for i in 0..20 {
        let t = NormalizedTelemetry::builder()
            .rpm(2000.0 + i as f32 * 200.0)
            .speed_ms(10.0 + i as f32 * 3.0)
            .gear(((i % 6) + 1) as i8)
            .ffb_scalar((i as f32 / 20.0) * 0.8)
            .build();
        let frame = TelemetryFrame::new(t, i as u64 * 16_000_000, i as u64, 128);
        recorder.record_frame(frame);
    }
    let original = recorder.stop_recording(Some("Roundtrip deep test".to_string()))?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), original.frames.len());
    assert_eq!(loaded.metadata.game_id, "roundtrip_game");
    assert_eq!(loaded.metadata.frame_count, 20);

    for (orig, load) in original.frames.iter().zip(loaded.frames.iter()) {
        assert!((orig.data.rpm - load.data.rpm).abs() < 0.01);
        assert!((orig.data.speed_ms - load.data.speed_ms).abs() < 0.01);
        assert!((orig.data.ffb_scalar - load.data.ffb_scalar).abs() < 0.01);
        assert_eq!(orig.data.gear, load.data.gear);
        assert_eq!(orig.timestamp_ns, load.timestamp_ns);
        assert_eq!(orig.sequence, load.sequence);
        assert_eq!(orig.raw_size, load.raw_size);
    }
    Ok(())
}

#[test]
fn save_load_preserves_flags() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("flags.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("flags_test".to_string());
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        in_pits: true,
        pit_limiter: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };
    let t = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .flags(flags)
        .build();
    let frame = TelemetryFrame::new(t, 0, 0, 64);
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 1);
    let f = &loaded.frames[0].data.flags;
    assert!(f.yellow_flag);
    assert!(f.blue_flag);
    assert!(f.in_pits);
    assert!(f.pit_limiter);
    assert!(f.abs_active);
    assert!(!f.red_flag);
    assert!(!f.checkered_flag);
    Ok(())
}

#[test]
fn save_load_preserves_car_and_track_ids() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("ids.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("id_test".to_string());
    let t = NormalizedTelemetry::builder()
        .rpm(4000.0)
        .car_id("mclaren_p1")
        .track_id("laguna_seca")
        .build();
    let frame = TelemetryFrame::new(t, 0, 0, 64);
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert!(loaded.metadata.car_id.as_deref() == Some("mclaren_p1"));
    assert!(loaded.metadata.track_id.as_deref() == Some("laguna_seca"));
    Ok(())
}

#[test]
fn load_nonexistent_file_returns_error() -> TestResult {
    let result = TelemetryRecorder::load_recording("nonexistent_path_xyz.json");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn serialization_roundtrip_via_serde() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("serde_deep".to_string(), 1.0, 30.0);
    let json = serde_json::to_string(&recording)?;
    let deserialized: TelemetryRecording = serde_json::from_str(&json)?;

    assert_eq!(deserialized.frames.len(), recording.frames.len());
    assert_eq!(deserialized.metadata.game_id, "serde_deep");
    assert_eq!(deserialized.metadata.frame_count, 30);

    for (orig, deser) in recording.frames.iter().zip(deserialized.frames.iter()) {
        assert_eq!(orig.timestamp_ns, deser.timestamp_ns);
        assert_eq!(orig.sequence, deser.sequence);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording limits: max file size, max duration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_large_frame_count() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("large.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("large_test".to_string());
    for i in 0..500 {
        let t = NormalizedTelemetry::builder()
            .rpm(3000.0 + (i % 100) as f32 * 50.0)
            .speed_ms(20.0)
            .build();
        let frame = TelemetryFrame::new(t, i as u64 * 1_000_000, i as u64, 64);
        recorder.record_frame(frame);
    }
    assert_eq!(recorder.frame_count(), 500);

    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 500);
    assert_eq!(recording.metadata.frame_count, 500);

    // Verify file was written and can be loaded
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 500);
    Ok(())
}

#[test]
fn recording_zero_frames_produces_valid_file() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("empty.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("empty_test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 0);
    assert_eq!(recording.metadata.frame_count, 0);

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 0);
    Ok(())
}

#[test]
fn recording_single_frame() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("single.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("single_test".to_string());
    let t = NormalizedTelemetry::builder().rpm(7000.0).build();
    let frame = TelemetryFrame::new(t, 42_000_000, 0, 32);
    recorder.record_frame(frame);
    let recording = recorder.stop_recording(None)?;

    assert_eq!(recording.frames.len(), 1);
    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), 1);
    assert!((loaded.frames[0].data.rpm - 7000.0).abs() < 0.01);
    Ok(())
}

#[test]
fn fixture_zero_duration_no_frames() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("zero_dur".to_string(), 0.0, 60.0);
    assert_eq!(recording.frames.len(), 0);
    assert_eq!(recording.metadata.frame_count, 0);
    Ok(())
}

#[test]
fn fixture_zero_fps_no_frames() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("zero_fps".to_string(), 5.0, 0.0);
    assert_eq!(recording.frames.len(), 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording metadata: timestamps, game info, session info
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn metadata_game_id_preserved() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_game.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("iracing".to_string());
    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.metadata.game_id, "iracing");
    Ok(())
}

#[test]
fn metadata_timestamp_is_reasonable() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_time.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("time_test".to_string());
    let recording = recorder.stop_recording(None)?;
    // Timestamp should be a recent Unix timestamp (after 2020-01-01)
    assert!(
        recording.metadata.timestamp > 1_577_836_800,
        "timestamp should be after 2020"
    );
    Ok(())
}

#[test]
fn metadata_duration_nonnegative() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_dur.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("dur_test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert!(
        recording.metadata.duration_seconds >= 0.0,
        "duration must be non-negative"
    );
    Ok(())
}

#[test]
fn metadata_frame_count_matches_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_count.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("count_test".to_string());
    for i in 0..7 {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let recording = recorder.stop_recording(None)?;
    assert_eq!(recording.metadata.frame_count, recording.frames.len());
    assert_eq!(recording.metadata.frame_count, 7);
    Ok(())
}

#[test]
fn metadata_description_preserved() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_desc.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("desc_test".to_string());
    let recording = recorder.stop_recording(Some("A detailed description".to_string()))?;
    assert!(recording.metadata.description.as_deref() == Some("A detailed description"));
    Ok(())
}

#[test]
fn metadata_description_none_when_not_provided() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_nodesc.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("nodesc_test".to_string());
    let recording = recorder.stop_recording(None)?;
    assert!(recording.metadata.description.is_none());
    Ok(())
}

#[test]
fn metadata_car_track_from_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("meta_car_track.json");
    let mut recorder = TelemetryRecorder::new(path)?;

    recorder.start_recording("car_track_test".to_string());
    // First frame has no car/track
    let frame1 = TelemetryFrame::new(NormalizedTelemetry::default(), 0, 0, 64);
    recorder.record_frame(frame1);
    // Second frame has car/track
    let t = NormalizedTelemetry::builder()
        .car_id("bmw_m4_gt3")
        .track_id("monza")
        .build();
    let frame2 = TelemetryFrame::new(t, 1_000_000, 1, 64);
    recorder.record_frame(frame2);

    let recording = recorder.stop_recording(None)?;
    // Metadata should pick up car/track from frames
    assert!(recording.metadata.car_id.as_deref() == Some("bmw_m4_gt3"));
    assert!(recording.metadata.track_id.as_deref() == Some("monza"));
    Ok(())
}

#[test]
fn metadata_serialization_roundtrip() -> TestResult {
    let metadata = RecordingMetadata {
        game_id: "deep_test".to_string(),
        timestamp: 1_700_000_000,
        duration_seconds: 42.5,
        frame_count: 100,
        average_fps: 60.0,
        car_id: Some("porsche_911".to_string()),
        track_id: Some("spa".to_string()),
        description: Some("Deep test metadata".to_string()),
    };
    let json = serde_json::to_string(&metadata)?;
    let loaded: RecordingMetadata = serde_json::from_str(&json)?;

    assert_eq!(loaded.game_id, "deep_test");
    assert_eq!(loaded.timestamp, 1_700_000_000);
    assert!((loaded.duration_seconds - 42.5).abs() < 0.01);
    assert_eq!(loaded.frame_count, 100);
    assert!((loaded.average_fps - 60.0).abs() < 0.01);
    assert!(loaded.car_id.as_deref() == Some("porsche_911"));
    assert!(loaded.track_id.as_deref() == Some("spa"));
    assert!(loaded.description.as_deref() == Some("Deep test metadata"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Concurrent recording (multiple sessions via separate recorders)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn concurrent_recorders_independent_files() -> TestResult {
    let dir = tempdir()?;
    let path1 = dir.path().join("concurrent_1.json");
    let path2 = dir.path().join("concurrent_2.json");

    let mut recorder1 = TelemetryRecorder::new(path1.clone())?;
    let mut recorder2 = TelemetryRecorder::new(path2.clone())?;

    recorder1.start_recording("game_a".to_string());
    recorder2.start_recording("game_b".to_string());

    for i in 0..10 {
        let t1 = NormalizedTelemetry::builder()
            .rpm(3000.0 + i as f32 * 100.0)
            .build();
        let t2 = NormalizedTelemetry::builder()
            .rpm(5000.0 + i as f32 * 50.0)
            .build();
        recorder1.record_frame(TelemetryFrame::new(t1, i * 16_000_000, i, 64));
        recorder2.record_frame(TelemetryFrame::new(t2, i * 16_000_000, i, 64));
    }

    let rec1 = recorder1.stop_recording(Some("Session A".to_string()))?;
    let rec2 = recorder2.stop_recording(Some("Session B".to_string()))?;

    assert_eq!(rec1.metadata.game_id, "game_a");
    assert_eq!(rec2.metadata.game_id, "game_b");
    assert_eq!(rec1.frames.len(), 10);
    assert_eq!(rec2.frames.len(), 10);

    // Verify loaded files are independent
    let loaded1 = TelemetryRecorder::load_recording(&path1)?;
    let loaded2 = TelemetryRecorder::load_recording(&path2)?;
    assert_eq!(loaded1.metadata.game_id, "game_a");
    assert_eq!(loaded2.metadata.game_id, "game_b");
    // RPM values should differ between sessions
    assert!(
        (loaded1.frames[0].data.rpm - loaded2.frames[0].data.rpm).abs() > 1.0,
        "concurrent sessions should have different data"
    );
    Ok(())
}

#[test]
fn concurrent_recorders_in_threads() -> TestResult {
    let dir = tempdir()?;
    let dir_path = dir.path().to_path_buf();

    std::thread::scope(|s| {
        let handles: Vec<_> = (0..4)
            .map(|idx| {
                let p = dir_path.join(format!("thread_{idx}.json"));
                s.spawn(move || -> anyhow::Result<()> {
                    let mut recorder = TelemetryRecorder::new(p.clone())?;
                    recorder.start_recording(format!("thread_game_{idx}"));
                    for j in 0..10 {
                        let t = NormalizedTelemetry::builder()
                            .rpm(1000.0 * (idx as f32 + 1.0) + j as f32 * 10.0)
                            .build();
                        let frame = TelemetryFrame::new(t, j as u64 * 16_000_000, j as u64, 64);
                        recorder.record_frame(frame);
                    }
                    let rec = recorder.stop_recording(None)?;
                    assert_eq!(rec.frames.len(), 10);
                    assert_eq!(rec.metadata.game_id, format!("thread_game_{idx}"));

                    let loaded = TelemetryRecorder::load_recording(&p)?;
                    assert_eq!(loaded.frames.len(), 10);
                    Ok(())
                })
            })
            .collect();

        for h in handles {
            if let Err(e) = h.join() {
                std::panic::resume_unwind(e);
            }
        }
    });

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Replay: recorded data matches original
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn replay_player_initial_state() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("replay_test".to_string(), 1.0, 10.0);
    let player = TelemetryPlayer::new(recording);
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
    Ok(())
}

#[test]
fn replay_player_empty_recording_finished() -> TestResult {
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
fn replay_first_frame_available_immediately() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("replay_imm".to_string(), 1.0, 60.0);
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    let frame = player.get_next_frame();
    assert!(
        frame.is_some(),
        "first frame should be available immediately"
    );
    Ok(())
}

#[test]
fn replay_without_start_returns_none() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("no_start".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    let frame = player.get_next_frame();
    assert!(frame.is_none());
    Ok(())
}

#[test]
fn replay_reset_restores_initial_state() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("reset_test".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);

    player.start_playback();
    let _ = player.get_next_frame();
    assert!(player.progress() > 0.0);

    player.reset();
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
    Ok(())
}

#[test]
fn replay_speed_clamp_lower_bound() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("speed_lo".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(0.001);
    // Should not panic; speed clamped to 0.1
    Ok(())
}

#[test]
fn replay_speed_clamp_upper_bound() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("speed_hi".to_string(), 1.0, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(999.0);
    // Should not panic; speed clamped to 10.0
    Ok(())
}

#[test]
fn replay_metadata_accessible() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("meta_replay".to_string(), 2.0, 30.0);
    let player = TelemetryPlayer::new(recording);
    let meta = player.metadata();
    assert_eq!(meta.game_id, "meta_replay");
    assert_eq!(meta.frame_count, 60);
    assert!((meta.average_fps - 30.0).abs() < 0.01);
    assert!(meta.car_id.is_some());
    assert!(meta.track_id.is_some());
    Ok(())
}

#[test]
fn replay_saved_then_loaded_recording() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("replay_save_load.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("replay_sl".to_string());
    for i in 0..15 {
        let t = NormalizedTelemetry::builder()
            .rpm(3000.0 + i as f32 * 200.0)
            .speed_ms(20.0 + i as f32 * 2.0)
            .build();
        let frame = TelemetryFrame::new(t, i as u64 * 16_000_000, i as u64, 64);
        recorder.record_frame(frame);
    }
    let _original = recorder.stop_recording(None)?;

    // Load and replay
    let loaded = TelemetryRecorder::load_recording(&path)?;
    let mut player = TelemetryPlayer::new(loaded);
    player.start_playback();

    let first = player.get_next_frame();
    assert!(first.is_some());
    if let Some(f) = first {
        assert!((f.data.rpm - 3000.0).abs() < 0.01);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// File integrity: verify checksums / consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn file_content_is_valid_json() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("valid_json.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("json_test".to_string());
    for i in 0..5 {
        let t = NormalizedTelemetry::builder()
            .rpm(4000.0 + i as f32 * 100.0)
            .build();
        let frame = TelemetryFrame::new(t, i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    // Read raw file and verify it's valid JSON
    let raw = std::fs::read_to_string(&path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    assert!(parsed.is_object());
    assert!(parsed.get("metadata").is_some());
    assert!(parsed.get("frames").is_some());
    Ok(())
}

#[test]
fn file_metadata_matches_frame_array_length() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("integrity_count.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("integrity_test".to_string());
    for i in 0..12 {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    let raw = std::fs::read_to_string(&path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    let meta_count = parsed["metadata"]["frame_count"]
        .as_u64()
        .ok_or_else(|| std::io::Error::other("frame_count not found in metadata"))?;
    let frames_len = parsed["frames"]
        .as_array()
        .ok_or_else(|| std::io::Error::other("frames not an array"))?
        .len() as u64;
    assert_eq!(meta_count, frames_len);
    assert_eq!(meta_count, 12);
    Ok(())
}

#[test]
fn file_timestamps_monotonic_on_disk() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("monotonic.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("mono_test".to_string());
    for i in 0..10 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::default(),
            i as u64 * 16_000_000,
            i as u64,
            64,
        );
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    for pair in loaded.frames.windows(2) {
        assert!(
            pair[1].timestamp_ns >= pair[0].timestamp_ns,
            "timestamps should be monotonically increasing on disk"
        );
    }
    Ok(())
}

#[test]
fn file_sequences_monotonic_on_disk() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("seq_mono.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("seq_test".to_string());
    for i in 0..10u64 {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), i * 16_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    for pair in loaded.frames.windows(2) {
        assert!(
            pair[1].sequence > pair[0].sequence,
            "sequences should be strictly increasing on disk"
        );
    }
    Ok(())
}

#[test]
fn file_overwrite_on_second_recording() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("overwrite.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    // First recording: 3 frames
    recorder.start_recording("first".to_string());
    for i in 0..3 {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let _rec1 = recorder.stop_recording(None)?;

    // Second recording: 7 frames — should overwrite file
    recorder.start_recording("second".to_string());
    for i in 0..7 {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), i * 1_000_000, i, 64);
        recorder.record_frame(frame);
    }
    let _rec2 = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.metadata.game_id, "second");
    assert_eq!(loaded.frames.len(), 7);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Test scenarios: fixture generator deep tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_constant_speed_uniform_values() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 1.0, 20.0);
    assert_eq!(recording.frames.len(), 20);
    for frame in &recording.frames {
        assert!((frame.data.speed_ms - 50.0).abs() < 0.01);
        assert!((frame.data.rpm - 6000.0).abs() < 0.01);
        assert_eq!(frame.data.gear, 4);
    }
    Ok(())
}

#[test]
fn scenario_acceleration_speed_ramp() -> TestResult {
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
        "speed should increase: first={first_speed}, last={last_speed}"
    );
    Ok(())
}

#[test]
fn scenario_cornering_high_ffb_and_slip() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 20.0);
    for frame in &recording.frames {
        assert!(
            (frame.data.ffb_scalar - 0.9).abs() < 0.01,
            "cornering should have high FFB"
        );
        assert!(
            (frame.data.slip_ratio - 0.4).abs() < 0.01,
            "cornering should have elevated slip"
        );
    }
    Ok(())
}

#[test]
fn scenario_pitstop_has_pit_phase() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);
    let in_pit = recording
        .frames
        .iter()
        .filter(|f| f.data.flags.in_pits)
        .count();
    let out_pit = recording
        .frames
        .iter()
        .filter(|f| !f.data.flags.in_pits)
        .count();
    assert!(in_pit > 0, "should have some frames in pits");
    assert!(out_pit > 0, "should have some frames outside pits");
    Ok(())
}

#[test]
fn scenario_pitstop_limiter_matches_pit_flag() -> TestResult {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 30.0);
    for frame in &recording.frames {
        assert_eq!(frame.data.flags.pit_limiter, frame.data.flags.in_pits);
    }
    Ok(())
}

#[test]
fn fixture_timestamps_monotonically_increasing() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("mono_fix".to_string(), 2.0, 60.0);
    for pair in recording.frames.windows(2) {
        assert!(pair[1].timestamp_ns >= pair[0].timestamp_ns);
    }
    Ok(())
}

#[test]
fn fixture_sequences_strictly_increasing() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("seq_fix".to_string(), 2.0, 60.0);
    for pair in recording.frames.windows(2) {
        assert!(pair[1].sequence > pair[0].sequence);
    }
    Ok(())
}

#[test]
fn fixture_frames_have_positive_rpm_and_speed() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("positive".to_string(), 2.0, 60.0);
    for frame in &recording.frames {
        assert!(frame.data.rpm > 0.0, "rpm should be positive");
        assert!(frame.data.speed_ms > 0.0, "speed should be positive");
    }
    Ok(())
}

#[test]
fn fixture_has_car_and_track_metadata() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("meta_fix".to_string(), 1.0, 10.0);
    assert!(recording.metadata.car_id.is_some());
    assert!(recording.metadata.track_id.is_some());
    assert!(recording.metadata.description.is_some());
    Ok(())
}
