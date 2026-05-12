use openracing_hardware_core::{
    DescriptorTrustEvidence, DeviceCapabilityKind, DeviceCapabilityRegistry, DeviceFamily,
    Disconnected, EnumerationEvidence, EvidenceError, EvidenceSource, FinalZeroPolicy,
    LowTorqueArmEvidence, OutputBarrierDecisionReason, OutputBarrierError, OutputCapabilityStage,
    OutputCommand, OutputWatchdogState, OutputWriteBarrier, PassiveVerificationEvidence,
    VirtualHidDescriptor, VirtualHidError, VirtualHidFaultKind, VirtualHidIdentity,
    VirtualHidReplay, VirtualInputReport, VirtualOutputKind, VirtualWriteResult,
    ZeroOutputEvidence,
};
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, TestCaseError};

const MOZA_VENDOR_ID: u16 = 0x346E;
const MOZA_R5_V1_PID: u16 = 0x0004;
const MOZA_R5_V2_PID: u16 = 0x0014;
const MOZA_SRP_PID: u16 = 0x0003;
const MOZA_HGP_PID: u16 = 0x0020;
const MOZA_SGP_PID: u16 = 0x0021;
const MOZA_HBP_PID: u16 = 0x0022;

fn testcase_error(error: impl ToString) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}

fn virtual_identity(product_id: u16) -> Result<VirtualHidIdentity, VirtualHidError> {
    VirtualHidIdentity::new(MOZA_VENDOR_ID, product_id, "virtual-moza-r5")?
        .with_manufacturer("Virtual Moza")?
        .with_product_name("Virtual R5")
        .map(|identity| identity.with_interface(0).with_usage(0x0001, 0x0004))
}

fn virtual_descriptor(min_input_len: usize) -> Result<VirtualHidDescriptor, VirtualHidError> {
    VirtualHidDescriptor::new("0x12345678")?
        .with_input_report_lengths([min_input_len])
        .map(|descriptor| {
            descriptor
                .with_output_report_ids([0x20])
                .with_feature_report_ids([0x03, 0x11])
        })
}

fn zero_verified() -> Result<openracing_hardware_core::ZeroOutputVerified, EvidenceError> {
    Ok(Disconnected::new()
        .enumerate(EnumerationEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/moza-r5/2026-05-12/device-list.json",
            MOZA_VENDOR_ID,
            MOZA_R5_V2_PID,
            "moza-r5",
        )?)
        .trust_descriptor(DescriptorTrustEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/moza-r5/2026-05-12/descriptor.json",
        )?)
        .verify_passive(PassiveVerificationEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/moza-r5/2026-05-12/passive-verification.json",
        )?)
        .verify_zero_output(ZeroOutputEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/moza-r5/2026-05-12/zero-verification.json",
        )?))
}

fn low_torque_armed() -> Result<openracing_hardware_core::LowTorqueArmed, EvidenceError> {
    Ok(zero_verified()?.arm_low_torque(LowTorqueArmEvidence::new(
        EvidenceSource::RealHardware,
        "ci/hardware/moza-r5/2026-05-12/low-torque-arm.json",
    )?))
}

fn lineage_with_source(
    source: EvidenceSource,
) -> Result<openracing_hardware_core::ValidationLineage, EvidenceError> {
    Ok(Disconnected::new()
        .enumerate(EnumerationEvidence::new(
            source,
            "target/openracing/virtual/device-list.json",
            MOZA_VENDOR_ID,
            MOZA_R5_V2_PID,
            "virtual-moza-r5",
        )?)
        .trust_descriptor(DescriptorTrustEvidence::new(
            source,
            "target/openracing/virtual/descriptor.json",
        )?)
        .verify_passive(PassiveVerificationEvidence::new(
            source,
            "target/openracing/virtual/passive-verification.json",
        )?)
        .lineage()
        .clone())
}

