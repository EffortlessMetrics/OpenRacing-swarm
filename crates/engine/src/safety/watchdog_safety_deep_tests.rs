//! Deep watchdog and safety interlock tests
//!
//! Covers: concurrent SharedWatchdog access, watchdog behavior during recovery,
//! rapid state oscillation, simultaneous faults, timing guarantees,
//! challenge-response edge cases, and property-based state machine invariants.
//!
//! Every test returns `Result` and avoids `unwrap()`/`expect()` per project policy.

use super::hardware_watchdog::{
    HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SafetyTrigger, SoftwareWatchdog,
    TorqueLimit, WatchdogError, WatchdogTimeoutHandler,
};
use super::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

// =========================================================================
// Helpers
// =========================================================================

fn svc() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

fn activate(service: &mut SafetyService, device: &str) -> Result<(), String> {
    let challenge = service.request_high_torque(device)?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.report_combo_start(challenge.challenge_token)?;
    std::thread::sleep(Duration::from_millis(2100));
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    service.confirm_high_torque(device, ack)
}

fn interlock_system(timeout_ms: u32, max_torque: f32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(watchdog, max_torque)
}

// =========================================================================
// 1. Concurrent SharedWatchdog access
// =========================================================================

#[test]
fn test_shared_watchdog_concurrent_feed() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(200));
    let shared = SharedWatchdog::new(watchdog);
    shared.arm()?;

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let w = shared.clone();
            std::thread::spawn(move || {
                for _ in 0..50 {
                    let _ = w.feed();
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
        })
        .collect();

    for h in handles {
        h.join()
            .map_err(|_| WatchdogError::HardwareError("thread panic".into()))?;
    }

    assert!(!shared.has_timed_out());
    Ok(())
}

#[test]
fn test_shared_watchdog_concurrent_arm_disarm() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(500));
    let shared = SharedWatchdog::new(watchdog);

    // Arm then have multiple threads try operations concurrently
    shared.arm()?;

    let s1 = shared.clone();
    let s2 = shared.clone();
    let h1 = std::thread::spawn(move || {
        for _ in 0..20 {
            let _ = s1.feed();
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let h2 = std::thread::spawn(move || {
        for _ in 0..20 {
            let _ = s2.has_timed_out();
            std::thread::sleep(Duration::from_millis(1));
        }
    });

    h1.join()
        .map_err(|_| WatchdogError::HardwareError("thread panic".into()))?;
    h2.join()
        .map_err(|_| WatchdogError::HardwareError("thread panic".into()))?;

    // After concurrent access, state should be consistent
    assert!(shared.is_armed());
    Ok(())
}

#[test]
fn test_shared_watchdog_timeout_visible_across_threads() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(20));
    let shared = SharedWatchdog::new(watchdog);
    shared.arm()?;
    shared.feed()?;

    // Let it time out
    std::thread::sleep(Duration::from_millis(30));

    let s = shared.clone();
    let result = std::thread::spawn(move || s.has_timed_out())
        .join()
        .map_err(|_| WatchdogError::HardwareError("thread panic".into()))?;

    assert!(result, "Timeout should be visible from another thread");
    Ok(())
}

// =========================================================================
// 2. Watchdog behavior during recovery sequences
// =========================================================================

#[test]
fn test_watchdog_timeout_during_fault_recovery() -> Result<(), WatchdogError> {
    let mut system = interlock_system(30, 25.0);
    system.arm()?;

    // Enter safe mode via fault
    system.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        system.state(),
        SafetyInterlockState::SafeMode { .. }
    ));

    // Let watchdog time out while in safe mode
    std::thread::sleep(Duration::from_millis(40));

    // Process tick — watchdog timeout should keep torque at zero
    let result = system.process_tick(10.0);
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);

    Ok(())
}

#[test]
fn test_reset_clears_watchdog_and_restores_normal_after_timeout() -> Result<(), WatchdogError> {
    let mut system = interlock_system(15, 25.0);
    system.arm()?;
    let _ = system.process_tick(10.0);

    std::thread::sleep(Duration::from_millis(20));
    let result = system.process_tick(10.0);
    assert_eq!(result.torque_command, 0.0);

    // Full reset
    system.reset()?;
    assert_eq!(*system.state(), SafetyInterlockState::Normal);

    // Re-arm and verify normal operation
    system.arm()?;
    let result = system.process_tick(12.0);
    assert_eq!(result.torque_command, 12.0);
    assert!(!result.fault_occurred);
    Ok(())
}

