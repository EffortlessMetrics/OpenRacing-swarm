//! Comprehensive tests for telemetry session recording, replay, and analysis.
//!
//! Covers: lifecycle, recording format, replay fidelity, metadata,
//! multiple simultaneous sessions, large sessions, export formats (CSV / JSON / binary),
//! and session comparison / diff.

use openracing_telemetry_recorder::{
    FieldDiff, RecordingMetadata, TelemetryPlayer, TelemetryRecorder, TelemetryRecording,
    TestFixtureGenerator, TestScenario,
};
use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn make_frame(rpm: f32, speed: f32, gear: i8, ts_ns: u64, seq: u64) -> TelemetryFrame {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(rpm)
        .speed_ms(speed)
        .gear(gear)
        .ffb_scalar(0.5)
        .slip_ratio(0.1)
        .build();
    TelemetryFrame::new(telemetry, ts_ns, seq, 64)
}

fn make_frame_with_car_track(
    rpm: f32,
    speed: f32,
    gear: i8,
    ts_ns: u64,
    seq: u64,
    car: &str,
    track: &str,
) -> TelemetryFrame {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(rpm)
        .speed_ms(speed)
        .gear(gear)
        .ffb_scalar(0.5)
        .slip_ratio(0.1)
        .car_id(car)
        .track_id(track)
        .build();
    TelemetryFrame::new(telemetry, ts_ns, seq, 64)
}

fn make_simple_recording(game_id: &str, frame_count: usize) -> TelemetryRecording {
    let mut frames = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        frames.push(make_frame(
            3000.0 + (i as f32) * 100.0,
            20.0 + (i as f32) * 2.0,
            ((i % 6) as i8) + 1,
            (i as u64) * 16_666_667, // ~60 Hz
            i as u64,
        ));
    }
    TelemetryRecording {
        metadata: RecordingMetadata {
            game_id: game_id.to_string(),
            timestamp: 1_700_000_000,
            duration_seconds: frame_count as f64 / 60.0,
            frame_count,
            average_fps: 60.0,
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
            description: Some("unit test session".to_string()),
        },
        frames,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Session start / stop lifecycle
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn session_lifecycle_start_record_stop() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("session.json");
    let mut rec = TelemetryRecorder::new(path)?;

    assert!(!rec.is_recording());
    assert_eq!(rec.frame_count(), 0);

    rec.start_recording("iracing".to_string());
    assert!(rec.is_recording());

    rec.record_frame(make_frame(5000.0, 40.0, 3, 1_000_000, 0));
    rec.record_frame(make_frame(5200.0, 42.0, 3, 2_000_000, 1));
    assert_eq!(rec.frame_count(), 2);

    let recording = rec.stop_recording(Some("test session".to_string()))?;
    assert!(!rec.is_recording());
    assert_eq!(recording.frames.len(), 2);
    assert_eq!(recording.metadata.game_id, "iracing");
    assert!(recording.metadata.description.as_deref() == Some("test session"));

    Ok(())
}

#[test]
fn session_stop_without_start_errors() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("session.json");
    let mut rec = TelemetryRecorder::new(path)?;

    let result = rec.stop_recording(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn session_restart_clears_previous_frames() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("session.json");
    let mut rec = TelemetryRecorder::new(path)?;

    rec.start_recording("game_a".to_string());
    rec.record_frame(make_frame(3000.0, 20.0, 2, 100, 0));
    rec.record_frame(make_frame(3100.0, 21.0, 2, 200, 1));
    assert_eq!(rec.frame_count(), 2);

    // Restart — previous frames should be cleared
    rec.start_recording("game_b".to_string());
    assert_eq!(rec.frame_count(), 0);

    rec.record_frame(make_frame(6000.0, 60.0, 5, 300, 0));
    let recording = rec.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 1);
    assert_eq!(recording.metadata.game_id, "game_b");

    Ok(())
}

