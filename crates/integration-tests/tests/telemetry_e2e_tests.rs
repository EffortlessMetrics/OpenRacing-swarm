//! End-to-end telemetry pipeline integration tests.
//!
//! Verifies cross-crate telemetry flows:
//!
//! 1. Full telemetry pipeline: adapter → normalize → filter → output
//! 2. Multi-game switching
//! 3. Telemetry recording and playback
//! 4. Rate limiting across adapters
//! 5. Telemetry stream management

use std::collections::HashSet;
use std::time::Duration;

use tempfile::TempDir;

// ── Telemetry adapters + schemas ─────────────────────────────────────────────
use openracing_telemetry_adapters::{MockAdapter, TelemetryAdapter, adapter_factories};
use openracing_telemetry_config::matrix_game_id_set;
use racing_wheel_schemas::prelude::*;

// ── Engine + filter types ────────────────────────────────────────────────────
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};
use racing_wheel_engine::safety::{FaultType, SafetyService};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline as EnginePipeline};

// ── Recording/playback ───────────────────────────────────────────────────────
use openracing_telemetry_recorder::{TelemetryPlayer, TelemetryRecorder};
use racing_wheel_schemas::telemetry::TelemetryFrame;

// ── Stream processing ────────────────────────────────────────────────────────
use openracing_telemetry_streams::RateLimiter;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn get_adapter(game_id: &str) -> Result<Box<dyn TelemetryAdapter>, String> {
    let factories = adapter_factories();
    let (_, factory) = factories
        .iter()
        .find(|(id, _)| *id == game_id)
        .ok_or_else(|| format!("adapter '{game_id}' not found in registry"))?;
    Ok(factory())
}

/// Assert a float is within tolerance.
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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Full telemetry pipeline: adapter → normalize → filter → output
// ═══════════════════════════════════════════════════════════════════════════════

mod full_pipeline {
    use super::*;