#[test]
fn test_watchdog_feed_restarts_timer_preventing_timeout() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(50);
    watchdog.arm()?;

    // Feed at 30ms intervals — always within the 50ms window
    for _ in 0..6 {
        watchdog.feed()?;
        std::thread::sleep(Duration::from_millis(30));
    }

    assert!(
        !watchdog.has_timed_out(),
        "Feeding within window must prevent timeout"
    );
    Ok(())
}

// =========================================================================
// 3. Rapid state oscillation
// =========================================================================

#[test]
fn test_rapid_fault_clear_cycles_stay_consistent() -> Result<(), String> {
    let mut service = svc();

    for i in 0..10 {
        service.report_fault(FaultType::UsbStall);
        assert_eq!(
            service.clamp_torque_nm(5.0),
            0.0,
            "Cycle {i}: torque must be zero while faulted"
        );

        std::thread::sleep(Duration::from_millis(110));
        service.clear_fault()?;
        assert_eq!(
            service.state(),
            &SafetyState::SafeTorque,
            "Cycle {i}: must return to SafeTorque after clear"
        );
    }
    Ok(())
}

#[test]
fn test_rapid_interlock_cancel_cycles() -> Result<(), String> {
    let mut service = svc();

    for _ in 0..5 {
        let _challenge = service.request_high_torque("dev")?;
        assert!(matches!(
            service.state(),
            SafetyState::HighTorqueChallenge { .. }
        ));
        service.cancel_challenge()?;
        assert_eq!(service.state(), &SafetyState::SafeTorque);
    }
    Ok(())
}

#[test]
fn test_rapid_emergency_stop_reset_cycles() -> Result<(), WatchdogError> {
    let mut system = interlock_system(500, 25.0);

    for _ in 0..5 {
        system.arm()?;
        let result = system.emergency_stop();
        assert_eq!(result.torque_command, 0.0);
        assert!(matches!(
            system.state(),
            SafetyInterlockState::EmergencyStop { .. }
        ));
        system.reset()?;
        assert_eq!(*system.state(), SafetyInterlockState::Normal);
    }
    Ok(())
}

// =========================================================================
// 4. Simultaneous / cascading faults
// =========================================================================

#[test]
fn test_simultaneous_faults_last_wins_state() -> Result<(), String> {
    let mut service = svc();

    service.report_fault(FaultType::EncoderNaN);
    service.report_fault(FaultType::ThermalLimit);
    service.report_fault(FaultType::Overcurrent);

    // State reflects the last reported fault
    match service.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::Overcurrent);
        }
        other => return Err(format!("Expected Faulted, got {:?}", other)),
    }

    // Torque is zero regardless
    assert_eq!(service.clamp_torque_nm(25.0), 0.0);
    Ok(())
}

#[test]
fn test_fault_during_challenge_aborts_challenge() -> Result<(), String> {
    let mut service = svc();
    let _challenge = service.request_high_torque("dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // Fault arrives during challenge
    service.report_fault(FaultType::UsbStall);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Challenge is effectively abandoned; torque is zero
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    Ok(())
}

#[test]
fn test_fault_during_awaiting_ack_aborts_sequence() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;

    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));

    service.report_fault(FaultType::Overcurrent);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    Ok(())
}

#[test]
fn test_interlock_system_multiple_faults_logged() -> Result<(), WatchdogError> {
    let mut system = interlock_system(500, 25.0);
    system.arm()?;

    system.report_fault(FaultType::EncoderNaN);
    system.report_fault(FaultType::Overcurrent);
    system.report_fault(FaultType::PipelineFault);

    let log = system.fault_log();
    assert!(log.len() >= 3, "All faults must be logged");
    Ok(())
}

// =========================================================================
// 5. Timing guarantees
// =========================================================================

#[test]
fn test_fault_detection_to_zero_torque_under_10ms() -> Result<(), String> {
    let mut service = svc();
    activate(&mut service, "dev")?;

    let before = Instant::now();
    service.report_fault(FaultType::TimingViolation);
    let torque = service.clamp_torque_nm(25.0);
    let elapsed = before.elapsed();

    assert_eq!(torque, 0.0);
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault-to-zero took {:?}, budget is 10ms",
        elapsed
    );
    Ok(())
}