#[test]
fn session_frames_before_start_are_ignored() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("session.json");
    let mut rec = TelemetryRecorder::new(path)?;

    rec.record_frame(make_frame(1000.0, 10.0, 1, 100, 0));
    assert_eq!(rec.frame_count(), 0);

    rec.start_recording("test".to_string());
    rec.record_frame(make_frame(2000.0, 20.0, 2, 200, 0));
    assert_eq!(rec.frame_count(), 1);

    let recording = rec.stop_recording(None)?;
    assert_eq!(recording.frames.len(), 1);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Recording format — save, load roundtrip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn recording_save_and_load_preserves_all_fields() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("roundtrip.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;

    rec.start_recording("acc".to_string());
    rec.record_frame(make_frame_with_car_track(
        7500.0,
        55.0,
        4,
        1_000_000,
        0,
        "ferrari_488",
        "monza",
    ));
    rec.record_frame(make_frame_with_car_track(
        7600.0,
        56.0,
        4,
        2_000_000,
        1,
        "ferrari_488",
        "monza",
    ));
    let original = rec.stop_recording(Some("roundtrip test".to_string()))?;

    let loaded = TelemetryRecorder::load_recording(&path)?;

    assert_eq!(loaded.metadata.game_id, original.metadata.game_id);
    assert_eq!(loaded.metadata.frame_count, original.metadata.frame_count);
    assert_eq!(loaded.metadata.car_id, original.metadata.car_id);
    assert_eq!(loaded.metadata.track_id, original.metadata.track_id);
    assert_eq!(loaded.metadata.description, original.metadata.description);
    assert_eq!(loaded.frames.len(), original.frames.len());

    for (a, b) in loaded.frames.iter().zip(original.frames.iter()) {
        assert_eq!(a.timestamp_ns, b.timestamp_ns);
        assert_eq!(a.sequence, b.sequence);
        assert_eq!(a.raw_size, b.raw_size);
        assert!((a.data.rpm - b.data.rpm).abs() < f32::EPSILON);
        assert!((a.data.speed_ms - b.data.speed_ms).abs() < f32::EPSILON);
        assert_eq!(a.data.gear, b.data.gear);
    }

    Ok(())
}

#[test]
fn recording_load_nonexistent_path_fails() -> anyhow::Result<()> {
    let result = TelemetryRecorder::load_recording("nonexistent_file_12345.json");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn recording_flags_survive_json_roundtrip() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("flags.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;

    rec.start_recording("f1".to_string());

    let flags = TelemetryFlags {
        yellow_flag: true,
        drs_active: true,
        abs_active: true,
        in_pits: true,
        ..TelemetryFlags::default()
    };
    let telemetry = NormalizedTelemetry::builder()
        .rpm(12000.0)
        .speed_ms(80.0)
        .gear(6)
        .flags(flags)
        .build();
    rec.record_frame(TelemetryFrame::new(telemetry, 100, 0, 128));
    rec.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    let f = &loaded.frames[0];
    assert!(f.data.flags.yellow_flag);
    assert!(f.data.flags.drs_active);
    assert!(f.data.flags.abs_active);
    assert!(f.data.flags.in_pits);
    assert!(!f.data.flags.red_flag);

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Replay fidelity — recorded data matches played-back data
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn replay_returns_frames_in_order() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 5);
    let mut player = TelemetryPlayer::new(recording.clone());
    player.set_playback_speed(10.0); // fast playback
    player.start_playback();

    // Wait enough for all frames to be "due"
    thread::sleep(Duration::from_millis(200));

    let mut collected = Vec::new();
    while let Some(frame) = player.get_next_frame() {
        collected.push(frame);
    }

    assert_eq!(collected.len(), 5);
    for (i, frame) in collected.iter().enumerate() {
        assert_eq!(frame.sequence, i as u64);
    }

    Ok(())
}

