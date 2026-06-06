//! Filter Node Library for Real-Time Force Feedback Processing
//!
//! This module provides filter types and functions for the FFB pipeline.
//!
//! See the `openracing-filters` crate for detailed filter documentation.

pub use openracing_filters::{
    BumpstopState, CurveState, DamperState, FilterState, FrictionState, HandsOffState,
    InertiaState, NotchState, ReconstructionState, ResponseCurveState, SlewRateState,
};

use crate::rt::Frame;

fn debug_assert_state_ptr<T>(state: *mut u8) {
    debug_assert!(!state.is_null(), "filter state pointer is null");
    debug_assert!(
        (state as usize).is_multiple_of(std::mem::align_of::<T>()),
        "filter state pointer is not aligned for its state type"
    );
}

/// Reconstruction filter (anti-aliasing) - smooths high-frequency content
pub fn reconstruction_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<ReconstructionState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `ReconstructionState`.
    unsafe {
        let state = &mut *(state as *mut ReconstructionState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::reconstruction_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Friction filter with speed adaptation - simulates tire/road friction
pub fn friction_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<FrictionState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `FrictionState`.
    unsafe {
        let state = &*(state as *const FrictionState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::friction_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Damper filter with speed adaptation - velocity-proportional resistance
pub fn damper_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<DamperState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `DamperState`.
    unsafe {
        let state = &*(state as *const DamperState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::damper_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Inertia filter - simulates rotational inertia
pub fn inertia_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<InertiaState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `InertiaState`.
    unsafe {
        let state = &mut *(state as *mut InertiaState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::inertia_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Notch filter (biquad implementation) - eliminates specific frequencies
pub fn notch_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<NotchState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `NotchState`.
    unsafe {
        let state = &mut *(state as *mut NotchState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::notch_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Slew rate limiter - limits rate of change
pub fn slew_rate_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<SlewRateState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `SlewRateState`.
    unsafe {
        let state = &mut *(state as *mut SlewRateState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::slew_rate_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Curve mapping filter using lookup table - applies force curve
pub fn curve_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<CurveState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `CurveState`.
    unsafe {
        let state = &*(state as *const CurveState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::curve_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Response curve filter using CurveLut - applies response curve transformation
pub fn response_curve_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<ResponseCurveState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `ResponseCurveState`.
    unsafe {
        let state = &*(state as *const ResponseCurveState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::response_curve_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Torque cap filter (safety) - limits maximum torque
pub fn torque_cap_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<f32>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `f32` (max torque value).
    unsafe {
        let max_torque = *(state as *const f32);
        // SAFETY-CRITICAL: NaN/Inf must map to 0.0 (safe state), never to max_torque.
        frame.torque_out = if frame.torque_out.is_finite() {
            frame.torque_out.clamp(-max_torque, max_torque)
        } else {
            0.0
        };
    }
}

/// Bumpstop model filter - simulates physical steering stops
pub fn bumpstop_filter(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<BumpstopState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `BumpstopState`.
    unsafe {
        let state = &mut *(state as *mut BumpstopState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::bumpstop_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Hands-off detector - detects when user is not holding the wheel
pub fn hands_off_detector(frame: &mut Frame, state: *mut u8) {
    debug_assert_state_ptr::<HandsOffState>(state);
    // SAFETY: Caller guarantees `state` points to a valid, aligned `HandsOffState`.
    unsafe {
        let state = &mut *(state as *mut HandsOffState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::hands_off_detector(&mut filter_frame, state);
        frame.hands_off = filter_frame.hands_off;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openracing_filters::FilterState;
    use std::f32::consts::PI;

    fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    // ── Individual filter behavior ──────────────────────────────────

    #[test]
    fn test_reconstruction_filter() {
        let mut state = ReconstructionState::new(4);
        let mut frame = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);

        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);
    }

    #[test]
    fn test_reconstruction_filter_bypass_passes_through() -> Result<(), String> {
        let mut state = ReconstructionState::new(0); // level 0 = alpha 1.0
        let mut frame = create_test_frame(0.7, 0.0);
        reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if (frame.torque_out - 0.7).abs() >= 0.001 {
            return Err(format!(
                "bypass should pass through: expected ~0.7, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_reconstruction_filter_convergence() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        for _ in 0..200 {
            let mut frame = create_test_frame(1.0, 0.0);
            reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);
        }
        if (state.prev_output - 1.0).abs() >= 0.01 {
            return Err(format!(
                "should converge to 1.0 after 200 ticks, got {}",
                state.prev_output
            ));
        }
        Ok(())
    }

    #[test]
    fn test_reconstruction_heavy_smoothing_slower_than_light() -> Result<(), String> {
        let mut light = ReconstructionState::new(2);
        let mut heavy = ReconstructionState::new(6);
        let mut frame_l = create_test_frame(1.0, 0.0);
        let mut frame_h = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame_l, &mut light as *mut _ as *mut u8);
        reconstruction_filter(&mut frame_h, &mut heavy as *mut _ as *mut u8);
        if frame_h.torque_out >= frame_l.torque_out {
            return Err(format!(
                "heavy smoothing should lag: heavy={}, light={}",
                frame_h.torque_out, frame_l.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_friction_filter() {
        let state = FrictionState::new(0.1, true);
        let mut frame = create_test_frame(0.0, 1.0);
        friction_filter(&mut frame, &state as *const _ as *mut u8);

        assert!(frame.torque_out.abs() > 0.0);
    }

    #[test]
    fn test_friction_filter_opposes_motion() -> Result<(), String> {
        let state = FrictionState::new(0.1, false);
        let mut frame_pos = create_test_frame(0.0, 1.0);
        friction_filter(&mut frame_pos, &state as *const _ as *mut u8);
        if frame_pos.torque_out >= 0.0 {
            return Err(format!(
                "friction should oppose positive speed, got {}",
                frame_pos.torque_out
            ));
        }
        let mut frame_neg = create_test_frame(0.0, -1.0);
        friction_filter(&mut frame_neg, &state as *const _ as *mut u8);
        if frame_neg.torque_out <= 0.0 {
            return Err(format!(
                "friction should oppose negative speed, got {}",
                frame_neg.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_friction_filter_zero_speed_no_effect() -> Result<(), String> {
        let state = FrictionState::new(0.5, true);
        let mut frame = create_test_frame(0.3, 0.0);
        friction_filter(&mut frame, &state as *const _ as *mut u8);
        if (frame.torque_out - 0.3).abs() >= 0.001 {
            return Err(format!(
                "friction at zero speed should not change torque, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_friction_speed_adaptive_reduces_at_high_speed() -> Result<(), String> {
        let state = FrictionState::new(0.5, true);
        let mut frame_slow = create_test_frame(0.0, 1.0);
        friction_filter(&mut frame_slow, &state as *const _ as *mut u8);
        let slow_mag = frame_slow.torque_out.abs();

        let mut frame_fast = create_test_frame(0.0, 8.0);
        friction_filter(&mut frame_fast, &state as *const _ as *mut u8);
        let fast_mag = frame_fast.torque_out.abs();

        if fast_mag >= slow_mag {
            return Err(format!(
                "adaptive friction should be smaller at high speed: slow={}, fast={}",
                slow_mag, fast_mag
            ));
        }
        Ok(())
    }

    #[test]
    fn test_damper_filter() {
        let state = DamperState::new(0.1, true);
        let mut frame = create_test_frame(0.0, 1.0);
        damper_filter(&mut frame, &state as *const _ as *mut u8);

        assert!(frame.torque_out.abs() > 0.0);
    }

    #[test]
    fn test_damper_proportional_to_speed() -> Result<(), String> {
        let state = DamperState::new(0.2, false);
        let mut frame = create_test_frame(0.0, 1.0);
        damper_filter(&mut frame, &state as *const _ as *mut u8);
        if (frame.torque_out.abs() - 0.2).abs() >= 0.01 {
            return Err(format!(
                "damper should produce coefficient * speed = 0.2, got {}",
                frame.torque_out.abs()
            ));
        }
        Ok(())
    }

    #[test]
    fn test_damper_opposes_motion() -> Result<(), String> {
        let state = DamperState::new(0.1, false);
        let mut f_pos = create_test_frame(0.0, 5.0);
        damper_filter(&mut f_pos, &state as *const _ as *mut u8);
        let mut f_neg = create_test_frame(0.0, -5.0);
        damper_filter(&mut f_neg, &state as *const _ as *mut u8);
        if f_pos.torque_out >= 0.0 || f_neg.torque_out <= 0.0 {
            return Err(format!(
                "damper must oppose motion: pos_out={}, neg_out={}",
                f_pos.torque_out, f_neg.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_inertia_filter_opposes_acceleration() -> Result<(), String> {
        let mut state = InertiaState::new(0.1);
        let mut f0 = create_test_frame(0.0, 0.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);

        let mut f1 = create_test_frame(0.0, 5.0);
        f1.torque_out = 0.0;
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if f1.torque_out >= 0.0 {
            return Err(format!(
                "inertia should oppose positive acceleration, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_inertia_constant_speed_no_torque() -> Result<(), String> {
        let mut state = InertiaState::new(0.1);
        let mut f0 = create_test_frame(0.0, 5.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);

        let mut f1 = Frame {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed: 5.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if f1.torque_out.abs() >= 0.001 {
            return Err(format!(
                "constant speed should produce no inertia torque, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_notch_filter_bypass_passes_through() -> Result<(), String> {
        let mut state = NotchState::bypass();
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.42;
        notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if (frame.torque_out - 0.42).abs() >= 0.001 {
            return Err(format!(
                "bypass notch should pass through, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_notch_filter_attenuates_center_frequency() -> Result<(), String> {
        let center_freq = 50.0f32;
        let sample_rate = 1000.0f32;
        let mut state = NotchState::new(center_freq, 5.0, -12.0, sample_rate);

        // Feed 50 Hz sine for many cycles to reach steady state
        let mut amplitude_at_center = 0.0f32;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * center_freq * t).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
            if i > 1500 {
                amplitude_at_center = amplitude_at_center.max(frame.torque_out.abs());
            }
        }

        // Also measure passband (DC-like: 5 Hz sine should pass mostly)
        let mut state2 = NotchState::new(center_freq, 5.0, -12.0, sample_rate);
        let mut amplitude_passband = 0.0f32;
        let pass_freq = 5.0f32;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * pass_freq * t).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state2 as *mut _ as *mut u8);
            if i > 1500 {
                amplitude_passband = amplitude_passband.max(frame.torque_out.abs());
            }
        }

        if amplitude_at_center >= amplitude_passband {
            return Err(format!(
                "notch should attenuate center freq: center_amp={}, pass_amp={}",
                amplitude_at_center, amplitude_passband
            ));
        }
        Ok(())
    }

    #[test]
    fn test_lowpass_attenuates_high_frequencies() -> Result<(), String> {
        let cutoff = 50.0f32;
        let sample_rate = 1000.0f32;
        let mut state_low = NotchState::lowpass(cutoff, 0.707, sample_rate);
        let mut state_high = NotchState::lowpass(cutoff, 0.707, sample_rate);

        // Measure amplitude at 10 Hz (below cutoff)
        let mut amp_low_freq = 0.0f32;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * 10.0 * t).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state_low as *mut _ as *mut u8);
            if i > 1500 {
                amp_low_freq = amp_low_freq.max(frame.torque_out.abs());
            }
        }

        // Measure amplitude at 200 Hz (above cutoff)
        let mut amp_high_freq = 0.0f32;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * 200.0 * t).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state_high as *mut _ as *mut u8);
            if i > 1500 {
                amp_high_freq = amp_high_freq.max(frame.torque_out.abs());
            }
        }

        if amp_high_freq >= amp_low_freq {
            return Err(format!(
                "lowpass should attenuate high freqs: low_amp={}, high_amp={}",
                amp_low_freq, amp_high_freq
            ));
        }
        Ok(())
    }

    #[test]
    fn test_slew_rate_limits_step_change() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5);
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 1.0;
        slew_rate_filter(&mut frame, &mut state as *mut _ as *mut u8);
        let max_per_tick = 0.5 / 1000.0;
        if (frame.torque_out - max_per_tick).abs() >= 0.0001 {
            return Err(format!(
                "slew rate should limit to {} per tick, got {}",
                max_per_tick, frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_slew_rate_unlimited_passes_through() -> Result<(), String> {
        let mut state = SlewRateState::unlimited();
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.9;
        slew_rate_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if (frame.torque_out - 0.9).abs() >= 0.001 {
            return Err(format!(
                "unlimited slew should pass through, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_slew_rate_convergence() -> Result<(), String> {
        let mut state = SlewRateState::new(1.0);
        for _ in 0..2000 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 1.0;
            slew_rate_filter(&mut frame, &mut state as *mut _ as *mut u8);
        }
        if (state.prev_output - 1.0).abs() >= 0.01 {
            return Err(format!(
                "slew should converge to 1.0, got {}",
                state.prev_output
            ));
        }
        Ok(())
    }

    #[test]
    fn test_curve_filter_linear_identity() -> Result<(), String> {
        let state = CurveState::linear();
        for &input in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            curve_filter(&mut frame, &state as *const _ as *mut u8);
            if (frame.torque_out - input).abs() >= 0.02 {
                return Err(format!(
                    "linear curve at {} should be ~{}, got {}",
                    input, input, frame.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn test_curve_filter_preserves_sign() -> Result<(), String> {
        let state = CurveState::linear();
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = -0.5;
        curve_filter(&mut frame, &state as *const _ as *mut u8);
        if frame.torque_out >= 0.0 {
            return Err(format!(
                "curve should preserve negative sign, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_response_curve_linear_identity() -> Result<(), String> {
        let state = ResponseCurveState::linear();
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.5;
        response_curve_filter(&mut frame, &state as *const _ as *mut u8);
        if (frame.torque_out - 0.5).abs() >= 0.02 {
            return Err(format!(
                "linear response curve at 0.5 should be ~0.5, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_bumpstop_disabled_no_effect() -> Result<(), String> {
        let mut state = BumpstopState::disabled();
        let mut frame = create_test_frame(0.0, 5.0);
        bumpstop_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if frame.torque_out.abs() >= 0.001 {
            return Err(format!(
                "disabled bumpstop should have no effect, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_bumpstop_applies_resistance_past_start_angle() -> Result<(), String> {
        let mut state = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        state.current_angle = 15.0;
        let mut frame = create_test_frame(0.0, 0.0);
        bumpstop_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if frame.torque_out.abs() < 0.001 {
            return Err("bumpstop past start angle should apply force".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_hands_off_triggers_after_timeout() -> Result<(), String> {
        let mut state = HandsOffState::new(true, 0.05, 0.1); // 100ms
        for _ in 0..150 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.01;
            hands_off_detector(&mut frame, &mut state as *mut _ as *mut u8);
        }
        let mut final_frame = create_test_frame(0.0, 0.0);
        final_frame.torque_out = 0.01;
        hands_off_detector(&mut final_frame, &mut state as *mut _ as *mut u8);
        if !final_frame.hands_off {
            return Err(format!(
                "hands_off should be true after timeout, counter={}",
                state.counter
            ));
        }
        Ok(())
    }

    #[test]
    fn test_hands_off_resets_on_resistance() -> Result<(), String> {
        let mut state = HandsOffState::new(true, 0.05, 0.5);
        for _ in 0..200 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.01;
            hands_off_detector(&mut frame, &mut state as *mut _ as *mut u8);
        }
        assert!(state.counter > 0);
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.5;
        hands_off_detector(&mut frame, &mut state as *mut _ as *mut u8);
        if state.counter != 0 {
            return Err(format!(
                "counter should reset to 0 on resistance, got {}",
                state.counter
            ));
        }
        Ok(())
    }

    #[test]
    fn test_torque_cap_filter() {
        let mut frame = create_test_frame(1.0, 0.0);
        frame.torque_out = 1.0;
        let max_torque = 0.8f32;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);

        assert!((frame.torque_out - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_torque_cap_negative_clamped() -> Result<(), String> {
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = -1.0;
        let max_torque = 0.5f32;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
        if (frame.torque_out - (-0.5)).abs() >= 0.001 {
            return Err(format!(
                "negative torque should clamp to -0.5, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_torque_cap_within_limit_unchanged() -> Result<(), String> {
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.3;
        let max_torque = 0.8f32;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
        if (frame.torque_out - 0.3).abs() >= 0.001 {
            return Err(format!(
                "torque within limit should be unchanged, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    // ── Filter chain composition ────────────────────────────────────

    #[test]
    fn test_chain_reconstruction_then_slew_rate() -> Result<(), String> {
        let mut recon_state = ReconstructionState::new(4);
        let mut slew_state = SlewRateState::new(0.5);

        let mut frame = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, &mut recon_state as *mut _ as *mut u8);
        let after_recon = frame.torque_out;

        slew_rate_filter(&mut frame, &mut slew_state as *mut _ as *mut u8);
        let after_slew = frame.torque_out;

        // Slew rate should further limit the already-smoothed output
        if after_slew > after_recon + 0.001 {
            return Err(format!(
                "slew should not increase output: recon={}, slew={}",
                after_recon, after_slew
            ));
        }
        Ok(())
    }

    #[test]
    fn test_chain_friction_then_damper_additive() -> Result<(), String> {
        let friction_state = FrictionState::new(0.1, false);
        let damper_state = DamperState::new(0.1, false);

        let mut frame = create_test_frame(0.0, 2.0);
        friction_filter(&mut frame, &friction_state as *const _ as *mut u8);
        let after_friction = frame.torque_out;

        damper_filter(&mut frame, &damper_state as *const _ as *mut u8);
        let after_both = frame.torque_out;

        // Both oppose motion, so combined magnitude should be greater
        if after_both.abs() <= after_friction.abs() {
            return Err(format!(
                "combined should be larger: friction={}, both={}",
                after_friction, after_both
            ));
        }
        Ok(())
    }

    #[test]
    fn test_chain_notch_then_torque_cap() -> Result<(), String> {
        let mut notch_state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let max_torque = 0.5f32;

        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.8;
        notch_filter(&mut frame, &mut notch_state as *mut _ as *mut u8);
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);

        if !frame.torque_out.is_finite() {
            return Err("chain output should be finite".to_string());
        }
        if frame.torque_out.abs() > 0.5 + 0.001 {
            return Err(format!(
                "torque cap should clamp after notch: got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_chain_curve_then_response_curve() -> Result<(), String> {
        let curve_state = CurveState::quadratic();
        let resp_state = ResponseCurveState::soft();

        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.8;
        curve_filter(&mut frame, &curve_state as *const _ as *mut u8);
        let after_curve = frame.torque_out;
        response_curve_filter(&mut frame, &resp_state as *const _ as *mut u8);

        // Both are sub-linear, so output should be reduced
        if frame.torque_out >= 0.8 {
            return Err(format!(
                "dual sub-linear curves should reduce 0.8: curve={}, final={}",
                after_curve, frame.torque_out
            ));
        }
        if frame.torque_out <= 0.0 {
            return Err(format!("output should be positive: {}", frame.torque_out));
        }
        Ok(())
    }

    #[test]
    fn test_full_pipeline_order() -> Result<(), String> {
        // Simulate a realistic pipeline: reconstruction → friction → damper →
        // inertia → notch → slew → curve → torque_cap
        let mut recon = ReconstructionState::new(2);
        let friction = FrictionState::new(0.05, false);
        let damper = DamperState::new(0.05, false);
        let mut inertia = InertiaState::new(0.05);
        let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(1.0);
        let curve = CurveState::linear();
        let max_torque = 0.9f32;

        // Initialize inertia with a prior tick
        let mut init_frame = create_test_frame(0.0, 1.0);
        inertia_filter(&mut init_frame, &mut inertia as *mut _ as *mut u8);

        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 1.5,
            hands_off: false,
            ts_mono_ns: 1000,
            seq: 1,
        };

        reconstruction_filter(&mut frame, &mut recon as *mut _ as *mut u8);
        friction_filter(&mut frame, &friction as *const _ as *mut u8);
        damper_filter(&mut frame, &damper as *const _ as *mut u8);
        inertia_filter(&mut frame, &mut inertia as *mut _ as *mut u8);
        notch_filter(&mut frame, &mut notch as *mut _ as *mut u8);
        slew_rate_filter(&mut frame, &mut slew as *mut _ as *mut u8);
        curve_filter(&mut frame, &curve as *const _ as *mut u8);
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);

        if !frame.torque_out.is_finite() {
            return Err(format!(
                "full pipeline output must be finite, got {}",
                frame.torque_out
            ));
        }
        if frame.torque_out.abs() > 0.9 + 0.001 {
            return Err(format!(
                "full pipeline output must be within cap, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    // ── Edge cases: NaN, zero, extreme values ───────────────────────

    #[test]
    fn test_reconstruction_nan_input() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        // Warm up with valid data first
        for _ in 0..10 {
            let mut f = create_test_frame(0.5, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        // Now inject NaN
        let mut frame = create_test_frame(f32::NAN, 0.0);
        reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);
        // The filter is an EMA: output = prev + alpha*(NaN - prev) = NaN
        // We just verify it doesn't crash; the output may be NaN
        // (torque_cap_filter is the safety net)
        Ok(())
    }

    #[test]
    fn test_notch_nan_input_does_not_crash() -> Result<(), String> {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = f32::NAN;
        notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
        // Should not crash; output may be NaN
        Ok(())
    }

    #[test]
    fn test_slew_rate_nan_input_bounded() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5);
        state.prev_output = 0.5;
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = f32::NAN;
        slew_rate_filter(&mut frame, &mut state as *mut _ as *mut u8);
        // clamp(NaN, -max, max) returns NaN on most platforms; verify no crash
        Ok(())
    }

    #[test]
    fn test_torque_cap_nan_yields_zero() -> Result<(), String> {
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = f32::NAN;
        let max_torque = 0.8f32;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
        // Engine wrapper: NaN → 0.0 (safe state)
        if frame.torque_out != 0.0 {
            return Err(format!(
                "NaN should map to 0.0 (safe state), got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_torque_cap_infinity_yields_zero() -> Result<(), String> {
        for &val in &[f32::INFINITY, f32::NEG_INFINITY] {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = val;
            let max_torque = 0.8f32;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            if frame.torque_out != 0.0 {
                return Err(format!(
                    "{} should map to 0.0 (safe state), got {}",
                    val, frame.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn test_friction_extreme_speed() -> Result<(), String> {
        let state = FrictionState::new(0.1, true);
        let mut frame = create_test_frame(0.0, f32::MAX);
        friction_filter(&mut frame, &state as *const _ as *mut u8);
        if !frame.torque_out.is_finite() {
            return Err(format!(
                "friction with f32::MAX speed should be finite, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_damper_extreme_speed() -> Result<(), String> {
        let state = DamperState::new(0.01, false);
        let mut frame = create_test_frame(0.0, 1000.0);
        damper_filter(&mut frame, &state as *const _ as *mut u8);
        if !frame.torque_out.is_finite() {
            return Err(format!(
                "damper with extreme speed should be finite, got {}",
                frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_inertia_extreme_acceleration() -> Result<(), String> {
        let mut state = InertiaState::new(0.01);
        let mut f0 = create_test_frame(0.0, 0.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);
        let mut f1 = create_test_frame(0.0, 10000.0);
        f1.torque_out = 0.0;
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if !f1.torque_out.is_finite() {
            return Err(format!(
                "inertia with extreme acceleration should be finite, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_notch_zero_coefficients_via_bypass() -> Result<(), String> {
        let mut state = NotchState::bypass();
        // Feed several values through bypass filter
        for &input in &[0.0f32, 0.5, -0.5, 1.0, -1.0] {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
            if (frame.torque_out - input).abs() >= 0.01 {
                return Err(format!(
                    "bypass at input {} should pass through, got {}",
                    input, frame.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn test_curve_filter_extreme_input_clamped() -> Result<(), String> {
        let state = CurveState::linear();
        for &val in &[100.0f32, -100.0, f32::MAX, f32::MIN] {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = val;
            curve_filter(&mut frame, &state as *const _ as *mut u8);
            if !frame.torque_out.is_finite() {
                return Err(format!(
                    "curve with extreme input {} should be finite, got {}",
                    val, frame.torque_out
                ));
            }
            if frame.torque_out.abs() > 1.0 + 0.01 {
                return Err(format!(
                    "curve should clamp extreme input {}: got {}",
                    val, frame.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn test_zero_coefficient_filters_no_effect() -> Result<(), String> {
        // Friction with zero coefficient
        let friction = FrictionState::new(0.0, false);
        let mut f = create_test_frame(0.3, 5.0);
        friction_filter(&mut f, &friction as *const _ as *mut u8);
        if (f.torque_out - 0.3).abs() >= 0.001 {
            return Err(format!(
                "zero friction should not change torque: got {}",
                f.torque_out
            ));
        }

        // Damper with zero coefficient
        let damper = DamperState::new(0.0, false);
        let mut f2 = create_test_frame(0.3, 5.0);
        damper_filter(&mut f2, &damper as *const _ as *mut u8);
        if (f2.torque_out - 0.3).abs() >= 0.001 {
            return Err(format!(
                "zero damper should not change torque: got {}",
                f2.torque_out
            ));
        }

        // Inertia with zero coefficient
        let mut inertia = InertiaState::new(0.0);
        let mut f3 = create_test_frame(0.3, 0.0);
        inertia_filter(&mut f3, &mut inertia as *mut _ as *mut u8);
        let mut f4 = create_test_frame(0.3, 5.0);
        inertia_filter(&mut f4, &mut inertia as *mut _ as *mut u8);
        if (f4.torque_out - 0.3).abs() >= 0.001 {
            return Err(format!(
                "zero inertia should not change torque: got {}",
                f4.torque_out
            ));
        }
        Ok(())
    }

    // ── Frequency response characteristics ──────────────────────────

    #[test]
    fn test_notch_dc_passes_through() -> Result<(), String> {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        // Feed constant DC for many samples to reach steady state
        for _ in 0..500 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.7;
            notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
        }
        let mut final_frame = create_test_frame(0.0, 0.0);
        final_frame.torque_out = 0.7;
        notch_filter(&mut final_frame, &mut state as *mut _ as *mut u8);
        // DC (0 Hz) should pass through a notch at 50 Hz
        if (final_frame.torque_out - 0.7).abs() >= 0.05 {
            return Err(format!(
                "notch should pass DC: expected ~0.7, got {}",
                final_frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_lowpass_passes_dc() -> Result<(), String> {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
        for _ in 0..500 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.6;
            notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
        }
        let mut final_frame = create_test_frame(0.0, 0.0);
        final_frame.torque_out = 0.6;
        notch_filter(&mut final_frame, &mut state as *mut _ as *mut u8);
        if (final_frame.torque_out - 0.6).abs() >= 0.05 {
            return Err(format!(
                "lowpass should pass DC: expected ~0.6, got {}",
                final_frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_reconstruction_acts_as_lowpass() -> Result<(), String> {
        // With heavy smoothing, reconstruction should attenuate rapid changes
        let mut state = ReconstructionState::new(6);
        let mut outputs = Vec::new();
        for i in 0..100 {
            // Alternating +1/-1 (Nyquist frequency)
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut frame = create_test_frame(input, 0.0);
            reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);
            if i > 50 {
                outputs.push(frame.torque_out.abs());
            }
        }
        let max_amplitude: f32 = outputs.iter().copied().fold(0.0f32, f32::max);
        // Heavy smoothing should attenuate Nyquist to well below 1.0
        if max_amplitude >= 0.5 {
            return Err(format!(
                "heavy smoothing should attenuate Nyquist: max_amp={}",
                max_amplitude
            ));
        }
        Ok(())
    }

    // ── Filter state reset behavior ─────────────────────────────────

    #[test]
    fn test_reconstruction_state_reset() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        // Warm up
        for _ in 0..50 {
            let mut f = create_test_frame(1.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        assert!(state.prev_output > 0.5);

        FilterState::reset(&mut state);
        if state.prev_output.abs() >= 0.001 {
            return Err(format!(
                "reset should clear prev_output, got {}",
                state.prev_output
            ));
        }

        // After reset, a step input should produce initial response again
        let mut frame = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);
        if (frame.torque_out - state.alpha).abs() >= 0.001 {
            return Err(format!(
                "after reset, first sample should be alpha={}, got {}",
                state.alpha, frame.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn test_notch_state_reset() -> Result<(), String> {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        // Warm up to populate delay line
        for i in 0..100 {
            let input = (i as f32 * 0.1).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut state as *mut _ as *mut u8);
        }
        assert!(state.x1.abs() > 0.0 || state.y1.abs() > 0.0);

        FilterState::reset(&mut state);
        if state.x1.abs() > 0.0
            || state.x2.abs() > 0.0
            || state.y1.abs() > 0.0
            || state.y2.abs() > 0.0
        {
            return Err(format!(
                "notch reset should clear delay line: x1={}, x2={}, y1={}, y2={}",
                state.x1, state.x2, state.y1, state.y2
            ));
        }
        // Coefficients should be preserved
        if !state.b0.is_finite() || state.b0.abs() < 0.001 {
            return Err(format!(
                "notch reset should preserve coefficients: b0={}",
                state.b0
            ));
        }
        Ok(())
    }

    #[test]
    fn test_slew_rate_state_reset() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5);
        // Warm up
        for _ in 0..100 {
            let mut f = create_test_frame(0.0, 0.0);
            f.torque_out = 1.0;
            slew_rate_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        assert!(state.prev_output > 0.0);

        FilterState::reset(&mut state);
        if state.prev_output.abs() >= 0.001 {
            return Err(format!(
                "slew reset should clear prev_output, got {}",
                state.prev_output
            ));
        }
        // max_change_per_tick should be preserved
        if (state.max_change_per_tick - 0.5 / 1000.0).abs() >= 0.0001 {
            return Err(format!(
                "slew reset should preserve rate, got {}",
                state.max_change_per_tick
            ));
        }
        Ok(())
    }

    #[test]
    fn test_inertia_state_reset() -> Result<(), String> {
        let mut state = InertiaState::new(0.1);
        let mut f = create_test_frame(0.0, 10.0);
        inertia_filter(&mut f, &mut state as *mut _ as *mut u8);
        assert!((state.prev_wheel_speed - 10.0).abs() < 0.001);

        FilterState::reset(&mut state);
        if state.prev_wheel_speed.abs() >= 0.001 {
            return Err(format!(
                "inertia reset should clear prev_wheel_speed, got {}",
                state.prev_wheel_speed
            ));
        }
        if (state.coefficient - 0.1).abs() >= 0.001 {
            return Err(format!(
                "inertia reset should preserve coefficient, got {}",
                state.coefficient
            ));
        }
        Ok(())
    }

    #[test]
    fn test_bumpstop_state_reset() -> Result<(), String> {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state.current_angle = 500.0;

        FilterState::reset(&mut state);
        if state.current_angle.abs() >= 0.001 {
            return Err(format!(
                "bumpstop reset should clear angle, got {}",
                state.current_angle
            ));
        }
        if !state.enabled {
            return Err("bumpstop reset should preserve enabled flag".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_hands_off_state_reset() -> Result<(), String> {
        let mut state = HandsOffState::new(true, 0.05, 2.0);
        state.counter = 500;
        state.last_torque = 0.3;

        FilterState::reset(&mut state);
        if state.counter != 0 {
            return Err(format!(
                "hands_off reset should clear counter, got {}",
                state.counter
            ));
        }
        if state.last_torque.abs() >= 0.001 {
            return Err(format!(
                "hands_off reset should clear last_torque, got {}",
                state.last_torque
            ));
        }
        if !state.enabled {
            return Err("hands_off reset should preserve enabled".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_reset_then_reprocess_matches_fresh_state() -> Result<(), String> {
        // Verify that resetting a used filter gives same results as a fresh one
        let mut used_state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        for i in 0..200 {
            let input = (i as f32 * 0.05).sin();
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            notch_filter(&mut frame, &mut used_state as *mut _ as *mut u8);
        }
        FilterState::reset(&mut used_state);

        let mut fresh_state = NotchState::new(50.0, 2.0, -6.0, 1000.0);

        // Process identical input through both
        let test_inputs = [0.1f32, 0.5, -0.3, 0.8, -0.7];
        for &input in &test_inputs {
            let mut frame_used = create_test_frame(0.0, 0.0);
            frame_used.torque_out = input;
            notch_filter(&mut frame_used, &mut used_state as *mut _ as *mut u8);

            let mut frame_fresh = create_test_frame(0.0, 0.0);
            frame_fresh.torque_out = input;
            notch_filter(&mut frame_fresh, &mut fresh_state as *mut _ as *mut u8);

            if (frame_used.torque_out - frame_fresh.torque_out).abs() >= 1e-6 {
                return Err(format!(
                    "reset state diverged from fresh at input {}: used={}, fresh={}",
                    input, frame_used.torque_out, frame_fresh.torque_out
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // --- torque_cap_filter ---

        #[test]
        fn prop_torque_cap_nan_yields_zero(max_torque in 0.0f32..=100.0) {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = f32::NAN;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            prop_assert_eq!(
                frame.torque_out, 0.0,
                "NaN input must map to 0.0 (safe state)"
            );
        }

        #[test]
        fn prop_torque_cap_pos_inf_yields_zero(max_torque in 0.0f32..=100.0) {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = f32::INFINITY;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            prop_assert_eq!(
                frame.torque_out, 0.0,
                "positive infinity must map to 0.0 (safe state)"
            );
        }

        #[test]
        fn prop_torque_cap_neg_inf_yields_zero(max_torque in 0.0f32..=100.0) {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = f32::NEG_INFINITY;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            prop_assert_eq!(
                frame.torque_out, 0.0,
                "negative infinity must map to 0.0 (safe state)"
            );
        }

        #[test]
        fn prop_torque_cap_output_bounded(
            torque in -100.0f32..=100.0,
            max_torque in 0.0f32..=50.0,
        ) {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = torque;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            prop_assert!(
                frame.torque_out >= -max_torque && frame.torque_out <= max_torque,
                "output {} not in [-{}, {}]", frame.torque_out, max_torque, max_torque
            );
        }

        #[test]
        fn prop_torque_cap_preserves_sign(
            torque in -100.0f32..=100.0,
            max_torque in 0.01f32..=50.0,
        ) {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = torque;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            if torque > 0.0 {
                prop_assert!(
                    frame.torque_out >= 0.0,
                    "positive torque {} became negative {}", torque, frame.torque_out
                );
            } else if torque < 0.0 {
                prop_assert!(
                    frame.torque_out <= 0.0,
                    "negative torque {} became positive {}", torque, frame.torque_out
                );
            }
        }

        #[test]
        fn prop_torque_cap_within_limit_unchanged(
            max_torque in 1.0f32..=50.0,
            fraction in -1.0f32..=1.0,
        ) {
            let torque = fraction * max_torque * 0.99;
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = torque;
            torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
            let diff = (frame.torque_out - torque).abs();
            prop_assert!(
                diff < 0.001,
                "torque within limit should pass through unchanged: in={}, out={}", torque, frame.torque_out
            );
        }
    }
}