fn is_known_default_device(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == MOZA_VENDOR_ID
        && matches!(
            product_id,
            MOZA_R5_V1_PID
                | MOZA_R5_V2_PID
                | MOZA_SRP_PID
                | MOZA_HGP_PID
                | MOZA_SGP_PID
                | MOZA_HBP_PID
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn virtual_input_replay_preserves_fifo_order_and_fault_classification(
        min_input_len in 1usize..=96,
        reports in proptest::collection::vec((any::<u64>(), proptest::collection::vec(any::<u8>(), 1..=128)), 1..=32),
    ) {
        let identity = virtual_identity(MOZA_R5_V2_PID).map_err(testcase_error)?;
        let descriptor = virtual_descriptor(min_input_len).map_err(testcase_error)?;
        let mut replay = VirtualHidReplay::new(identity, descriptor);

        for (timestamp_us, bytes) in &reports {
            let report = VirtualInputReport::new(*timestamp_us, bytes.clone()).map_err(testcase_error)?;
            replay.queue_input_report(report);
        }

        for (timestamp_us, bytes) in &reports {
            let report = replay.read_input_report().map_err(testcase_error)?;
            prop_assert_eq!(report.timestamp_us(), *timestamp_us);
            prop_assert_eq!(report.bytes(), bytes.as_slice());
        }

        prop_assert_eq!(replay.read_input_report(), Err(VirtualHidError::NoInputReport));
        prop_assert_eq!(replay.input_log().len(), reports.len());

        for (index, entry) in replay.input_log().iter().enumerate() {
            let event_index = u64::try_from(index).map_err(testcase_error)? + 1;
            prop_assert_eq!(entry.event_index(), event_index);
            prop_assert_eq!(entry.short_report(), reports[index].1.len() < min_input_len);
            let duplicate_timestamp = index > 0 && reports[index - 1].0 == reports[index].0;
            prop_assert_eq!(entry.duplicate_timestamp(), duplicate_timestamp);
        }

        let expected_short_reports = reports
            .iter()
            .filter(|(_, bytes)| bytes.len() < min_input_len)
            .count();
        let expected_duplicate_timestamps = reports
            .windows(2)
            .filter(|window| window[0].0 == window[1].0)
            .count();
        let short_report_faults = replay
            .fault_events()
            .iter()
            .filter(|fault| fault.kind() == VirtualHidFaultKind::ShortInputReport)
            .count();
        let duplicate_timestamp_faults = replay
            .fault_events()
            .iter()
            .filter(|fault| fault.kind() == VirtualHidFaultKind::DuplicateTimestamp)
            .count();

        prop_assert_eq!(short_report_faults, expected_short_reports);
        prop_assert_eq!(duplicate_timestamp_faults, expected_duplicate_timestamps);

        let receipt = replay.receipt();
        prop_assert_eq!(receipt.hardware_source(), EvidenceSource::Virtual);
        prop_assert!(!receipt.real_hardware_validated());
        prop_assert_eq!(receipt.input_log().len(), reports.len());
    }

    #[test]
    fn virtual_writes_log_every_attempt_without_real_hardware_claims(
        kind_is_feature in any::<bool>(),
        timestamp_us in any::<u64>(),
        mode in 0u8..=2,
        timeout_ms in 1u64..=10_000,
        bytes in proptest::collection::vec(any::<u8>(), 1..=128),
    ) {
        let identity = virtual_identity(MOZA_R5_V2_PID).map_err(testcase_error)?;
        let descriptor = virtual_descriptor(1).map_err(testcase_error)?;
        let mut replay = VirtualHidReplay::new(identity, descriptor);
        replay.set_timestamp_us(timestamp_us);

        match mode {
            1 => replay.disconnect(),
            2 => replay.expire_watchdog(timeout_ms),
            _ => {}
        }

        let result = if kind_is_feature {
            replay.write_feature_report(&bytes)
        } else {
            replay.write_output_report(&bytes)
        };

        match mode {
            0 => prop_assert_eq!(result, Ok(bytes.len())),
            1 => prop_assert_eq!(result, Err(VirtualHidError::Disconnected)),
            _ => prop_assert_eq!(result, Err(VirtualHidError::WatchdogExpired { timeout_ms })),
        }

        prop_assert_eq!(replay.output_log().len(), 1);
        let Some(entry) = replay.output_log().first() else {
            prop_assert!(false, "missing virtual output log entry");
            unreachable!();
        };
        prop_assert_eq!(entry.timestamp_us(), timestamp_us);
        prop_assert_eq!(entry.bytes(), bytes.as_slice());
        prop_assert_eq!(
            entry.kind(),
            if kind_is_feature {
                VirtualOutputKind::Feature
            } else {
                VirtualOutputKind::Output
            }
        );

        match mode {
            0 => prop_assert_eq!(entry.result(), &VirtualWriteResult::Written { bytes_written: bytes.len() }),
            1 => prop_assert_eq!(entry.result(), &VirtualWriteResult::Disconnected),
            _ => prop_assert_eq!(entry.result(), &VirtualWriteResult::WatchdogExpired { timeout_ms }),
        }

        let receipt = replay.receipt();
        prop_assert_eq!(receipt.hardware_source(), EvidenceSource::Virtual);
        prop_assert!(!receipt.real_hardware_validated());
        prop_assert_eq!(receipt.output_log().len(), 1);
    }

    #[test]
    fn output_barrier_rejects_all_non_zero_commands_without_capability(
        signed_percent in (-100.0f32..=100.0f32).prop_filter("non-zero command", |value| value.abs() > f32::EPSILON),
    ) {
        let capability = zero_verified().map_err(testcase_error)?.zero_output_capability();
        let watchdog = OutputWatchdogState::active(100).map_err(testcase_error)?;
        let mut barrier = OutputWriteBarrier::new(capability, watchdog, FinalZeroPolicy::required());
        let command = OutputCommand::new(signed_percent).map_err(testcase_error)?;

        let result = barrier.evaluate(command);

        prop_assert_eq!(
            result,
            Err(OutputBarrierError::NonZeroWithoutCapability {
                stage: OutputCapabilityStage::ZeroOnly,
                signed_percent,
            })
        );
        prop_assert!(!barrier.final_zero_pending());
        prop_assert_eq!(barrier.events().len(), 1);
    }

    #[test]
    fn output_barrier_acceptance_matches_capability_limit_and_final_zero_policy(
        max_percent in 0.1f32..=100.0f32,
        signed_percent in -100.0f32..=100.0f32,
    ) {
        let capability = low_torque_armed()
            .map_err(testcase_error)?
            .low_torque_output_capability(max_percent)
            .map_err(testcase_error)?;
        let watchdog = OutputWatchdogState::active(100).map_err(testcase_error)?;
        let mut barrier = OutputWriteBarrier::new(capability, watchdog, FinalZeroPolicy::required());
        let command = OutputCommand::new(signed_percent).map_err(testcase_error)?;

        let result = barrier.evaluate(command);

        if command.is_zero() {
            let decision = result.map_err(testcase_error)?;
            prop_assert_eq!(decision.reason(), OutputBarrierDecisionReason::ZeroAccepted);
            prop_assert!(!decision.final_zero_required_after_write());
            prop_assert!(!barrier.final_zero_pending());
        } else if signed_percent.abs() <= max_percent {
            let decision = result.map_err(testcase_error)?;
            prop_assert_eq!(decision.reason(), OutputBarrierDecisionReason::NonZeroAccepted);
            prop_assert!(decision.final_zero_required_after_write());
            prop_assert!(barrier.final_zero_pending());
            let Some(final_zero) = barrier.final_zero_command() else {
                prop_assert!(false, "expected final zero command after accepted non-zero output");
                unreachable!();
            };
            prop_assert!(final_zero.is_zero());
            prop_assert!(!barrier.final_zero_pending());
        } else {
            prop_assert_eq!(
                result,
                Err(OutputBarrierError::OutputExceedsCapability {
                    requested_percent: signed_percent.abs(),
                    max_percent,
                })
            );
            prop_assert!(!barrier.final_zero_pending());
        }
    }

    #[test]
    fn virtual_and_synthetic_lineage_never_validate_capability_records(use_virtual in any::<bool>()) {
        let source = if use_virtual {
            EvidenceSource::Virtual
        } else {
            EvidenceSource::Synthetic
        };
        let lineage = lineage_with_source(source).map_err(testcase_error)?;
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry.lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID);

        let result = record.with_validated_lineage(&lineage);

        prop_assert_eq!(
            result,
            Err(openracing_hardware_core::CapabilityRegistryError::NonRealHardwareEvidence {
                evidence_source: source,
            })
        );
    }

    #[test]
    fn unknown_devices_default_to_passive_only(vendor_id in any::<u16>(), product_id in any::<u16>()) {
        prop_assume!(!is_known_default_device(vendor_id, product_id));
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry.lookup(vendor_id, product_id);

        prop_assert_eq!(record.vendor_id(), vendor_id);
        prop_assert_eq!(record.product_id(), product_id);
        prop_assert_eq!(record.family(), DeviceFamily::Unknown);
        prop_assert_eq!(record.kind(), DeviceCapabilityKind::Unknown);
        prop_assert!(record.input());
        prop_assert!(!record.ffb_output());
        prop_assert!(!record.serial_config());
        prop_assert!(!record.firmware_dfu());
        prop_assert!(!record.high_torque());
        prop_assert!(record.validated_stages().is_empty());
    }

    #[test]
    fn real_lineage_validates_only_prefix_stages(stage_index in 1usize..=3) {
        let base = Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-12/device-list.json",
                MOZA_VENDOR_ID,
                MOZA_R5_V2_PID,
                "moza-r5",
            ).map_err(testcase_error)?);

        let lineage = match stage_index {
            1 => base.lineage().clone(),
            2 => base
                .trust_descriptor(DescriptorTrustEvidence::new(
                    EvidenceSource::RealHardware,
                    "ci/hardware/moza-r5/2026-05-12/descriptor.json",
                ).map_err(testcase_error)?)
                .lineage()
                .clone(),
            _ => base
                .trust_descriptor(DescriptorTrustEvidence::new(
                    EvidenceSource::RealHardware,
                    "ci/hardware/moza-r5/2026-05-12/descriptor.json",
                ).map_err(testcase_error)?)
                .verify_passive(PassiveVerificationEvidence::new(
                    EvidenceSource::RealHardware,
                    "ci/hardware/moza-r5/2026-05-12/passive-verification.json",
                ).map_err(testcase_error)?)
                .lineage()
                .clone(),
        };

        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry
            .lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID)
            .with_validated_lineage(&lineage)
            .map_err(testcase_error)?;

        prop_assert_eq!(record.validated_stages().last().copied(), Some(lineage.stage()));
        prop_assert_eq!(record.validated_stages().len(), stage_index);
    }
}