#[test]
fn test_interlock_system_timeout_response_under_1ms() -> Result<(), WatchdogError> {
    let mut system = interlock_system(15, 25.0);
    system.arm()?;
    let _ = system.process_tick(10.0);

    std::thread::sleep(Duration::from_millis(20));

    let result = system.process_tick(10.0);
    assert_eq!(result.torque_command, 0.0);
    assert!(
        result.response_time < Duration::from_millis(1),
        "Timeout response {:?} exceeds 1ms budget",
        result.response_time
    );
    Ok(())
}

#[test]
fn test_emergency_stop_response_under_1ms() -> Result<(), WatchdogError> {
    let mut system = interlock_system(500, 25.0);
    system.arm()?;

    let before = Instant::now();
    let result = system.emergency_stop();
    let elapsed = before.elapsed();

    assert_eq!(result.torque_command, 0.0);
    assert!(
        elapsed < Duration::from_millis(1),
        "Emergency stop took {:?}, budget is 1ms",
        elapsed
    );
    Ok(())
}

#[test]
fn test_process_tick_latency_under_1ms_normal() -> Result<(), WatchdogError> {
    let mut system = interlock_system(500, 25.0);
    system.arm()?;

    for _ in 0..100 {
        let result = system.process_tick(10.0);
        assert!(
            result.response_time < Duration::from_millis(1),
            "Normal tick {:?} exceeds 1ms",
            result.response_time
        );
    }
    Ok(())
}

// =========================================================================
// 6. Challenge-response interlock edge cases
// =========================================================================

#[test]
fn test_wrong_token_ui_consent_rejected() -> Result<(), String> {
    let mut service = svc();
    let _challenge = service.request_high_torque("dev")?;
    let result = service.provide_ui_consent(0xDEADBEEF);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_wrong_token_combo_start_rejected() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;

    let result = service.report_combo_start(0xDEADBEEF);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_wrong_token_confirm_rejected() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.report_combo_start(challenge.challenge_token)?;
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: 0xDEADBEEF,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev", ack);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_confirm_without_combo_start_rejected() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;

    // Skip report_combo_start — go straight to confirm
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev", ack);
    assert!(
        result.is_err(),
        "Confirm without combo start should be rejected"
    );
    Ok(())
}

#[test]
fn test_confirm_with_short_hold_rejected() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.report_combo_start(challenge.challenge_token)?;

    // Don't wait — confirm immediately (hold too short)
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev", ack);
    assert!(result.is_err(), "Short hold should be rejected");
    Ok(())
}