#[test]
fn replay_frame_data_matches_original() -> anyhow::Result<()> {
    let recording = make_simple_recording("acc", 10);
    let mut player = TelemetryPlayer::new(recording.clone());
    player.set_playback_speed(10.0);
    player.start_playback();

    thread::sleep(Duration::from_millis(500));

    let mut replayed = Vec::new();
    while let Some(frame) = player.get_next_frame() {
        replayed.push(frame);
    }

    assert_eq!(replayed.len(), recording.frames.len());
    for (original, played) in recording.frames.iter().zip(replayed.iter()) {
        assert!((original.data.rpm - played.data.rpm).abs() < f32::EPSILON);
        assert!((original.data.speed_ms - played.data.speed_ms).abs() < f32::EPSILON);
        assert_eq!(original.data.gear, played.data.gear);
        assert_eq!(original.timestamp_ns, played.timestamp_ns);
    }

    Ok(())
}

#[test]
fn replay_progress_tracks_correctly() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 4);
    let mut player = TelemetryPlayer::new(recording);

    assert!((player.progress() - 0.0).abs() < f32::EPSILON);
    assert!(!player.is_finished());

    player.set_playback_speed(10.0);
    player.start_playback();
    thread::sleep(Duration::from_millis(200));

    // Consume all frames
    while player.get_next_frame().is_some() {}

    assert!((player.progress() - 1.0).abs() < f32::EPSILON);
    assert!(player.is_finished());

    Ok(())
}

#[test]
fn replay_reset_restarts_from_beginning() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 3);
    let mut player = TelemetryPlayer::new(recording);
    player.set_playback_speed(10.0);
    player.start_playback();
    thread::sleep(Duration::from_millis(200));

    // Drain all frames
    while player.get_next_frame().is_some() {}
    assert!(player.is_finished());

    player.reset();
    assert!(!player.is_finished());
    assert!((player.progress() - 0.0).abs() < f32::EPSILON);

    // Start again — should be able to replay
    player.start_playback();
    thread::sleep(Duration::from_millis(200));
    let first = player.get_next_frame();
    assert!(first.is_some());

    Ok(())
}

#[test]
fn replay_empty_recording_is_finished_immediately() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 0);
    let mut player = TelemetryPlayer::new(recording);

    assert!(player.is_finished());
    assert!((player.progress() - 1.0).abs() < f32::EPSILON);

    player.start_playback();
    assert!(player.get_next_frame().is_none());

    Ok(())
}

#[test]
fn replay_without_start_returns_none() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 5);
    let mut player = TelemetryPlayer::new(recording);

    assert!(player.get_next_frame().is_none());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Session metadata
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn metadata_game_id_preserved() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("meta.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;

    rec.start_recording("raceroom".to_string());
    rec.record_frame(make_frame(4000.0, 30.0, 3, 100, 0));
    let recording = rec.stop_recording(Some("meta test".to_string()))?;

    assert_eq!(recording.metadata.game_id, "raceroom");

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.metadata.game_id, "raceroom");

    Ok(())
}

#[test]
fn metadata_car_and_track_extracted_from_frames() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("car_track.json");
    let mut rec = TelemetryRecorder::new(path)?;

    rec.start_recording("acc".to_string());
    rec.record_frame(make_frame_with_car_track(
        6000.0,
        50.0,
        4,
        100,
        0,
        "porsche_911",
        "spa",
    ));
    let recording = rec.stop_recording(None)?;

    assert_eq!(recording.metadata.car_id.as_deref(), Some("porsche_911"));
    assert_eq!(recording.metadata.track_id.as_deref(), Some("spa"));

    Ok(())
}