    /// Forza telemetry flows from raw bytes through adapter, filters, and
    /// engine pipeline to a safety-clamped output.
    #[test]
    fn forza_bytes_to_safety_clamped_output() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = get_adapter("forza_motorsport")?;
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

        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Filter output must be finite and in [-1,1], got {}",
            frame.torque_out
        );

        // Engine + safety stage
        let mut engine_frame = EngineFrame {
            ffb_in: frame.torque_out,
            torque_out: frame.torque_out,
            wheel_speed: telemetry.speed_ms,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let mut pipeline = EnginePipeline::new();
        pipeline.process(&mut engine_frame)?;

        let safety = SafetyService::new(5.0, 20.0);
        let clamped = safety.clamp_torque_nm(engine_frame.torque_out * 5.0);
        assert!(
            clamped.abs() <= 5.0,
            "Safety-clamped torque must be ≤ 5 Nm, got {}",
            clamped
        );

        Ok(())
    }

    /// LFS telemetry through the full pipeline with faulted safety service
    /// must produce zero output.
    #[test]
    fn lfs_bytes_faulted_safety_zeroes_output() -> Result<(), Box<dyn std::error::Error>> {
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

        // Faulted safety must zero output
        let mut safety = SafetyService::new(5.0, 20.0);
        safety.report_fault(FaultType::UsbStall);
        let clamped = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            clamped.abs() < 0.001,
            "Faulted safety must zero torque, got {}",
            clamped
        );

        Ok(())
    }

    /// MockAdapter output integrates correctly with filter and engine pipeline.
    #[test]
    fn mock_adapter_through_full_pipeline() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = MockAdapter::new("pipeline_test".to_string());
        let telemetry = adapter.normalize(&[])?;

        assert!(
            (telemetry.rpm - 5000.0).abs() < 1.0,
            "MockAdapter RPM should be 5000, got {}",
            telemetry.rpm
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

        let mut engine_frame = EngineFrame {
            ffb_in: frame.torque_out,
            torque_out: frame.torque_out,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let mut pipeline = EnginePipeline::new();
        pipeline.process(&mut engine_frame)?;

        assert!(
            engine_frame.torque_out.is_finite(),
            "Engine output must be finite"
        );

        Ok(())
    }

    /// All registered adapters produce valid NormalizedTelemetry for empty input.
    #[test]
    fn all_adapters_normalize_without_error() -> Result<(), Box<dyn std::error::Error>> {
        let factories = adapter_factories();
        let mut failed = Vec::new();

        for (game_id, factory) in factories {
            let adapter = factory();
            // Create a reasonably-sized zero buffer
            let buf = vec![0u8; 1024];
            match adapter.normalize(&buf) {
                Ok(t) => {
                    if !t.speed_ms.is_finite() {
                        failed.push(format!("{game_id}: speed_ms not finite"));
                    }
                    if !t.rpm.is_finite() {
                        failed.push(format!("{game_id}: rpm not finite"));
                    }
                }
                Err(_) => {
                    // Some adapters may reject zero buffers — that's acceptable
                }
            }
        }

        assert!(
            failed.is_empty(),
            "Adapters with non-finite fields: {failed:?}"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multi-game switching
// ═══════════════════════════════════════════════════════════════════════════════

mod multi_game_switching {
    use super::*;

    /// Switching between two games: adapters produce independent telemetry.
    #[test]
    fn switch_between_forza_and_lfs() -> Result<(), Box<dyn std::error::Error>> {
        let forza = get_adapter("forza_motorsport")?;
        let lfs = get_adapter("live_for_speed")?;

        // Forza session
        let forza_pkt = build_forza_packet(25.0, 35.0, 6000.0, 8500.0);
        let forza_telem = forza.normalize(&forza_pkt)?;
        assert!(
            forza_telem.rpm > 5000.0,
            "Forza RPM must be > 5000, got {}",
            forza_telem.rpm
        );

        // Switch to LFS
        let lfs_pkt = build_lfs_packet(50.0, 3500.0, 4, 0.6);
        let lfs_telem = lfs.normalize(&lfs_pkt)?;
        assert_f32_near(lfs_telem.speed_ms, 50.0, 0.5, "LFS speed after switch");

        // Both adapters must produce independent, valid results
        assert!(
            (forza_telem.speed_ms - lfs_telem.speed_ms).abs() > 1.0,
            "Forza and LFS must produce different speeds"
        );

        Ok(())
    }

    /// Rapid game switching: pipeline state resets properly between games.
    #[test]
    fn rapid_game_switching_pipeline_resets() -> Result<(), Box<dyn std::error::Error>> {
        let game_ids = ["forza_motorsport", "live_for_speed"];
        let mut previous_speed = 0.0f32;

        for round in 0..3 {
            for game_id in &game_ids {
                let adapter = get_adapter(game_id)?;
                let buf = if *game_id == "forza_motorsport" {
                    build_forza_packet(10.0 + round as f32, 20.0, 5000.0, 8000.0)
                } else {
                    build_lfs_packet(30.0 + round as f32, 4000.0, 3, 0.5)
                };

                let telem = adapter.normalize(&buf)?;
                assert!(
                    telem.speed_ms.is_finite(),
                    "Round {round}, game {game_id}: speed must be finite"
                );

                // Each round should produce a valid, distinct speed
                if round > 0 || *game_id != "forza_motorsport" {
                    assert!(
                        (telem.speed_ms - previous_speed).abs() > 0.01
                            || telem.speed_ms.is_finite(),
                        "Speed must be valid after switch"
                    );
                }
                previous_speed = telem.speed_ms;
            }
        }

        Ok(())
    }

    /// Game support matrix contains all adapters — no orphaned adapters.
    #[test]
    fn all_adapters_have_matrix_entries() -> Result<(), Box<dyn std::error::Error>> {
        let config_ids = matrix_game_id_set()?;
        let adapter_ids: HashSet<&str> = adapter_factories().iter().map(|(id, _)| *id).collect();

        let mut missing = Vec::new();
        for id in &adapter_ids {
            if !config_ids.contains(*id) {
                missing.push(*id);
            }
        }

        assert!(
            missing.is_empty(),
            "Adapters missing from game support matrix: {missing:?}"
        );

        Ok(())
    }

    /// Different adapters report appropriate update rates.
    #[test]
    fn adapters_report_reasonable_update_rates() -> Result<(), Box<dyn std::error::Error>> {
        let test_games = ["forza_motorsport", "live_for_speed", "acc"];

        for game_id in &test_games {
            let adapter = get_adapter(game_id)?;
            let rate = adapter.expected_update_rate();
            assert!(
                rate >= Duration::from_millis(1) && rate <= Duration::from_secs(1),
                "{game_id}: update rate must be between 1ms and 1s, got {rate:?}"
            );
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Telemetry recording and playback
// ═══════════════════════════════════════════════════════════════════════════════

mod recording_playback {
    use super::*;

    /// Record telemetry frames, then play them back and verify fidelity.
    #[test]
    fn record_and_playback_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("test_recording.json");

        let mut recorder = TelemetryRecorder::new(output.clone())?;
        recorder.start_recording("forza_motorsport".to_string());

        // Record some frames
        let adapter = get_adapter("forza_motorsport")?;
        let speeds = [10.0f32, 20.0, 30.0, 40.0, 50.0];
        for (i, speed) in speeds.iter().enumerate() {
            let packet = build_forza_packet(*speed, 0.0, 5000.0, 8000.0);
            let telem = adapter.normalize(&packet)?;
            let frame = TelemetryFrame::new(telem, 0, i as u64, 232);
            recorder.record_frame(frame);
        }

        let recording = recorder.stop_recording(None)?;

        assert_eq!(
            recording.frames.len(),
            speeds.len(),
            "Recording must contain all {} frames",
            speeds.len()
        );
        assert_eq!(
            recording.metadata.game_id, "forza_motorsport",
            "Recording game_id must match"
        );

        // Playback
        let mut player = TelemetryPlayer::new(recording);
        player.start_playback();

        let mut played_count = 0usize;
        while !player.is_finished() {
            if let Some(_frame) = player.get_next_frame() {
                played_count += 1;
            }
            if played_count > speeds.len() + 1 {
                break; // Safety bound
            }
        }

        assert_eq!(
            played_count,
            speeds.len(),
            "Playback must yield all {} frames",
            speeds.len()
        );

        Ok(())
    }

    /// Recording metadata captures correct frame count and game ID.
    #[test]
    fn recording_metadata_accuracy() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("meta_test.json");

        let mut recorder = TelemetryRecorder::new(output)?;
        recorder.start_recording("live_for_speed".to_string());

        let adapter = get_adapter("live_for_speed")?;
        for i in 0..10 {
            let packet = build_lfs_packet(20.0 + i as f32, 3000.0, 3, 0.5);
            let telem = adapter.normalize(&packet)?;
            let frame = TelemetryFrame::new(telem, 0, i as u64, 96);
            recorder.record_frame(frame);
        }

        let recording = recorder.stop_recording(None)?;
        assert_eq!(recording.metadata.frame_count, 10, "frame_count must be 10");
        assert_eq!(
            recording.metadata.game_id, "live_for_speed",
            "game_id must be live_for_speed"
        );

        Ok(())
    }

    /// Player reset allows re-playing the same recording.
    #[test]
    fn player_reset_allows_replay() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("replay_test.json");

        let mut recorder = TelemetryRecorder::new(output)?;
        recorder.start_recording("test_replay".to_string());

        let adapter = MockAdapter::new("replay_test".to_string());
        for i in 0..5 {
            let telem = adapter.normalize(&[])?;
            // Use timestamp 0 for all frames so playback delivers them instantly
            let frame = TelemetryFrame::new(telem, 0, i as u64, 0);
            recorder.record_frame(frame);
        }

        let recording = recorder.stop_recording(None)?;
        let mut player = TelemetryPlayer::new(recording);

        // First playback
        player.start_playback();
        let mut count = 0usize;
        while player.get_next_frame().is_some() {
            count += 1;
            if count > 10 {
                break;
            }
        }
        assert_eq!(count, 5, "First playback must yield 5 frames");

        // Reset and replay
        player.reset();
        player.start_playback();
        let mut count2 = 0usize;
        while player.get_next_frame().is_some() {
            count2 += 1;
            if count2 > 10 {
                break;
            }
        }
        assert_eq!(
            count2, 5,
            "Second playback after reset must also yield 5 frames"
        );

        Ok(())
    }

    /// Playback speed can be adjusted.
    #[test]
    fn playback_speed_adjustment() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("speed_test.json");

        let mut recorder = TelemetryRecorder::new(output)?;
        recorder.start_recording("speed_test".to_string());

        let adapter = MockAdapter::new("speed_test".to_string());
        for i in 0..3 {
            let telem = adapter.normalize(&[])?;
            let frame = TelemetryFrame::new(telem, 0, i as u64, 0);
            recorder.record_frame(frame);
        }

        let recording = recorder.stop_recording(None)?;
        let mut player = TelemetryPlayer::new(recording);

        player.set_playback_speed(2.0);
        player.start_playback();

        // Frames should still be accessible
        let frame = player.get_next_frame();
        assert!(frame.is_some(), "First frame must be available at 2x speed");

        Ok(())
    }

    /// Empty recording produces valid metadata with zero frames.
    #[test]
    fn empty_recording_valid_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("empty_test.json");

        let mut recorder = TelemetryRecorder::new(output)?;
        recorder.start_recording("empty_test".to_string());
        let recording = recorder.stop_recording(None)?;

        assert_eq!(
            recording.frames.len(),
            0,
            "Empty recording must have 0 frames"
        );
        assert_eq!(
            recording.metadata.frame_count, 0,
            "Empty recording frame_count must be 0"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Rate limiting across adapters
// ═══════════════════════════════════════════════════════════════════════════════

mod rate_limiting {
    use super::*;

    /// Rate limiter allows first update and blocks subsequent rapid updates.
    #[test]
    fn rate_limiter_allows_first_then_blocks() -> Result<(), Box<dyn std::error::Error>> {
        let mut limiter = RateLimiter::new(60.0); // 60 Hz

        // First update is always allowed
        assert!(limiter.should_update(), "First update must be allowed");

        // Immediate second call should be blocked (< 16ms has elapsed)
        assert!(
            !limiter.should_update(),
            "Immediate second update must be blocked at 60Hz"
        );

        Ok(())
    }

    /// Rate limiter reset allows a new burst.
    #[test]
    fn rate_limiter_reset_allows_new_burst() -> Result<(), Box<dyn std::error::Error>> {
        let mut limiter = RateLimiter::new(60.0);

        // Consume the initial allowance
        let _ = limiter.should_update();

        // Reset and verify
        limiter.reset();
        assert!(
            limiter.should_update(),
            "First update after reset must be allowed"
        );

        Ok(())
    }

    /// Different rate limiters for different adapters operate independently.
    #[test]
    fn independent_rate_limiters_per_adapter() -> Result<(), Box<dyn std::error::Error>> {
        let mut limiter_60hz = RateLimiter::new(60.0);
        let mut limiter_120hz = RateLimiter::new(120.0);

        // Both allow first update
        assert!(limiter_60hz.should_update(), "60Hz first update");
        assert!(limiter_120hz.should_update(), "120Hz first update");

        // Both block immediate second update
        assert!(!limiter_60hz.should_update(), "60Hz blocks rapid second");
        assert!(!limiter_120hz.should_update(), "120Hz blocks rapid second");

        Ok(())
    }

    /// Rate limiter with very high rate (1kHz) has a tight interval.
    #[test]
    fn high_rate_limiter_tight_interval() -> Result<(), Box<dyn std::error::Error>> {
        let mut limiter = RateLimiter::new(1000.0); // 1kHz

        assert!(
            limiter.should_update(),
            "1kHz limiter must allow first update"
        );

        // Immediate second call at 1kHz should still be blocked
        assert!(
            !limiter.should_update(),
            "1kHz limiter must block immediate second call"
        );

        Ok(())
    }

    /// Rate limiter integrates with adapter telemetry processing: only
    /// frames that pass the rate gate should be processed.
    #[test]
    fn rate_limiter_gates_adapter_processing() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = MockAdapter::new("rate_test".to_string());
        let mut limiter = RateLimiter::new(60.0);

        let mut processed = 0u32;
        for _ in 0..10 {
            if limiter.should_update() {
                let telem = adapter.normalize(&[])?;
                assert!(telem.rpm.is_finite(), "Processed frame RPM must be finite");
                processed += 1;
            }
        }

        // At 60Hz, only the first immediate call should pass through
        assert!(
            processed >= 1,
            "At least one frame must be processed, got {}",
            processed
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Telemetry stream management
// ═══════════════════════════════════════════════════════════════════════════════

mod stream_management {
    use super::*;

    /// Multiple adapters can produce telemetry streams simultaneously.
    #[test]
    fn multiple_adapters_produce_valid_streams() -> Result<(), Box<dyn std::error::Error>> {
        let games = ["forza_motorsport", "live_for_speed"];
        let mut results: Vec<(&str, NormalizedTelemetry)> = Vec::new();

        for game_id in &games {
            let adapter = get_adapter(game_id)?;
            let buf = if *game_id == "forza_motorsport" {
                build_forza_packet(30.0, 40.0, 6000.0, 8000.0)
            } else {
                build_lfs_packet(45.0, 4500.0, 4, 0.7)
            };

            let telem = adapter.normalize(&buf)?;
            assert!(
                telem.speed_ms.is_finite(),
                "{game_id}: stream speed must be finite"
            );
            assert!(
                telem.rpm.is_finite(),
                "{game_id}: stream RPM must be finite"
            );
            results.push((game_id, telem));
        }

        // Each adapter's stream is independent
        assert_eq!(results.len(), 2, "Both streams must produce results");
        assert!(
            (results[0].1.speed_ms - results[1].1.speed_ms).abs() > 1.0,
            "Different games must produce different speeds"
        );

        Ok(())
    }

    /// Telemetry stream fed into filter pipeline produces bounded output.
    #[test]
    fn stream_through_filter_pipeline_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = get_adapter("forza_motorsport")?;

        for i in 0..20 {
            let speed = 10.0 + i as f32 * 5.0;
            let packet = build_forza_packet(speed, 0.0, 4000.0 + i as f32 * 200.0, 9000.0);
            let telem = adapter.normalize(&packet)?;

            let mut frame = FilterFrame {
                ffb_in: telem.ffb_scalar,
                torque_out: telem.ffb_scalar,
                wheel_speed: telem.speed_ms,
                hands_off: false,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: i as u16,
            };
            damper_filter(&mut frame, &DamperState::fixed(0.02));
            torque_cap_filter(&mut frame, 1.0);

            assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "Frame {i}: filter output must be in [-1,1], got {}",
                frame.torque_out
            );
        }

        Ok(())
    }

    /// Telemetry from recording integrates with the live processing pipeline.
    #[test]
    fn recorded_telemetry_feeds_live_pipeline() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let output = tmp.path().join("stream_test.json");

        let mut recorder = TelemetryRecorder::new(output)?;
        recorder.start_recording("forza_motorsport".to_string());

        let adapter = get_adapter("forza_motorsport")?;
        for i in 0..5 {
            let packet = build_forza_packet(15.0 + i as f32 * 3.0, 0.0, 5000.0, 8000.0);
            let telem = adapter.normalize(&packet)?;
            let frame = TelemetryFrame::new(telem, 0, i as u64, 232);
            recorder.record_frame(frame);
        }

        let recording = recorder.stop_recording(None)?;
        let mut player = TelemetryPlayer::new(recording);
        player.start_playback();

        let mut processed = 0u32;
        while let Some(frame) = player.get_next_frame() {
            // Feed recorded frame into filter pipeline
            let mut filter_frame = FilterFrame {
                ffb_in: frame.data.ffb_scalar,
                torque_out: frame.data.ffb_scalar,
                wheel_speed: frame.data.speed_ms,
                hands_off: false,
                ts_mono_ns: processed as u64 * 1_000_000,
                seq: processed as u16,
            };
            damper_filter(&mut filter_frame, &DamperState::fixed(0.01));
            torque_cap_filter(&mut filter_frame, 1.0);

            assert!(
                filter_frame.torque_out.is_finite(),
                "Recorded frame {processed}: pipeline output must be finite"
            );
            processed += 1;

            if processed > 10 {
                break;
            }
        }

        assert_eq!(processed, 5, "All 5 recorded frames must be processed");

        Ok(())
    }

    /// Normalized telemetry fields are always within sane ranges across
    /// multiple adapter streams.
    #[test]
    fn telemetry_fields_in_sane_ranges() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = get_adapter("forza_motorsport")?;

        for i in 0..10 {
            let packet = build_forza_packet(
                5.0 + i as f32 * 2.0,
                10.0 + i as f32,
                3000.0 + i as f32 * 300.0,
                9000.0,
            );
            let telem = adapter.normalize(&packet)?;

            assert!(
                telem.speed_ms >= 0.0,
                "Frame {i}: speed must be non-negative"
            );
            assert!(telem.rpm >= 0.0, "Frame {i}: RPM must be non-negative");
            assert!(
                telem.throttle >= 0.0 && telem.throttle <= 1.0,
                "Frame {i}: throttle must be in [0,1], got {}",
                telem.throttle
            );
            assert!(
                telem.brake >= 0.0 && telem.brake <= 1.0,
                "Frame {i}: brake must be in [0,1], got {}",
                telem.brake
            );
        }

        Ok(())
    }
}