#[test]
fn test_disable_high_torque_removes_device_token() -> Result<(), String> {
    let mut service = svc();
    activate(&mut service, "dev")?;
    assert!(service.has_valid_token("dev"));

    service.disable_high_torque("dev")?;
    assert!(!service.has_valid_token("dev"));
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn test_disable_high_torque_when_not_active_fails() -> Result<(), String> {
    let mut service = svc();
    let result = service.disable_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_cancel_challenge_from_safe_torque_fails() -> Result<(), String> {
    let mut service = svc();
    let result = service.cancel_challenge();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_challenge_expiry_returns_to_safe_torque() -> Result<(), String> {
    let mut service =
        SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2));

    // Manually set an already-expired challenge state
    service.state = SafetyState::HighTorqueChallenge {
        challenge_token: 123,
        expires: Instant::now() - Duration::from_secs(1),
        ui_consent_given: false,
    };

    let expired = service.check_challenge_expiry();
    assert!(expired);
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn test_get_challenge_time_remaining_none_in_safe_torque() -> Result<(), String> {
    let service = svc();
    assert!(service.get_challenge_time_remaining().is_none());
    Ok(())
}

#[test]
fn test_get_challenge_time_remaining_some_during_challenge() -> Result<(), String> {
    let mut service = svc();
    let _challenge = service.request_high_torque("dev")?;
    let remaining = service.get_challenge_time_remaining();
    assert!(remaining.is_some());

    // Should be close to 30 seconds (the challenge timeout)
    if let Some(dur) = remaining {
        assert!(dur > Duration::from_secs(20));
    }
    Ok(())
}

// =========================================================================
// 7. Edge cases: watchdog state after various transitions
// =========================================================================

#[test]
fn test_double_arm_returns_error() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(100);
    watchdog.arm()?;
    let result = watchdog.arm();
    assert_eq!(result, Err(WatchdogError::AlreadyArmed));
    Ok(())
}

#[test]
fn test_double_disarm_returns_error() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(100);
    let result = watchdog.disarm();
    assert_eq!(result, Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn test_feed_unarmed_returns_not_armed() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(100);
    let result = watchdog.feed();
    assert_eq!(result, Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn test_interlock_system_unarmed_watchdog_allows_limited_torque() -> Result<(), WatchdogError> {
    let mut system = interlock_system(100, 25.0);
    // Do NOT arm

    let result = system.process_tick(10.0);
    // Unarmed watchdog: feed() returns NotArmed, handler returns limited torque
    assert!(result.torque_command.abs() <= 25.0);
    assert!(!result.fault_occurred);
    Ok(())
}

#[test]
fn test_watchdog_time_since_last_feed_increases() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(500);
    watchdog.arm()?;
    watchdog.feed()?;

    let t1 = watchdog.time_since_last_feed();
    std::thread::sleep(Duration::from_millis(20));
    let t2 = watchdog.time_since_last_feed();

    assert!(t2 > t1, "Time since last feed should increase");
    Ok(())
}

#[test]
fn test_watchdog_default_timeout_100ms() -> Result<(), WatchdogError> {
    let watchdog = SoftwareWatchdog::with_default_timeout();
    assert_eq!(watchdog.timeout_ms(), 100);
    Ok(())
}

// =========================================================================
// 8. Safety interlock system: communication loss edge cases
// =========================================================================

#[test]
fn test_no_communication_reported_does_not_trigger_loss() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(500));
    let torque_limit = TorqueLimit::new(25.0, 5.0);
    let mut system =
        SafetyInterlockSystem::with_config(watchdog, torque_limit, Duration::from_millis(50));
    system.arm()?;

    // No report_communication() called — last_communication is None
    let result = system.process_tick(10.0);
    // check_communication_loss returns false when last_communication is None
    assert!(!result.fault_occurred);
    Ok(())
}

#[test]
fn test_communication_refresh_prevents_loss() -> Result<(), WatchdogError> {
    let communication_timeout = Duration::from_millis(50);
    let watchdog = Box::new(SoftwareWatchdog::new(30_000));
    let torque_limit = TorqueLimit::new(25.0, 5.0);
    let mut system =
        SafetyInterlockSystem::with_config(watchdog, torque_limit, communication_timeout);
    system.arm()?;

    system.report_communication();
    system.age_last_communication(communication_timeout + Duration::from_millis(1));
    system.report_communication();
    let result = system.process_tick(10.0);
    assert!(
        !result.fault_occurred,
        "A fresh communication report must refresh the deadline"
    );

    system.age_last_communication(communication_timeout + Duration::from_millis(1));
    let result = system.process_tick(10.0);
    assert!(
        result.fault_occurred,
        "An expired communication timeout must fault"
    );
    Ok(())
}

// =========================================================================
// 9. Torque limit edge cases
// =========================================================================

#[test]
fn test_torque_limit_zero_max() -> Result<(), String> {
    let mut limit = TorqueLimit::new(0.0, 0.0);
    let (clamped, was_clamped) = limit.clamp(1.0);
    assert_eq!(clamped, 0.0);
    assert!(was_clamped);
    Ok(())
}

#[test]
fn test_torque_limit_safe_mode_limit_accessor() -> Result<(), String> {
    let limit = TorqueLimit::new(25.0, 5.0);
    assert_eq!(limit.safe_mode_limit(), 5.0);
    Ok(())
}

#[test]
fn test_interlock_warning_state_uses_safe_limit() -> Result<(), WatchdogError> {
    let mut system = interlock_system(500, 25.0);
    system.arm()?;

    // Manually transition to warning state is not directly exposed,
    // but safe mode uses safe_mode_limit
    system.report_fault(FaultType::PluginOverrun);
    let safe_limit = system.torque_limit().safe_mode_limit();

    let result = system.process_tick(100.0);
    assert!(
        result.torque_command <= safe_limit,
        "Safe mode torque {} exceeds limit {}",
        result.torque_command,
        safe_limit
    );
    Ok(())
}