#[test]
fn metadata_duration_and_fps_are_positive() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("duration.json");
    let mut rec = TelemetryRecorder::new(path)?;

    rec.start_recording("test".to_string());
    for i in 0..10 {
        rec.record_frame(make_frame(
            3000.0,
            20.0,
            2,
            (i as u64) * 16_000_000,
            i as u64,
        ));
    }
    // Small sleep so duration is measurably positive
    thread::sleep(Duration::from_millis(10));
    let recording = rec.stop_recording(None)?;

    assert!(recording.metadata.duration_seconds > 0.0);
    assert!(recording.metadata.average_fps > 0.0);
    assert_eq!(recording.metadata.frame_count, 10);

    Ok(())
}

#[test]
fn metadata_timestamp_is_reasonable_epoch() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("ts.json");
    let mut rec = TelemetryRecorder::new(path)?;

    rec.start_recording("test".to_string());
    rec.record_frame(make_frame(3000.0, 20.0, 2, 100, 0));
    let recording = rec.stop_recording(None)?;

    // Timestamp should be after 2020-01-01 (epoch 1577836800)
    assert!(recording.metadata.timestamp > 1_577_836_800);
    Ok(())
}

#[test]
fn metadata_none_fields_serialize_correctly() -> anyhow::Result<()> {
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

    let json = serde_json::to_string(&recording)?;
    let deserialized: TelemetryRecording = serde_json::from_str(&json)?;

    assert!(deserialized.metadata.car_id.is_none());
    assert!(deserialized.metadata.track_id.is_none());
    assert!(deserialized.metadata.description.is_none());

    Ok(())
}

#[test]
fn player_metadata_reflects_recording() -> anyhow::Result<()> {
    let recording = make_simple_recording("iracing", 20);
    let player = TelemetryPlayer::new(recording.clone());

    let meta = player.metadata();
    assert_eq!(meta.game_id, "iracing");
    assert_eq!(meta.frame_count, 20);
    assert_eq!(meta.car_id.as_deref(), Some("test_car"));
    assert_eq!(meta.track_id.as_deref(), Some("test_track"));

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Multiple simultaneous sessions
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn multiple_recorders_independent() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path_a = dir.path().join("session_a.json");
    let path_b = dir.path().join("session_b.json");

    let mut rec_a = TelemetryRecorder::new(path_a.clone())?;
    let mut rec_b = TelemetryRecorder::new(path_b.clone())?;

    rec_a.start_recording("iracing".to_string());
    rec_b.start_recording("acc".to_string());

    rec_a.record_frame(make_frame(5000.0, 40.0, 3, 100, 0));
    rec_b.record_frame(make_frame(6000.0, 50.0, 4, 100, 0));
    rec_a.record_frame(make_frame(5100.0, 41.0, 3, 200, 1));
    rec_b.record_frame(make_frame(6100.0, 51.0, 4, 200, 1));
    rec_b.record_frame(make_frame(6200.0, 52.0, 4, 300, 2));

    let recording_a = rec_a.stop_recording(None)?;
    let recording_b = rec_b.stop_recording(None)?;

    assert_eq!(recording_a.frames.len(), 2);
    assert_eq!(recording_b.frames.len(), 3);
    assert_eq!(recording_a.metadata.game_id, "iracing");
    assert_eq!(recording_b.metadata.game_id, "acc");

    // Files should be independent
    let loaded_a = TelemetryRecorder::load_recording(&path_a)?;
    let loaded_b = TelemetryRecorder::load_recording(&path_b)?;
    assert_eq!(loaded_a.frames.len(), 2);
    assert_eq!(loaded_b.frames.len(), 3);

    Ok(())
}

#[test]
fn multiple_players_independent() -> anyhow::Result<()> {
    let rec_a = make_simple_recording("game_a", 5);
    let rec_b = make_simple_recording("game_b", 10);

    let mut player_a = TelemetryPlayer::new(rec_a);
    let mut player_b = TelemetryPlayer::new(rec_b);

    player_a.set_playback_speed(10.0);
    player_b.set_playback_speed(10.0);
    player_a.start_playback();
    player_b.start_playback();

    thread::sleep(Duration::from_millis(500));

    let mut count_a = 0usize;
    let mut count_b = 0usize;
    while player_a.get_next_frame().is_some() {
        count_a += 1;
    }
    while player_b.get_next_frame().is_some() {
        count_b += 1;
    }

    assert_eq!(count_a, 5);
    assert_eq!(count_b, 10);

    Ok(())
}

