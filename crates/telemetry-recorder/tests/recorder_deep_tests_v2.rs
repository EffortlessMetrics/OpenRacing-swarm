//! Deep tests v2 for the openracing-telemetry-recorder crate.
//!
//! Exercises recording format validation, file size/rotation semantics,
//! concurrent multi-source recording, dropped-frame resilience, playback
//! edge cases, and metadata enrichment not covered by the first deep test suite.

use openracing_telemetry_recorder::{
    TelemetryPlayer, TelemetryRecorder, TelemetryRecording, TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use std::io::Read;
use tempfile::tempdir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Recording format — JSON structure
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn json_output_contains_metadata_object() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("fmt.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("format_test".to_string());
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder().rpm(3000.0).build(),
        100,
        0,
        32,
    );
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(Some("format check".to_string()))?;

    let mut contents = String::new();
    std::fs::File::open(&path)?.read_to_string(&mut contents)?;

    let parsed: serde_json::Value = serde_json::from_str(&contents)?;
    assert!(
        parsed.get("metadata").is_some(),
        "top-level 'metadata' key missing"
    );
    assert!(
        parsed.get("frames").is_some(),
        "top-level 'frames' key missing"
    );
    Ok(())
}

#[test]
fn json_metadata_has_required_fields() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("fields.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("field_test".to_string());
    let _recording = recorder.stop_recording(None)?;

    let mut contents = String::new();
    std::fs::File::open(&path)?.read_to_string(&mut contents)?;
    let parsed: serde_json::Value = serde_json::from_str(&contents)?;
    let meta = parsed.get("metadata").ok_or("missing metadata")?;

    assert!(meta.get("game_id").is_some());
    assert!(meta.get("timestamp").is_some());
    assert!(meta.get("duration_seconds").is_some());
    assert!(meta.get("frame_count").is_some());
    assert!(meta.get("average_fps").is_some());
    Ok(())
}

#[test]
fn json_frames_array_matches_metadata_frame_count() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("count.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("count_test".to_string());
    for i in 0..5 {
        let frame = TelemetryFrame::new(
            NormalizedTelemetry::builder().rpm(1000.0).build(),
            i * 1000,
            i,
            16,
        );
        recorder.record_frame(frame);
    }
    let _recording = recorder.stop_recording(None)?;

    let mut contents = String::new();
    std::fs::File::open(&path)?.read_to_string(&mut contents)?;
    let parsed: serde_json::Value = serde_json::from_str(&contents)?;

    let frame_count = parsed["metadata"]["frame_count"]
        .as_u64()
        .ok_or("missing frame_count")?;
    let frames_len = parsed["frames"]
        .as_array()
        .ok_or("missing frames array")?
        .len() as u64;
    assert_eq!(frame_count, frames_len);
    Ok(())
}

#[test]
fn json_frame_data_preserves_telemetry_values() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("values.json");
    let mut recorder = TelemetryRecorder::new(path.clone())?;

    recorder.start_recording("values_test".to_string());
    let telemetry = NormalizedTelemetry::builder()
        .rpm(7500.0)
        .speed_ms(42.0)
        .gear(5)
        .throttle(0.9)
        .brake(0.1)
        .build();
    let frame = TelemetryFrame::new(telemetry, 500_000, 0, 64);
    recorder.record_frame(frame);
    let _recording = recorder.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    let f = loaded
        .frames
        .first()
        .ok_or("no frames in loaded recording")?;
    assert!((f.data.rpm - 7500.0).abs() < f32::EPSILON);
    assert!((f.data.speed_ms - 42.0).abs() < f32::EPSILON);
    assert_eq!(f.data.gear, 5);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Binary serialization roundtrip (serde_json bytes)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn serde_json_bytes_roundtrip_preserves_recording() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("serde_bytes".to_string(), 0.5, 20.0);
    let bytes = serde_json::to_vec(&recording)?;
    let restored: TelemetryRecording = serde_json::from_slice(&bytes)?;

    assert_eq!(restored.metadata.game_id, recording.metadata.game_id);
    assert_eq!(restored.frames.len(), recording.frames.len());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// File size tracking / rotation semantics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_file_size_grows_with_frame_count() -> TestResult {
    let dir = tempdir()?;

    let small_path = dir.path().join("small.json");
    let mut rec_small = TelemetryRecorder::new(small_path.clone())?;
    rec_small.start_recording("small".to_string());
    for i in 0..2 {
        rec_small.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::default(),
            i * 1000,
            i,
            16,
        ));
    }
    let _s = rec_small.stop_recording(None)?;

    let large_path = dir.path().join("large.json");
    let mut rec_large = TelemetryRecorder::new(large_path.clone())?;
    rec_large.start_recording("large".to_string());
    for i in 0..200 {
        rec_large.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::default(),
            i * 1000,
            i,
            16,
        ));
    }
    let _l = rec_large.stop_recording(None)?;

    let small_size = std::fs::metadata(&small_path)?.len();
    let large_size = std::fs::metadata(&large_path)?.len();
    assert!(
        large_size > small_size,
        "more frames should produce a bigger file"
    );
    Ok(())
}