// =========================================================================
// 10. WatchdogTimeoutHandler edge cases
// =========================================================================

#[test]
fn test_timeout_handler_double_timeout() -> Result<(), String> {
    let mut handler = WatchdogTimeoutHandler::new();

    let r1 = handler.handle_timeout(15.0);
    assert_eq!(r1.torque_command, 0.0);
    assert_eq!(r1.previous_torque, 15.0);

    // Second timeout call — should still produce zero torque
    let r2 = handler.handle_timeout(20.0);
    assert_eq!(r2.torque_command, 0.0);
    assert_eq!(r2.previous_torque, 20.0);
    assert!(handler.is_timeout_triggered());
    Ok(())
}

#[test]
fn test_timeout_handler_reset_then_reuse() -> Result<(), String> {
    let mut handler = WatchdogTimeoutHandler::new();
    handler.handle_timeout(10.0);

    handler.reset();
    assert!(!handler.is_timeout_triggered());
    assert!(handler.timeout_timestamp().is_none());

    // Reuse after reset
    let response = handler.handle_timeout(5.0);
    assert_eq!(response.torque_command, 0.0);
    assert!(handler.is_timeout_triggered());
    Ok(())
}

// =========================================================================
// 11. Safety state machine: consent and challenge invariants
// =========================================================================

#[test]
fn test_consent_requirements_populated() -> Result<(), String> {
    let service = svc();
    let reqs = service.get_consent_requirements();
    assert!(reqs.requires_explicit_consent);
    assert!(!reqs.warnings.is_empty());
    assert!(!reqs.disclaimers.is_empty());
    assert_eq!(reqs.max_torque_nm, 25.0);
    Ok(())
}

#[test]
fn test_max_torque_during_challenge_is_safe_torque() -> Result<(), String> {
    let mut service = svc();
    let _challenge = service.request_high_torque("dev")?;

    // During challenge, max torque should be the safe limit
    assert_eq!(service.max_torque_nm(), 5.0);
    assert_eq!(service.clamp_torque_nm(10.0), 5.0);
    Ok(())
}

#[test]
fn test_max_torque_during_awaiting_ack_is_safe_torque() -> Result<(), String> {
    let mut service = svc();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;

    assert_eq!(service.max_torque_nm(), 5.0);
    Ok(())
}

#[test]
fn test_max_torque_in_high_torque_active() -> Result<(), String> {
    let mut service = svc();
    activate(&mut service, "dev")?;
    assert_eq!(service.max_torque_nm(), 25.0);
    Ok(())
}

#[test]
fn test_hands_on_update_in_safe_torque_is_noop() -> Result<(), String> {
    let mut service = svc();
    // Updating hands-on status outside HighTorqueActive is a no-op
    service.update_hands_on_status(false)?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn test_hands_off_timeout_triggers_fault_in_high_torque() -> Result<(), String> {
    let mut service = SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_millis(100), // Short hands-off timeout for test
        Duration::from_secs(2),
    );
    activate(&mut service, "dev")?;

    // Simulate hands off for longer than timeout
    std::thread::sleep(Duration::from_millis(120));
    let result = service.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn test_hands_on_resets_timeout_in_high_torque() -> Result<(), String> {
    let mut service = SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_millis(200),
        Duration::from_secs(2),
    );
    activate(&mut service, "dev")?;

    // Report hands-on periodically, staying within the timeout
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(50));
        service.update_hands_on_status(true)?;
    }

    // Should still be in HighTorqueActive
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

// =========================================================================
// 12. Property-based tests: state machine invariants
// =========================================================================

fn fault_type_strategy() -> impl Strategy<Value = FaultType> {
    prop_oneof![
        Just(FaultType::UsbStall),
        Just(FaultType::EncoderNaN),
        Just(FaultType::ThermalLimit),
        Just(FaultType::Overcurrent),
        Just(FaultType::PluginOverrun),
        Just(FaultType::TimingViolation),
        Just(FaultType::SafetyInterlockViolation),
        Just(FaultType::HandsOffTimeout),
        Just(FaultType::PipelineFault),
    ]
}