#[test]
fn sequential_sessions_on_same_recorder() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("sequential.json");
    let mut rec = TelemetryRecorder::new(path)?;

    // First session
    rec.start_recording("game_1".to_string());
    rec.record_frame(make_frame(3000.0, 20.0, 2, 100, 0));
    let first = rec.stop_recording(Some("first".to_string()))?;

    // Second session on same recorder
    rec.start_recording("game_2".to_string());
    rec.record_frame(make_frame(4000.0, 30.0, 3, 200, 0));
    rec.record_frame(make_frame(4100.0, 31.0, 3, 300, 1));
    let second = rec.stop_recording(Some("second".to_string()))?;

    assert_eq!(first.frames.len(), 1);
    assert_eq!(first.metadata.game_id, "game_1");
    assert_eq!(second.frames.len(), 2);
    assert_eq!(second.metadata.game_id, "game_2");

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Large session handling
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn large_session_ten_thousand_frames() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 10_000);

    assert_eq!(recording.frames.len(), 10_000);
    assert_eq!(recording.metadata.frame_count, 10_000);

    // Verify first and last frames are distinct
    let first = &recording.frames[0];
    let last = &recording.frames[9_999];
    assert!(first.timestamp_ns < last.timestamp_ns);
    assert!(first.sequence < last.sequence);

    Ok(())
}

#[test]
fn large_session_json_roundtrip() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("large.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;

    rec.start_recording("test".to_string());
    for i in 0..5_000 {
        rec.record_frame(make_frame(
            3000.0 + (i as f32),
            20.0 + (i as f32) * 0.01,
            ((i % 6) as i8) + 1,
            (i as u64) * 16_666_667,
            i as u64,
        ));
    }
    let original = rec.stop_recording(None)?;

    let loaded = TelemetryRecorder::load_recording(&path)?;
    assert_eq!(loaded.frames.len(), original.frames.len());

    // Spot-check first, middle, last frame
    for idx in [0, 2500, 4999] {
        let a = &original.frames[idx];
        let b = &loaded.frames[idx];
        assert!((a.data.rpm - b.data.rpm).abs() < f32::EPSILON);
        assert_eq!(a.sequence, b.sequence);
    }

    Ok(())
}

#[test]
fn large_session_binary_roundtrip() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 5_000);

    let binary = recording.to_binary()?;
    assert!(!binary.is_empty());

    let restored = TelemetryRecording::from_binary(&binary)?;
    assert_eq!(restored.frames.len(), 5_000);
    assert_eq!(restored.metadata.game_id, "test");

    // Spot-check
    assert!((restored.frames[0].data.rpm - recording.frames[0].data.rpm).abs() < f32::EPSILON);
    assert!(
        (restored.frames[4999].data.speed_ms - recording.frames[4999].data.speed_ms).abs()
            < f32::EPSILON
    );

    Ok(())
}

#[test]
fn large_fixture_generation() -> anyhow::Result<()> {
    let recording = TestFixtureGenerator::generate_racing_session("test".to_string(), 60.0, 60.0);

    assert_eq!(recording.frames.len(), 3_600);
    assert_eq!(recording.metadata.frame_count, 3_600);

    // Timestamps should be monotonically increasing
    for window in recording.frames.windows(2) {
        assert!(window[0].timestamp_ns < window[1].timestamp_ns);
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Export formats — CSV, compact JSON, binary
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn export_csv_header_and_row_count() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 3);
    let csv = recording.to_csv();

    let lines: Vec<&str> = csv.lines().collect();
    // 1 header + 3 data rows
    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[0],
        "timestamp_ns,frame_index,raw_size,ffb_scalar,rpm,speed_ms,slip_ratio,gear"
    );

    Ok(())
}