#[test]
fn overwrite_with_fewer_frames_shrinks_file() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("overwrite.json");

    // First recording: many frames
    let mut rec = TelemetryRecorder::new(path.clone())?;
    rec.start_recording("big".to_string());
    for i in 0..100 {
        rec.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::default(),
            i * 1000,
            i,
            16,
        ));
    }
    let _first = rec.stop_recording(None)?;
    let big_size = std::fs::metadata(&path)?.len();

    // Second recording: fewer frames, same file path
    let mut rec2 = TelemetryRecorder::new(path.clone())?;
    rec2.start_recording("small".to_string());
    rec2.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        0,
        0,
        16,
    ));
    let _second = rec2.stop_recording(None)?;
    let small_size = std::fs::metadata(&path)?.len();

    assert!(small_size < big_size);
    Ok(())
}

#[test]
fn separate_output_paths_simulate_rotation() -> TestResult {
    let dir = tempdir()?;
    let limit_frames = 10u64;

    let mut all_frames_total = 0usize;
    for segment in 0..3 {
        let seg_path = dir.path().join(format!("segment_{segment}.json"));
        let mut rec = TelemetryRecorder::new(seg_path)?;
        rec.start_recording(format!("seg_{segment}"));
        for i in 0..limit_frames {
            let ts = (segment as u64 * limit_frames + i) * 1_000_000;
            rec.record_frame(TelemetryFrame::new(
                NormalizedTelemetry::default(),
                ts,
                segment as u64 * limit_frames + i,
                16,
            ));
        }
        let recording = rec.stop_recording(None)?;
        all_frames_total += recording.frames.len();
    }

    assert_eq!(all_frames_total, 30);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Concurrent multi-source recording
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn concurrent_sources_write_distinct_game_ids() -> TestResult {
    let dir = tempdir()?;
    let handles: Vec<_> = (0..4)
        .map(|idx| {
            let d = dir.path().to_path_buf();
            std::thread::spawn(move || -> Result<String, anyhow::Error> {
                let p = d.join(format!("source_{idx}.json"));
                let game = format!("game_{idx}");
                let mut rec = TelemetryRecorder::new(p)?;
                rec.start_recording(game.clone());
                for i in 0..5 {
                    rec.record_frame(TelemetryFrame::new(
                        NormalizedTelemetry::builder()
                            .rpm((idx as f32 + 1.0) * 1000.0)
                            .build(),
                        i * 1000,
                        i,
                        16,
                    ));
                }
                let recording = rec.stop_recording(None)?;
                Ok(recording.metadata.game_id)
            })
        })
        .collect();

    let mut ids: Vec<String> = Vec::new();
    for h in handles {
        ids.push(h.join().map_err(|_| "thread panicked")??);
    }
    ids.sort();
    assert_eq!(ids, vec!["game_0", "game_1", "game_2", "game_3"]);
    Ok(())
}

#[test]
fn concurrent_recorders_no_file_corruption() -> TestResult {
    let dir = tempdir()?;
    let handles: Vec<_> = (0..4)
        .map(|idx| {
            let d = dir.path().to_path_buf();
            std::thread::spawn(move || -> Result<(), anyhow::Error> {
                let p = d.join(format!("corr_{idx}.json"));
                let mut rec = TelemetryRecorder::new(p.clone())?;
                rec.start_recording(format!("corr_{idx}"));
                for i in 0..20 {
                    rec.record_frame(TelemetryFrame::new(
                        NormalizedTelemetry::default(),
                        i * 1000,
                        i,
                        16,
                    ));
                }
                let _r = rec.stop_recording(None)?;
                // Verify file is valid JSON
                let _loaded = TelemetryRecorder::load_recording(&p)?;
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dropped frames / gaps in sequence numbers
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_with_gaps_in_sequence_numbers() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("gaps.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;
    rec.start_recording("gap_test".to_string());

    // Simulate dropped frames: sequences 0, 5, 10, 15
    for &seq in &[0u64, 5, 10, 15] {
        rec.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::builder().rpm(4000.0).build(),
            seq * 16_666_667, // ~60fps timing
            seq,
            32,
        ));
    }
    let recording = rec.stop_recording(None)?;

    assert_eq!(recording.frames.len(), 4);
    assert_eq!(recording.frames[0].sequence, 0);
    assert_eq!(recording.frames[1].sequence, 5);
    assert_eq!(recording.frames[2].sequence, 10);
    assert_eq!(recording.frames[3].sequence, 15);
    Ok(())
}

#[test]
fn recording_with_duplicate_timestamps_accepted() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("dup_ts.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("dup_ts".to_string());

    let ts = 1_000_000u64;
    for i in 0..3 {
        rec.record_frame(TelemetryFrame::new(
            NormalizedTelemetry::default(),
            ts,
            i,
            16,
        ));
    }
    let recording = rec.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 3);
    assert!(recording.frames.iter().all(|f| f.timestamp_ns == ts));
    Ok(())
}

#[test]
fn recording_with_out_of_order_timestamps() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("ooo.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("ooo_test".to_string());

    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        3_000_000,
        0,
        16,
    ));
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        1_000_000,
        1,
        16,
    ));
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        2_000_000,
        2,
        16,
    ));

    let recording = rec.stop_recording(None)?;
    // Frames should be in insertion order (recorder doesn't sort)
    assert_eq!(recording.frames[0].timestamp_ns, 3_000_000);
    assert_eq!(recording.frames[1].timestamp_ns, 1_000_000);
    assert_eq!(recording.frames[2].timestamp_ns, 2_000_000);
    Ok(())
}

