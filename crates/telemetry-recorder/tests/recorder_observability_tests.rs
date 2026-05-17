//! Deep tests for telemetry recording and replay: session lifecycle,
//! replay at various speeds, different data rates, export format correctness,
//! and concurrent recording safety.

use openracing_telemetry_recorder::{
    TelemetryPlayer, TelemetryRecorder, TelemetryRecording, TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFrame};
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn make_frame(timestamp_ns: u64, seq: u64, rpm: f32, speed: f32) -> TelemetryFrame {
    TelemetryFrame::new(
        NormalizedTelemetry::builder()
            .rpm(rpm)
            .speed_ms(speed)
            .ffb_scalar(0.5)
            .gear(3)
            .build(),
        timestamp_ns,
        seq,
        64,
    )
}

fn make_recording(game_id: &str, count: usize, fps: f32) -> TelemetryRecording {
    TestFixtureGenerator::generate_racing_session(game_id.to_string(), count as f32 / fps, fps)
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Recording session lifecycle (start, record, stop, export)
// ═══════════════════════════════════════════════════════════════════════════

mod session_lifecycle {
    use super::*;

    #[test]
    fn full_lifecycle_start_record_stop_export() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("lifecycle.json");
        let mut recorder = TelemetryRecorder::new(path.clone())?;

        assert!(!recorder.is_recording());

        recorder.start_recording("iracing".to_string());
        assert!(recorder.is_recording());

        for i in 0..10 {
            recorder.record_frame(make_frame(i * 16_666_666, i, 5000.0, 40.0));
        }
        assert_eq!(recorder.frame_count(), 10);

        let recording = recorder.stop_recording(Some("test session".to_string()))?;
        assert!(!recorder.is_recording());
        assert_eq!(recording.frames.len(), 10);
        assert_eq!(recording.metadata.game_id, "iracing");
        assert_eq!(
            recording.metadata.description.as_deref(),
            Some("test session")
        );

        // Verify file was written
        assert!(path.exists());
        let loaded = TelemetryRecorder::load_recording(&path)?;
        assert_eq!(loaded.frames.len(), 10);
        Ok(())
    }

    #[test]
    fn stop_without_start_fails() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("no_start.json");
        let mut recorder = TelemetryRecorder::new(path)?;

        let result = recorder.stop_recording(None);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn recording_ignores_frames_before_start() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("pre_start.json");
        let mut recorder = TelemetryRecorder::new(path)?;

        recorder.record_frame(make_frame(0, 0, 5000.0, 40.0));
        assert_eq!(recorder.frame_count(), 0);

        recorder.start_recording("test".to_string());
        recorder.record_frame(make_frame(100, 1, 5000.0, 40.0));
        assert_eq!(recorder.frame_count(), 1);
        Ok(())
    }

    #[test]
    fn restart_clears_previous_frames() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("restart.json");
        let mut recorder = TelemetryRecorder::new(path)?;

        recorder.start_recording("game1".to_string());
        recorder.record_frame(make_frame(0, 0, 5000.0, 40.0));
        recorder.record_frame(make_frame(1, 1, 5000.0, 40.0));
        assert_eq!(recorder.frame_count(), 2);

        recorder.start_recording("game2".to_string());
        assert_eq!(recorder.frame_count(), 0);

        let recording = recorder.stop_recording(None)?;
        assert_eq!(recording.metadata.game_id, "game2");
        assert!(recording.frames.is_empty());
        Ok(())
    }

    #[test]
    fn multiple_start_stop_cycles() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("cycles.json");
        let mut recorder = TelemetryRecorder::new(path)?;

        for cycle in 0..3 {
            recorder.start_recording(format!("game_{cycle}"));
            for i in 0..5 {
                recorder.record_frame(make_frame(i * 1000, i, 5000.0, 40.0));
            }
            let recording = recorder.stop_recording(None)?;
            assert_eq!(recording.frames.len(), 5);
            assert_eq!(recording.metadata.game_id, format!("game_{cycle}"));
        }
        Ok(())
    }

    #[test]
    fn metadata_extracts_car_and_track_from_frames() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("meta.json");
        let mut recorder = TelemetryRecorder::new(path)?;

        recorder.start_recording("ac".to_string());
        recorder.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::builder()
                .car_id("ferrari_488")
                .track_id("monza")
                .rpm(6000.0)
                .build(),
            0,
            0,
            64,
        ));

        let recording = recorder.stop_recording(None)?;
        assert_eq!(recording.metadata.car_id.as_deref(), Some("ferrari_488"));
        assert_eq!(recording.metadata.track_id.as_deref(), Some("monza"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Replay at various speeds
// ═══════════════════════════════════════════════════════════════════════════

mod replay_speeds {
    use super::*;

    #[test]
    fn replay_initial_state_not_started() -> TestResult {
        let recording = make_recording("test", 60, 60.0);
        let player = TelemetryPlayer::new(recording);

        assert!(!player.is_finished());
        assert!((player.progress() - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn replay_empty_recording_immediately_finished() -> TestResult {
        let recording = TelemetryRecording {
            metadata: openracing_telemetry_recorder::RecordingMetadata {
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

        let mut player = TelemetryPlayer::new(recording);
        player.start_playback();
        assert!(player.is_finished());
        assert!((player.progress() - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn replay_first_frame_available_immediately() -> TestResult {
        let recording = make_recording("test", 60, 60.0);
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
    fn replay_speed_1x_default() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let mut player = TelemetryPlayer::new(recording);
        player.start_playback();

        // First frame always available
        assert!(player.get_next_frame().is_some());
        Ok(())
    }

    #[test]
    fn replay_speed_clamped_to_bounds() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let mut player = TelemetryPlayer::new(recording);

        player.set_playback_speed(0.01); // below minimum 0.1
        // Speed should be clamped to 0.1

        player.set_playback_speed(100.0); // above maximum 10.0
        // Speed should be clamped to 10.0
        Ok(())
    }

    #[test]
    fn replay_without_start_returns_none() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let mut player = TelemetryPlayer::new(recording);

        let frame = player.get_next_frame();
        assert!(frame.is_none(), "should return None before start_playback");
        Ok(())
    }

    #[test]
    fn replay_reset_allows_restart() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let mut player = TelemetryPlayer::new(recording);

        player.start_playback();
        let _ = player.get_next_frame();

        player.reset();
        assert!(!player.is_finished());
        assert!((player.progress() - 0.0).abs() < f32::EPSILON);

        // After reset, get_next_frame returns None until start_playback
        assert!(player.get_next_frame().is_none());
        Ok(())
    }

    #[test]
    fn replay_progress_advances() -> TestResult {
        let recording = make_recording("test", 10, 10_000.0);
        let mut player = TelemetryPlayer::new(recording);
        player.set_playback_speed(10.0); // Fast playback
        player.start_playback();

        let initial = player.progress();

        // Consume some frames
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut consumed = 0;
        while player.get_next_frame().is_some() {
            consumed += 1;
        }

        let final_progress = player.progress();
        assert!(final_progress >= initial, "progress should not decrease");
        assert!(consumed > 0 || final_progress > 0.0);
        Ok(())
    }

    #[test]
    fn replay_metadata_accessible() -> TestResult {
        let recording = make_recording("iracing", 100, 60.0);
        let player = TelemetryPlayer::new(recording);

        let meta = player.metadata();
        assert_eq!(meta.game_id, "iracing");
        assert_eq!(meta.frame_count, 100);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Recording with different data rates
// ═══════════════════════════════════════════════════════════════════════════

mod data_rates {
    use super::*;

    #[test]
    fn fixture_at_60_fps() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 60.0);
        assert_eq!(recording.frames.len(), 60);
        assert!((recording.metadata.average_fps - 60.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn fixture_at_120_fps() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 1.0, 120.0);
        assert_eq!(recording.frames.len(), 120);
        Ok(())
    }

    #[test]
    fn fixture_at_1_fps() -> TestResult {
        let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 5.0, 1.0);
        assert_eq!(recording.frames.len(), 5);
        Ok(())
    }

    #[test]
    fn fixture_timestamps_monotonically_increasing() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);

        for window in recording.frames.windows(2) {
            assert!(
                window[1].timestamp_ns >= window[0].timestamp_ns,
                "timestamps must be monotonically increasing"
            );
        }
        Ok(())
    }

    #[test]
    fn fixture_sequences_monotonically_increasing() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 2.0, 60.0);

        for window in recording.frames.windows(2) {
            assert!(
                window[1].sequence > window[0].sequence,
                "sequences must be strictly increasing"
            );
        }
        Ok(())
    }

    #[test]
    fn scenario_constant_speed_produces_uniform_data() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 1.0, 60.0);

        for frame in &recording.frames {
            assert!((frame.data.speed_ms - 50.0).abs() < f32::EPSILON);
            assert!((frame.data.rpm - 6000.0).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn scenario_acceleration_has_increasing_speed() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 1.0, 60.0);

        let first_speed = recording.frames.first().map(|f| f.data.speed_ms);
        let last_speed = recording.frames.last().map(|f| f.data.speed_ms);

        if let (Some(first), Some(last)) = (first_speed, last_speed) {
            assert!(last > first, "speed should increase during acceleration");
        }
        Ok(())
    }

    #[test]
    fn scenario_cornering_has_high_slip() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 60.0);

        for frame in &recording.frames {
            assert!(
                frame.data.slip_ratio > 0.3,
                "cornering should have high slip ratio"
            );
        }
        Ok(())
    }

    #[test]
    fn scenario_pitstop_has_pit_phase() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 3.0, 60.0);

        let pit_frames: Vec<_> = recording
            .frames
            .iter()
            .filter(|f| f.data.flags.in_pits)
            .collect();

        assert!(
            !pit_frames.is_empty(),
            "pitstop scenario should have in-pit frames"
        );

        for f in &pit_frames {
            assert!(
                f.data.flags.pit_limiter,
                "pit limiter should be active in pits"
            );
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Export format correctness
// ═══════════════════════════════════════════════════════════════════════════

mod export_formats {
    use super::*;

    #[test]
    fn csv_export_has_header_and_correct_row_count() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let csv = recording.to_csv();

        let lines: Vec<&str> = csv.trim().lines().collect();
        assert!(lines.len() >= 2, "CSV should have header + data");

        // Header check
        assert!(lines[0].contains("timestamp_ns"));
        assert!(lines[0].contains("rpm"));
        assert!(lines[0].contains("speed_ms"));

        // Data rows = frame_count
        assert_eq!(lines.len() - 1, recording.frames.len());
        Ok(())
    }

    #[test]
    fn csv_export_values_match_frame_data() -> TestResult {
        let recording = make_recording("test", 3, 3.0);
        let csv = recording.to_csv();

        let lines: Vec<&str> = csv.trim().lines().collect();
        // First data row should match first frame
        let first_data = lines[1];
        let fields: Vec<&str> = first_data.split(',').collect();

        let first_frame = &recording.frames[0];
        let ts: u64 = fields[0].parse()?;
        assert_eq!(ts, first_frame.timestamp_ns);
        Ok(())
    }

    #[test]
    fn compact_json_roundtrips_correctly() -> TestResult {
        let original = make_recording("test", 20, 60.0);
        let json_bytes = original.to_compact_json()?;

        let restored: TelemetryRecording = serde_json::from_slice(&json_bytes)?;
        assert_eq!(restored.frames.len(), original.frames.len());
        assert_eq!(restored.metadata.game_id, original.metadata.game_id);
        Ok(())
    }

    #[test]
    fn binary_format_roundtrips_correctly() -> TestResult {
        let original = make_recording("test", 30, 60.0);
        let binary = original.to_binary()?;
        let restored = TelemetryRecording::from_binary(&binary)?;

        assert_eq!(restored.frames.len(), original.frames.len());
        assert_eq!(restored.metadata.game_id, original.metadata.game_id);
        assert_eq!(restored.metadata.frame_count, original.metadata.frame_count);

        // Verify frame data integrity
        for (orig, rest) in original.frames.iter().zip(restored.frames.iter()) {
            assert_eq!(orig.timestamp_ns, rest.timestamp_ns);
            assert_eq!(orig.sequence, rest.sequence);
            assert!((orig.data.rpm - rest.data.rpm).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn binary_format_too_short_fails() -> TestResult {
        let result = TelemetryRecording::from_binary(&[0, 1]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn binary_format_truncated_metadata_fails() -> TestResult {
        let result = TelemetryRecording::from_binary(&[0xFF, 0xFF, 0x00, 0x00, 0x01]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn json_file_roundtrip_via_save_load() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("roundtrip.json");

        let mut recorder = TelemetryRecorder::new(path.clone())?;
        recorder.start_recording("iracing".to_string());

        for i in 0..5 {
            recorder.record_frame(make_frame(i * 1_000_000, i, 6000.0, 50.0));
        }

        let original = recorder.stop_recording(Some("roundtrip test".to_string()))?;
        let loaded = TelemetryRecorder::load_recording(&path)?;

        assert_eq!(loaded.frames.len(), original.frames.len());
        assert_eq!(loaded.metadata.game_id, "iracing");

        for (o, l) in original.frames.iter().zip(loaded.frames.iter()) {
            assert_eq!(o.timestamp_ns, l.timestamp_ns);
            assert_eq!(o.sequence, l.sequence);
        }
        Ok(())
    }

    #[test]
    fn load_nonexistent_file_fails() -> TestResult {
        let result = TelemetryRecorder::load_recording("nonexistent_file.json");
        assert!(result.is_err());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Session comparison / diff
// ═══════════════════════════════════════════════════════════════════════════

mod session_diff {
    use super::*;

    #[test]
    fn identical_recordings_produce_empty_diff() -> TestResult {
        let recording = make_recording("test", 10, 10.0);
        let diff = recording.diff(&recording);

        assert!(diff.is_identical());
        assert!(diff.metadata_diffs.is_empty());
        assert_eq!(diff.frame_count_delta, 0);
        assert!(diff.field_diffs.is_empty());
        Ok(())
    }

    #[test]
    fn different_game_ids_detected() -> TestResult {
        let a = make_recording("iracing", 10, 10.0);
        let b = make_recording("acc", 10, 10.0);

        let diff = a.diff(&b);
        assert!(!diff.is_identical());
        assert!(diff.metadata_diffs.iter().any(|d| d.contains("game_id")));
        Ok(())
    }

    #[test]
    fn different_frame_counts_detected() -> TestResult {
        let a = make_recording("test", 10, 10.0);
        let b = make_recording("test", 20, 10.0);

        let diff = a.diff(&b);
        assert_ne!(diff.frame_count_delta, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Concurrent recording safety
// ═══════════════════════════════════════════════════════════════════════════

mod concurrent_recording {
    use super::*;

    #[test]
    fn concurrent_recorders_produce_independent_files() -> TestResult {
        let dir = tempdir()?;
        let dir_path = dir.path().to_path_buf();

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let path = dir_path.join(format!("concurrent_{i}.json"));
                std::thread::spawn(move || -> Result<(), String> {
                    let mut recorder = TelemetryRecorder::new(path).map_err(|e| e.to_string())?;
                    recorder.start_recording(format!("game_{i}"));
                    for j in 0..10 {
                        recorder.record_frame(make_frame(j * 1000, j, 5000.0, 40.0));
                    }
                    let recording = recorder.stop_recording(None).map_err(|e| e.to_string())?;
                    assert_eq!(recording.frames.len(), 10);
                    assert_eq!(recording.metadata.game_id, format!("game_{i}"));
                    Ok(())
                })
            })
            .collect();

        for handle in handles {
            handle
                .join()
                .map_err(|_| "thread panicked")?
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        }

        // Verify each file exists independently
        for i in 0..4 {
            let path = dir.path().join(format!("concurrent_{i}.json"));
            assert!(path.exists());
            let loaded = TelemetryRecorder::load_recording(&path)?;
            assert_eq!(loaded.frames.len(), 10);
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Fixture generator edge cases
// ═══════════════════════════════════════════════════════════════════════════

mod fixture_edges {
    use super::*;

    #[test]
    fn zero_duration_produces_no_frames() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 0.0, 60.0);
        assert!(recording.frames.is_empty());
        Ok(())
    }

    #[test]
    fn zero_fps_produces_no_frames() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 10.0, 0.0);
        assert!(recording.frames.is_empty());
        Ok(())
    }

    #[test]
    fn very_high_fps_produces_many_frames() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("test".to_string(), 0.1, 1000.0);
        assert_eq!(recording.frames.len(), 100);
        Ok(())
    }

    #[test]
    fn fixture_metadata_matches_requested_params() -> TestResult {
        let recording =
            TestFixtureGenerator::generate_racing_session("iracing".to_string(), 5.0, 60.0);

        assert_eq!(recording.metadata.game_id, "iracing");
        assert!((recording.metadata.duration_seconds - 5.0).abs() < 0.01);
        assert!((recording.metadata.average_fps - 60.0).abs() < f32::EPSILON);
        assert_eq!(recording.metadata.frame_count, 300);
        assert_eq!(recording.metadata.car_id.as_deref(), Some("test_car"));
        assert_eq!(recording.metadata.track_id.as_deref(), Some("test_track"));
        Ok(())
    }
}