#[test]
fn export_csv_values_match_frame_data() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 1);
    let csv = recording.to_csv();
    let data_line = csv.lines().nth(1);
    assert!(data_line.is_some());

    let cols: Vec<&str> = data_line
        .map(|l| l.split(',').collect())
        .unwrap_or_default();
    assert_eq!(cols.len(), 8);

    let f = &recording.frames[0];
    assert_eq!(cols[0], f.timestamp_ns.to_string());
    assert_eq!(cols[1], "0"); // frame_index for first frame
    assert_eq!(cols[2], f.raw_size.to_string());
    assert_eq!(cols[7], f.data.gear.to_string());

    Ok(())
}

#[test]
fn export_csv_empty_recording() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 0);
    let csv = recording.to_csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 1); // header only
    Ok(())
}

#[test]
fn export_compact_json_is_valid_json() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 5);
    let bytes = recording.to_compact_json()?;

    let deserialized: TelemetryRecording = serde_json::from_slice(&bytes)?;
    assert_eq!(deserialized.frames.len(), 5);
    assert_eq!(deserialized.metadata.game_id, "test");

    Ok(())
}

#[test]
fn export_compact_json_smaller_than_pretty() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 50);

    let compact = recording.to_compact_json()?;
    let pretty = serde_json::to_vec_pretty(&recording)?;

    assert!(compact.len() < pretty.len());

    Ok(())
}

#[test]
fn export_binary_roundtrip_exact() -> anyhow::Result<()> {
    let original = make_simple_recording("iracing", 20);

    let binary = original.to_binary()?;
    let restored = TelemetryRecording::from_binary(&binary)?;

    assert_eq!(restored.metadata.game_id, original.metadata.game_id);
    assert_eq!(restored.metadata.frame_count, original.metadata.frame_count);
    assert_eq!(restored.metadata.car_id, original.metadata.car_id);
    assert_eq!(restored.metadata.track_id, original.metadata.track_id);
    assert_eq!(restored.frames.len(), original.frames.len());

    for (a, b) in restored.frames.iter().zip(original.frames.iter()) {
        assert_eq!(a.timestamp_ns, b.timestamp_ns);
        assert_eq!(a.sequence, b.sequence);
        assert!((a.data.rpm - b.data.rpm).abs() < f32::EPSILON);
        assert!((a.data.speed_ms - b.data.speed_ms).abs() < f32::EPSILON);
        assert_eq!(a.data.gear, b.data.gear);
    }

    Ok(())
}

#[test]
fn export_binary_differs_from_pretty_json() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 100);

    let binary = recording.to_binary()?;
    let pretty = serde_json::to_vec_pretty(&recording)?;

    // Binary (length-prefixed compact JSON) should be smaller than pretty JSON
    assert!(binary.len() < pretty.len());

    Ok(())
}

#[test]
fn export_binary_invalid_data_fails() -> anyhow::Result<()> {
    let result = TelemetryRecording::from_binary(&[0xff, 0xfe, 0xfd]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn export_binary_empty_recording() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 0);

    let binary = recording.to_binary()?;
    let restored = TelemetryRecording::from_binary(&binary)?;

    assert_eq!(restored.frames.len(), 0);
    assert_eq!(restored.metadata.game_id, "test");

    Ok(())
}

#[test]
fn export_formats_all_produce_output() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 10);

    let csv = recording.to_csv();
    let json = recording.to_compact_json()?;
    let binary = recording.to_binary()?;

    assert!(!csv.is_empty());
    assert!(!json.is_empty());
    assert!(!binary.is_empty());

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Session comparison / diff
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn diff_identical_sessions_is_empty() -> anyhow::Result<()> {
    let recording = make_simple_recording("test", 10);
    let diff = recording.diff(&recording);

    assert!(diff.is_identical());
    assert!(diff.metadata_diffs.is_empty());
    assert_eq!(diff.frame_count_delta, 0);
    assert!(diff.field_diffs.is_empty());

    Ok(())
}