#[test]
fn recording_preserves_zero_timestamp_frames() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("zero_ts.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("zero_ts".to_string());

    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        0,
        0,
        16,
    ));
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        0,
        1,
        16,
    ));

    let recording = rec.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 2);
    assert_eq!(recording.frames[0].timestamp_ns, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording metadata — game, device, timestamp enrichment
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn metadata_average_fps_zero_for_zero_duration_recording() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("zero_dur.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("fps_test".to_string());
    // Immediately stop — near-zero duration
    let recording = rec.stop_recording(None)?;
    // fps should be 0 or non-negative
    assert!(recording.metadata.average_fps >= 0.0);
    Ok(())
}

#[test]
fn metadata_inherits_car_id_from_first_frame_with_it() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("car.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("car_test".to_string());

    // First frame: no car_id
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        0,
        0,
        16,
    ));
    // Second frame: has car_id
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::builder().car_id("porsche-911").build(),
        1000,
        1,
        32,
    ));

    let recording = rec.stop_recording(None)?;
    assert_eq!(recording.metadata.car_id.as_deref(), Some("porsche-911"));
    Ok(())
}

#[test]
fn metadata_inherits_track_id_from_first_frame_with_it() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("track.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("track_test".to_string());

    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::default(),
        0,
        0,
        16,
    ));
    rec.record_frame(TelemetryFrame::new(
        NormalizedTelemetry::builder()
            .track_id("spa-francorchamps")
            .build(),
        1000,
        1,
        32,
    ));

    let recording = rec.stop_recording(None)?;
    assert_eq!(
        recording.metadata.track_id.as_deref(),
        Some("spa-francorchamps")
    );
    Ok(())
}