fn safe_torque_strategy() -> impl Strategy<Value = f32> {
    1.0f32..=20.0
}

fn high_torque_strategy() -> impl Strategy<Value = f32> {
    20.0f32..=100.0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // Invariant: any fault immediately transitions to Faulted with zero torque
    #[test]
    fn prop_any_fault_yields_zero_torque(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        fault in fault_type_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(fault);
        let clamped = service.clamp_torque_nm(requested);
        prop_assert_eq!(clamped, 0.0, "Faulted state must clamp to 0.0");
    }

    // Invariant: clear_fault requires minimum duration
    #[test]
    fn prop_early_clear_fault_rejected(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        fault in fault_type_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(fault);
        // Immediately try to clear — must fail
        let result = service.clear_fault();
        prop_assert!(result.is_err(), "Early clear must be rejected");
    }

    // Invariant: after clear_fault, state is SafeTorque
    #[test]
    fn prop_clear_fault_restores_safe_torque(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        fault in fault_type_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(fault);
        std::thread::sleep(Duration::from_millis(110));
        let result = service.clear_fault();
        prop_assert!(result.is_ok(), "Clear should succeed after delay");
        prop_assert_eq!(service.state(), &SafetyState::SafeTorque);
    }

    // Invariant: faulted state blocks high-torque request
    #[test]
    fn prop_faulted_blocks_high_torque(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        fault in fault_type_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(fault);
        let result = service.request_high_torque("dev");
        prop_assert!(result.is_err());
    }

    // Invariant: watchdog timeout always produces zero torque and fault
    #[test]
    fn prop_watchdog_timeout_zero_torque_and_fault(
        timeout_ms in 10u32..100u32,
        requested in 0.0f32..100.0,
    ) {
        let mut system = interlock_system(timeout_ms, 25.0);
        system.arm().map_err(|e| TestCaseError::fail(format!("{:?}", e)))?;
        let _ = system.process_tick(requested);

        std::thread::sleep(Duration::from_millis((timeout_ms + 10) as u64));

        let result = system.process_tick(requested);
        prop_assert_eq!(result.torque_command, 0.0);
        prop_assert!(result.fault_occurred);
        let is_safe_mode = matches!(
            result.state,
            SafetyInterlockState::SafeMode {
                triggered_by: SafetyTrigger::WatchdogTimeout,
                ..
            }
        );
        prop_assert!(is_safe_mode, "Expected SafeMode with WatchdogTimeout");
    }

    // Invariant: torque never exceeds configured max in normal operation
    #[test]
    fn prop_torque_bounded_in_normal(
        max_torque in 5.0f32..50.0,
        requested in -200.0f32..200.0,
    ) {
        let mut system = interlock_system(500, max_torque);
        system.arm().map_err(|e| TestCaseError::fail(format!("{:?}", e)))?;
        let result = system.process_tick(requested);
        prop_assert!(
            result.torque_command.abs() <= max_torque + f32::EPSILON,
            "Torque {} exceeded max {}",
            result.torque_command,
            max_torque
        );
    }

    // Invariant: emergency stop always produces zero
    #[test]
    fn prop_emergency_stop_always_zero(
        max_torque in 5.0f32..50.0,
        requested in -200.0f32..200.0,
    ) {
        let mut system = interlock_system(500, max_torque);
        system.arm().map_err(|e| TestCaseError::fail(format!("{:?}", e)))?;
        system.emergency_stop();
        let result = system.process_tick(requested);
        prop_assert_eq!(result.torque_command, 0.0);
    }

    // Invariant: torque clamping is idempotent in SafetyService
    #[test]
    fn prop_safety_service_clamp_idempotent(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let service = SafetyService::new(safe, high);
        let once = service.clamp_torque_nm(requested);
        let twice = service.clamp_torque_nm(once);
        prop_assert!(
            (once - twice).abs() < f32::EPSILON,
            "Clamping must be idempotent: {} vs {}",
            once,
            twice
        );
    }

    // Invariant: cancel_challenge from non-challenge state fails
    #[test]
    fn prop_cancel_challenge_from_safe_torque_fails(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        let result = service.cancel_challenge();
        prop_assert!(result.is_err());
    }
}