#[test]
fn diff_detects_game_id_change() -> anyhow::Result<()> {
    let a = make_simple_recording("iracing", 5);
    let b = make_simple_recording("acc", 5);

    let diff = a.diff(&b);
    assert!(!diff.is_identical());
    assert!(diff.metadata_diffs.iter().any(|d| d.contains("game_id")));

    Ok(())
}

#[test]
fn diff_detects_frame_count_difference() -> anyhow::Result<()> {
    let a = make_simple_recording("test", 10);
    let b = make_simple_recording("test", 7);

    let diff = a.diff(&b);
    assert_eq!(diff.frame_count_delta, 3); // 10 - 7

    Ok(())
}

#[test]
fn diff_detects_frame_data_changes() -> anyhow::Result<()> {
    let a = make_simple_recording("test", 3);
    let mut b = make_simple_recording("test", 3);

    // Modify the second frame's RPM
    b.frames[1].data.rpm = 9999.0;

    let diff = a.diff(&b);
    assert!(!diff.is_identical());

    let rpm_diffs: Vec<&FieldDiff> = diff
        .field_diffs
        .iter()
        .filter(|d| d.field == "rpm" && d.frame_index == 1)
        .collect();
    assert_eq!(rpm_diffs.len(), 1);

    Ok(())
}

#[test]
fn diff_detects_gear_change() -> anyhow::Result<()> {
    let a = make_simple_recording("test", 2);
    let mut b = make_simple_recording("test", 2);
    b.frames[0].data.gear = 6;

    let diff = a.diff(&b);
    let gear_diffs: Vec<&FieldDiff> = diff
        .field_diffs
        .iter()
        .filter(|d| d.field == "gear")
        .collect();
    assert!(!gear_diffs.is_empty());

    Ok(())
}

#[test]
fn diff_detects_car_id_difference() -> anyhow::Result<()> {
    let mut a = make_simple_recording("test", 1);
    let mut b = make_simple_recording("test", 1);
    a.metadata.car_id = Some("car_a".to_string());
    b.metadata.car_id = Some("car_b".to_string());

    let diff = a.diff(&b);
    assert!(diff.metadata_diffs.iter().any(|d| d.contains("car_id")));

    Ok(())
}

#[test]
fn diff_detects_description_difference() -> anyhow::Result<()> {
    let mut a = make_simple_recording("test", 1);
    let mut b = make_simple_recording("test", 1);
    a.metadata.description = Some("first".to_string());
    b.metadata.description = Some("second".to_string());

    let diff = a.diff(&b);
    assert!(
        diff.metadata_diffs
            .iter()
            .any(|d| d.contains("description"))
    );

    Ok(())
}

#[test]
fn diff_symmetric_frame_count_delta() -> anyhow::Result<()> {
    let a = make_simple_recording("test", 10);
    let b = make_simple_recording("test", 6);

    let diff_ab = a.diff(&b);
    let diff_ba = b.diff(&a);

    assert_eq!(diff_ab.frame_count_delta, 4);
    assert_eq!(diff_ba.frame_count_delta, -4);

    Ok(())
}

#[test]
fn diff_empty_vs_nonempty() -> anyhow::Result<()> {
    let a = make_simple_recording("test", 0);
    let b = make_simple_recording("test", 5);

    let diff = a.diff(&b);
    assert_eq!(diff.frame_count_delta, -5);
    assert!(diff.field_diffs.is_empty()); // no overlapping frames

    Ok(())
}