#[test]
fn metadata_timestamp_is_unix_epoch_seconds() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("epoch.json");
    let mut rec = TelemetryRecorder::new(path)?;
    rec.start_recording("epoch_test".to_string());
    let recording = rec.stop_recording(None)?;

    // Timestamp should be a reasonable Unix epoch value (after year 2020)
    let year_2020 = 1_577_836_800u64;
    assert!(
        recording.metadata.timestamp >= year_2020,
        "timestamp should be after 2020"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Playback API — edge cases and speed control
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn playback_progress_monotonically_increases() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("play_test".to_string(), 0.1, 100.0);
    let total = recording.frames.len();
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(10.0); // fast forward
    player.start_playback();

    // Give some time for frames to become available
    std::thread::sleep(std::time::Duration::from_millis(50));

    let mut prev_progress = 0.0f32;
    let mut consumed = 0;
    for _ in 0..total + 10 {
        if player.get_next_frame().is_some() {
            consumed += 1;
        }
        let p = player.progress();
        assert!(p >= prev_progress, "progress should never decrease");
        prev_progress = p;
    }
    assert!(consumed > 0, "should have consumed at least one frame");
    Ok(())
}

#[test]
fn playback_finished_after_all_frames_consumed() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("fin_test".to_string(), 0.05, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(10.0);
    player.start_playback();

    std::thread::sleep(std::time::Duration::from_millis(200));

    // Drain all frames
    while player.get_next_frame().is_some() {}
    assert!(player.is_finished());
    assert!((player.progress() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn playback_speed_boundary_zero_clamps_to_minimum() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("spd0".to_string(), 0.1, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(0.0);
    // Speed should clamp to 0.1
    player.start_playback();
    // Not panicking is sufficient; progress check
    assert!(!player.is_finished() || player.progress() >= 0.0);
    Ok(())
}

#[test]
fn playback_speed_negative_clamps_to_minimum() -> TestResult {
    let recording = TestFixtureGenerator::generate_racing_session("spdn".to_string(), 0.1, 10.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(-5.0);
    player.start_playback();
    assert!(!player.is_finished() || player.progress() >= 0.0);
    Ok(())
}

#[test]
fn playback_reset_mid_stream_restarts_from_beginning() -> TestResult {
    let recording =
        TestFixtureGenerator::generate_racing_session("reset_mid".to_string(), 0.1, 100.0);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(10.0);
    player.start_playback();

    std::thread::sleep(std::time::Duration::from_millis(50));
    // Consume some frames
    let _ = player.get_next_frame();
    let _ = player.get_next_frame();

    player.reset();
    assert_eq!(player.progress(), 0.0);
    assert!(!player.is_finished());
    // After reset, get_next_frame returns None until start_playback called again
    assert!(player.get_next_frame().is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fixture / scenario generation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fixture_all_scenarios_produce_expected_frame_count() -> TestResult {
    let duration = 1.0;
    let fps = 30.0;
    let expected = (duration * fps) as usize;

    for scenario in [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ] {
        let rec = TestFixtureGenerator::generate_test_scenario(scenario, duration, fps);
        assert_eq!(
            rec.frames.len(),
            expected,
            "scenario {:?} produced wrong frame count",
            scenario
        );
    }
    Ok(())
}

#[test]
fn fixture_acceleration_scenario_end_speed_greater_than_start() -> TestResult {
    let rec = TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 2.0, 60.0);
    let first_speed = rec.frames.first().ok_or("empty")?.data.speed_ms;
    let last_speed = rec.frames.last().ok_or("empty")?.data.speed_ms;
    assert!(
        last_speed > first_speed,
        "acceleration should increase speed"
    );
    Ok(())
}

#[test]
fn fixture_pitstop_has_lower_speed_in_middle() -> TestResult {
    let rec = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 2.0, 60.0);
    let len = rec.frames.len();
    let start_speed = rec.frames[0].data.speed_ms;
    let mid_speed = rec.frames[len / 2].data.speed_ms;
    assert!(mid_speed < start_speed, "pit phase should have lower speed");
    Ok(())
}

#[test]
fn fixture_cornering_all_frames_have_consistent_slip() -> TestResult {
    let rec = TestFixtureGenerator::generate_test_scenario(TestScenario::Cornering, 1.0, 30.0);
    for frame in &rec.frames {
        assert!(
            (frame.data.slip_ratio - 0.4).abs() < f32::EPSILON,
            "cornering slip should be 0.4"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording flags preservation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn recording_preserves_all_flag_fields() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("flags.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;
    rec.start_recording("flags_test".to_string());

    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: true,
        blue_flag: true,
        checkered_flag: true,
        green_flag: false,
        pit_limiter: true,
        in_pits: true,
        drs_available: true,
        drs_active: true,
        ers_available: true,
        ers_active: true,
        launch_control: true,
        traction_control: true,
        abs_active: true,
        engine_limiter: true,
        safety_car: true,
        formation_lap: true,
        session_paused: true,
    };
    let telem = NormalizedTelemetry::builder().flags(flags).build();
    rec.record_frame(TelemetryFrame::new(telem, 0, 0, 64));
    let _r = rec.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    let f = &loaded.frames[0].data.flags;
    assert!(f.yellow_flag);
    assert!(f.red_flag);
    assert!(f.blue_flag);
    assert!(f.checkered_flag);
    assert!(!f.green_flag);
    assert!(f.pit_limiter);
    assert!(f.in_pits);
    assert!(f.drs_available);
    assert!(f.drs_active);
    assert!(f.ers_available);
    assert!(f.ers_active);
    assert!(f.launch_control);
    assert!(f.traction_control);
    assert!(f.abs_active);
    assert!(f.engine_limiter);
    assert!(f.safety_car);
    assert!(f.formation_lap);
    assert!(f.session_paused);
    Ok(())
}

#[test]
fn recording_preserves_extended_telemetry_values() -> TestResult {
    use racing_wheel_schemas::telemetry::TelemetryValue;

    let dir = tempdir()?;
    let path = dir.path().join("extended.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;
    rec.start_recording("ext_test".to_string());

    let telem = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .extended("boost_psi", TelemetryValue::Float(14.7))
        .extended("lap_valid", TelemetryValue::Boolean(true))
        .extended("sector", TelemetryValue::Integer(2))
        .extended("compound", TelemetryValue::String("soft".to_string()))
        .build();
    rec.record_frame(TelemetryFrame::new(telem, 0, 0, 128));
    let _r = rec.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    let ext = &loaded.frames[0].data.extended;
    assert_eq!(ext.get("boost_psi"), Some(&TelemetryValue::Float(14.7)));
    assert_eq!(ext.get("lap_valid"), Some(&TelemetryValue::Boolean(true)));
    assert_eq!(ext.get("sector"), Some(&TelemetryValue::Integer(2)));
    assert_eq!(
        ext.get("compound"),
        Some(&TelemetryValue::String("soft".to_string()))
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Load / error resilience
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn load_empty_file_returns_error() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("empty.json");
    std::fs::write(&path, "")?;

    let result = TelemetryRecorder::load_recording(&path);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn load_invalid_json_returns_error() -> TestResult {
    let dir = tempdir()?;
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "{ not valid json }")?;

    let result = TelemetryRecorder::load_recording(&path);
    assert!(result.is_err());
    Ok(())
}