#[test]
fn diff_after_binary_roundtrip_is_identical() -> anyhow::Result<()> {
    let original = make_simple_recording("test", 20);

    let binary = original.to_binary()?;
    let restored = TelemetryRecording::from_binary(&binary)?;

    let diff = original.diff(&restored);
    assert!(diff.is_identical());

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Fixture / scenario generation tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn fixture_all_scenarios_produce_frames() -> anyhow::Result<()> {
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];

    for scenario in scenarios {
        let recording = TestFixtureGenerator::generate_test_scenario(scenario, 2.0, 30.0);
        assert!(
            !recording.frames.is_empty(),
            "{scenario:?} produced no frames"
        );
        assert_eq!(
            recording.frames.len(),
            recording.metadata.frame_count,
            "{scenario:?} frame count mismatch"
        );
    }

    Ok(())
}

#[test]
fn fixture_pitstop_has_distinct_phases() -> anyhow::Result<()> {
    let recording = TestFixtureGenerator::generate_test_scenario(TestScenario::PitStop, 5.0, 60.0);

    let pit_frames = recording
        .frames
        .iter()
        .filter(|f| f.data.flags.in_pits)
        .count();
    let non_pit_frames = recording.frames.len() - pit_frames;

    assert!(pit_frames > 0, "no pit frames found");
    assert!(non_pit_frames > 0, "no non-pit frames found");

    Ok(())
}

#[test]
fn fixture_acceleration_final_speed_higher_than_initial() -> anyhow::Result<()> {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::Acceleration, 3.0, 60.0);

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

    assert!(last_speed > first_speed);

    Ok(())
}

#[test]
fn fixture_constant_speed_variance_is_low() -> anyhow::Result<()> {
    let recording =
        TestFixtureGenerator::generate_test_scenario(TestScenario::ConstantSpeed, 2.0, 60.0);

    let speeds: Vec<f32> = recording.frames.iter().map(|f| f.data.speed_ms).collect();
    let mean = speeds.iter().sum::<f32>() / speeds.len() as f32;
    let variance = speeds.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / speeds.len() as f32;

    // Constant speed should have zero variance
    assert!(variance < 0.01, "speed variance too high: {variance}");

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Cross-format roundtrip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn cross_format_json_to_binary_roundtrip() -> anyhow::Result<()> {
    let original = make_simple_recording("test", 15);

    // JSON roundtrip
    let json_bytes = original.to_compact_json()?;
    let from_json: TelemetryRecording = serde_json::from_slice(&json_bytes)?;

    // Binary roundtrip
    let bin_bytes = from_json.to_binary()?;
    let from_bin = TelemetryRecording::from_binary(&bin_bytes)?;

    let diff = original.diff(&from_bin);
    assert!(diff.is_identical());

    Ok(())
}

#[test]
fn cross_format_binary_to_json_roundtrip() -> anyhow::Result<()> {
    let original = make_simple_recording("test", 15);

    let bin_bytes = original.to_binary()?;
    let from_bin = TelemetryRecording::from_binary(&bin_bytes)?;

    let json_bytes = from_bin.to_compact_json()?;
    let from_json: TelemetryRecording = serde_json::from_slice(&json_bytes)?;

    let diff = original.diff(&from_json);
    assert!(diff.is_identical());

    Ok(())
}

#[test]
fn file_save_and_binary_produce_equivalent_recordings() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("equiv.json");
    let mut rec = TelemetryRecorder::new(path.clone())?;

    rec.start_recording("test".to_string());
    for i in 0..10 {
        rec.record_frame(make_frame(
            3000.0 + (i as f32) * 50.0,
            20.0 + (i as f32),
            ((i % 6) as i8) + 1,
            (i as u64) * 16_666_667,
            i as u64,
        ));
    }
    let original = rec.stop_recording(None)?;

    let loaded_json = TelemetryRecorder::load_recording(&path)?;

    let bin_bytes = original.to_binary()?;
    let loaded_bin = TelemetryRecording::from_binary(&bin_bytes)?;

    let diff = loaded_json.diff(&loaded_bin);
    assert!(diff.is_identical());

    Ok(())
}
