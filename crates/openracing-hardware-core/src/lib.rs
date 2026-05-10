//! Hardware validation state and evidence primitives.
//!
//! This crate contains hardware-family-neutral validation rails.  It does not
//! open devices, parse vendor reports, or send force-feedback output.  The goal
//! is to make evidence ordering explicit so higher layers cannot arm output
//! stages from loose booleans or partially populated receipt JSON.

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

use serde::{Deserialize, Serialize};

pub mod virtual_hid;

pub use virtual_hid::{
    VirtualHidDescriptor, VirtualHidError, VirtualHidFaultEvent, VirtualHidFaultKind,
    VirtualHidIdentity, VirtualHidReplay, VirtualHidReplayReceipt, VirtualInputLogEntry,
    VirtualInputReport, VirtualOutputKind, VirtualOutputLogEntry, VirtualWriteResult,
};

/// Ordered stages for receipt-backed hardware validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareValidationStage {
    /// No device has been observed for the lane.
    Disconnected,
    /// Device identity/enumeration evidence exists.
    Enumerated,
    /// Descriptor/signature evidence has been trusted for this lane.
    DescriptorTrusted,
    /// Passive input captures have replayed through parsers.
    PassiveVerified,
    /// Zero-output proof has completed.
    ZeroOutputVerified,
    /// Low torque has been armed from zero-output evidence.
    LowTorqueArmed,
    /// Bounded low torque proof has completed and returned to zero.
    LowTorqueVerified,
    /// Simulator smoke has been armed from telemetry and low-torque evidence.
    SimulatorSmokeArmed,
    /// Hardware smoke evidence is complete for the lane.
    SmokeReady,
}

impl HardwareValidationStage {
    /// All validation stages in order.
    pub const ALL: [Self; 9] = [
        Self::Disconnected,
        Self::Enumerated,
        Self::DescriptorTrusted,
        Self::PassiveVerified,
        Self::ZeroOutputVerified,
        Self::LowTorqueArmed,
        Self::LowTorqueVerified,
        Self::SimulatorSmokeArmed,
        Self::SmokeReady,
    ];

    /// Return the valid next stage for a transition, if the transition is legal.
    #[must_use]
    pub const fn next(self, transition: HardwareTransition) -> Option<Self> {
        match (self, transition) {
            (Self::Disconnected, HardwareTransition::Enumerate) => Some(Self::Enumerated),
            (Self::Enumerated, HardwareTransition::TrustDescriptor) => {
                Some(Self::DescriptorTrusted)
            }
            (Self::DescriptorTrusted, HardwareTransition::VerifyPassive) => {
                Some(Self::PassiveVerified)
            }
            (Self::PassiveVerified, HardwareTransition::VerifyZeroOutput) => {
                Some(Self::ZeroOutputVerified)
            }
            (Self::ZeroOutputVerified, HardwareTransition::ArmLowTorque) => {
                Some(Self::LowTorqueArmed)
            }
            (Self::LowTorqueArmed, HardwareTransition::VerifyLowTorque) => {
                Some(Self::LowTorqueVerified)
            }
            (Self::LowTorqueVerified, HardwareTransition::ArmSimulatorSmoke) => {
                Some(Self::SimulatorSmokeArmed)
            }
            (Self::SimulatorSmokeArmed, HardwareTransition::VerifySmokeReady) => {
                Some(Self::SmokeReady)
            }
            _ => None,
        }
    }
}

/// Transition names for the validation state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareTransition {
    Enumerate,
    TrustDescriptor,
    VerifyPassive,
    VerifyZeroOutput,
    ArmLowTorque,
    VerifyLowTorque,
    ArmSimulatorSmoke,
    VerifySmokeReady,
}

impl HardwareTransition {
    /// All transitions.
    pub const ALL: [Self; 8] = [
        Self::Enumerate,
        Self::TrustDescriptor,
        Self::VerifyPassive,
        Self::VerifyZeroOutput,
        Self::ArmLowTorque,
        Self::VerifyLowTorque,
        Self::ArmSimulatorSmoke,
        Self::VerifySmokeReady,
    ];
}

/// Source class for a piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// Evidence captured from physical hardware.
    RealHardware,
    /// Evidence produced by a virtual hardware backend.
    Virtual,
    /// Evidence generated synthetically for parser or verifier tests.
    Synthetic,
}

/// Kind of receipt/evidence used by validation transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    DeviceIdentity,
    DescriptorTrust,
    PassiveVerification,
    ZeroOutputProof,
    LowTorqueArm,
    LowTorqueProof,
    SimulatorTelemetry,
    SimulatorSmoke,
    FinalZero,
}

/// A typed reference to a validation artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    kind: EvidenceKind,
    source: EvidenceSource,
    artifact_path: String,
    digest: Option<String>,
}

impl EvidenceRef {
    /// Create an evidence reference with a required non-empty artifact path.
    pub fn new(
        kind: EvidenceKind,
        source: EvidenceSource,
        artifact_path: impl Into<String>,
    ) -> Result<Self, EvidenceError> {
        let artifact_path = artifact_path.into();
        validate_non_empty("artifact_path", &artifact_path)?;
        Ok(Self {
            kind,
            source,
            artifact_path,
            digest: None,
        })
    }

    /// Attach a content digest to this evidence reference.
    pub fn with_digest(mut self, digest: impl Into<String>) -> Result<Self, EvidenceError> {
        let digest = digest.into();
        validate_non_empty("digest", &digest)?;
        self.digest = Some(digest);
        Ok(self)
    }

    /// The evidence kind.
    #[must_use]
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }

    /// The source class.
    #[must_use]
    pub const fn source(&self) -> EvidenceSource {
        self.source
    }

    /// Artifact path as recorded by the lane.
    #[must_use]
    pub fn artifact_path(&self) -> &str {
        &self.artifact_path
    }

    /// Optional content digest.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }
}

/// Errors while constructing evidence values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceError {
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), EvidenceError> {
    if value.trim().is_empty() {
        Err(EvidenceError::EmptyField { field })
    } else {
        Ok(())
    }
}

macro_rules! evidence_wrapper {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            evidence: EvidenceRef,
        }

        impl $name {
            pub fn new(
                source: EvidenceSource,
                artifact_path: impl Into<String>,
            ) -> Result<Self, EvidenceError> {
                Ok(Self {
                    evidence: EvidenceRef::new($kind, source, artifact_path)?,
                })
            }

            #[must_use]
            pub fn evidence(&self) -> &EvidenceRef {
                &self.evidence
            }

            fn into_evidence(self) -> EvidenceRef {
                self.evidence
            }
        }
    };
}

evidence_wrapper!(DescriptorTrustEvidence, EvidenceKind::DescriptorTrust);
evidence_wrapper!(
    PassiveVerificationEvidence,
    EvidenceKind::PassiveVerification
);
evidence_wrapper!(ZeroOutputEvidence, EvidenceKind::ZeroOutputProof);
evidence_wrapper!(LowTorqueArmEvidence, EvidenceKind::LowTorqueArm);
evidence_wrapper!(LowTorqueEvidence, EvidenceKind::LowTorqueProof);
evidence_wrapper!(SimulatorTelemetryEvidence, EvidenceKind::SimulatorTelemetry);
evidence_wrapper!(SimulatorSmokeEvidence, EvidenceKind::SimulatorSmoke);
evidence_wrapper!(FinalZeroEvidence, EvidenceKind::FinalZero);

/// Device enumeration evidence with a generic USB identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumerationEvidence {
    evidence: EvidenceRef,
    vendor_id: u16,
    product_id: u16,
    device_key: String,
}

impl EnumerationEvidence {
    pub fn new(
        source: EvidenceSource,
        artifact_path: impl Into<String>,
        vendor_id: u16,
        product_id: u16,
        device_key: impl Into<String>,
    ) -> Result<Self, EvidenceError> {
        let device_key = device_key.into();
        validate_non_empty("device_key", &device_key)?;
        Ok(Self {
            evidence: EvidenceRef::new(EvidenceKind::DeviceIdentity, source, artifact_path)?,
            vendor_id,
            product_id,
            device_key,
        })
    }

    #[must_use]
    pub fn evidence(&self) -> &EvidenceRef {
        &self.evidence
    }

    #[must_use]
    pub const fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    #[must_use]
    pub const fn product_id(&self) -> u16 {
        self.product_id
    }

    #[must_use]
    pub fn device_key(&self) -> &str {
        &self.device_key
    }

    fn into_evidence(self) -> EvidenceRef {
        self.evidence
    }
}

/// Transition evidence consumed by the runtime state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTransitionEvidence {
    Enumerated(EnumerationEvidence),
    DescriptorTrusted(DescriptorTrustEvidence),
    PassiveVerified(PassiveVerificationEvidence),
    ZeroOutputVerified(ZeroOutputEvidence),
    LowTorqueArmed(LowTorqueArmEvidence),
    LowTorqueVerified {
        proof: LowTorqueEvidence,
        final_zero: FinalZeroEvidence,
    },
    SimulatorSmokeArmed(SimulatorTelemetryEvidence),
    SmokeReady {
        smoke: SimulatorSmokeEvidence,
        final_zero: FinalZeroEvidence,
    },
}

impl HardwareTransitionEvidence {
    /// The transition represented by this evidence.
    #[must_use]
    pub const fn transition(&self) -> HardwareTransition {
        match self {
            Self::Enumerated(_) => HardwareTransition::Enumerate,
            Self::DescriptorTrusted(_) => HardwareTransition::TrustDescriptor,
            Self::PassiveVerified(_) => HardwareTransition::VerifyPassive,
            Self::ZeroOutputVerified(_) => HardwareTransition::VerifyZeroOutput,
            Self::LowTorqueArmed(_) => HardwareTransition::ArmLowTorque,
            Self::LowTorqueVerified { .. } => HardwareTransition::VerifyLowTorque,
            Self::SimulatorSmokeArmed(_) => HardwareTransition::ArmSimulatorSmoke,
            Self::SmokeReady { .. } => HardwareTransition::VerifySmokeReady,
        }
    }

    fn into_evidence_refs(self) -> Vec<EvidenceRef> {
        match self {
            Self::Enumerated(evidence) => vec![evidence.into_evidence()],
            Self::DescriptorTrusted(evidence) => vec![evidence.into_evidence()],
            Self::PassiveVerified(evidence) => vec![evidence.into_evidence()],
            Self::ZeroOutputVerified(evidence) => vec![evidence.into_evidence()],
            Self::LowTorqueArmed(evidence) => vec![evidence.into_evidence()],
            Self::LowTorqueVerified { proof, final_zero } => {
                vec![proof.into_evidence(), final_zero.into_evidence()]
            }
            Self::SimulatorSmokeArmed(evidence) => vec![evidence.into_evidence()],
            Self::SmokeReady { smoke, final_zero } => {
                vec![smoke.into_evidence(), final_zero.into_evidence()]
            }
        }
    }
}

/// Ordered evidence lineage for a validation lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLineage {
    stage: HardwareValidationStage,
    evidence: Vec<EvidenceRef>,
}

impl ValidationLineage {
    /// Start at the disconnected stage.
    #[must_use]
    pub fn disconnected() -> Self {
        Self {
            stage: HardwareValidationStage::Disconnected,
            evidence: Vec::new(),
        }
    }

    /// Current validation stage.
    #[must_use]
    pub const fn stage(&self) -> HardwareValidationStage {
        self.stage
    }

    /// Evidence collected in transition order.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceRef] {
        &self.evidence
    }

    fn advance_to(
        mut self,
        transition: HardwareTransition,
        next: HardwareValidationStage,
        evidence: impl IntoIterator<Item = EvidenceRef>,
    ) -> Self {
        debug_assert_eq!(self.stage.next(transition), Some(next));
        self.stage = next;
        self.evidence.extend(evidence);
        self
    }
}

/// Errors from runtime validation state transitions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("invalid transition {transition:?} from {from:?}")]
    InvalidTransition {
        from: HardwareValidationStage,
        transition: HardwareTransition,
    },
}

/// Runtime mirror of the typed validation flow for receipt verifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareValidationMachine {
    lineage: ValidationLineage,
}

impl HardwareValidationMachine {
    /// Start a runtime machine in the disconnected stage.
    #[must_use]
    pub fn disconnected() -> Self {
        Self {
            lineage: ValidationLineage::disconnected(),
        }
    }

    /// Current validation stage.
    #[must_use]
    pub const fn stage(&self) -> HardwareValidationStage {
        self.lineage.stage()
    }

    /// Evidence collected in transition order.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceRef] {
        self.lineage.evidence()
    }

    /// Apply a transition with typed evidence.
    pub fn apply(
        &mut self,
        evidence: HardwareTransitionEvidence,
    ) -> Result<HardwareValidationStage, TransitionError> {
        let transition = evidence.transition();
        let next =
            self.stage()
                .next(transition)
                .ok_or_else(|| TransitionError::InvalidTransition {
                    from: self.stage(),
                    transition,
                })?;

        self.lineage.stage = next;
        self.lineage.evidence.extend(evidence.into_evidence_refs());
        Ok(next)
    }

    /// Consume the machine into its lineage.
    #[must_use]
    pub fn into_lineage(self) -> ValidationLineage {
        self.lineage
    }
}

macro_rules! state_type {
    ($name:ident, $stage:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            lineage: ValidationLineage,
        }

        impl $name {
            #[must_use]
            pub const fn stage(&self) -> HardwareValidationStage {
                $stage
            }

            #[must_use]
            pub fn lineage(&self) -> &ValidationLineage {
                &self.lineage
            }

            #[must_use]
            pub fn evidence(&self) -> &[EvidenceRef] {
                self.lineage.evidence()
            }
        }
    };
}

state_type!(Disconnected, HardwareValidationStage::Disconnected);
state_type!(Enumerated, HardwareValidationStage::Enumerated);
state_type!(
    DescriptorTrusted,
    HardwareValidationStage::DescriptorTrusted
);
state_type!(PassiveVerified, HardwareValidationStage::PassiveVerified);
state_type!(
    ZeroOutputVerified,
    HardwareValidationStage::ZeroOutputVerified
);
state_type!(LowTorqueArmed, HardwareValidationStage::LowTorqueArmed);
state_type!(
    LowTorqueVerified,
    HardwareValidationStage::LowTorqueVerified
);
state_type!(
    SimulatorSmokeArmed,
    HardwareValidationStage::SimulatorSmokeArmed
);
state_type!(SmokeReady, HardwareValidationStage::SmokeReady);

impl Disconnected {
    /// Start the typed flow before any hardware has been observed.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lineage: ValidationLineage::disconnected(),
        }
    }

    /// Record enumeration evidence.
    #[must_use]
    pub fn enumerate(self, evidence: EnumerationEvidence) -> Enumerated {
        Enumerated {
            lineage: self.lineage.advance_to(
                HardwareTransition::Enumerate,
                HardwareValidationStage::Enumerated,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl Default for Disconnected {
    fn default() -> Self {
        Self::new()
    }
}

impl Enumerated {
    /// Record descriptor trust evidence.
    #[must_use]
    pub fn trust_descriptor(self, evidence: DescriptorTrustEvidence) -> DescriptorTrusted {
        DescriptorTrusted {
            lineage: self.lineage.advance_to(
                HardwareTransition::TrustDescriptor,
                HardwareValidationStage::DescriptorTrusted,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl DescriptorTrusted {
    /// Record passive parser/capture verification evidence.
    #[must_use]
    pub fn verify_passive(self, evidence: PassiveVerificationEvidence) -> PassiveVerified {
        PassiveVerified {
            lineage: self.lineage.advance_to(
                HardwareTransition::VerifyPassive,
                HardwareValidationStage::PassiveVerified,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl PassiveVerified {
    /// Record zero-output safety proof evidence.
    #[must_use]
    pub fn verify_zero_output(self, evidence: ZeroOutputEvidence) -> ZeroOutputVerified {
        ZeroOutputVerified {
            lineage: self.lineage.advance_to(
                HardwareTransition::VerifyZeroOutput,
                HardwareValidationStage::ZeroOutputVerified,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl ZeroOutputVerified {
    /// Arm a bounded low-torque proof.
    #[must_use]
    pub fn arm_low_torque(self, evidence: LowTorqueArmEvidence) -> LowTorqueArmed {
        LowTorqueArmed {
            lineage: self.lineage.advance_to(
                HardwareTransition::ArmLowTorque,
                HardwareValidationStage::LowTorqueArmed,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl LowTorqueArmed {
    /// Complete a bounded low-torque proof and record the final zero.
    #[must_use]
    pub fn verify_low_torque(
        self,
        proof: LowTorqueEvidence,
        final_zero: FinalZeroEvidence,
    ) -> LowTorqueVerified {
        LowTorqueVerified {
            lineage: self.lineage.advance_to(
                HardwareTransition::VerifyLowTorque,
                HardwareValidationStage::LowTorqueVerified,
                [proof.into_evidence(), final_zero.into_evidence()],
            ),
        }
    }
}

impl LowTorqueVerified {
    /// Arm simulator smoke from telemetry evidence.
    #[must_use]
    pub fn arm_simulator_smoke(self, evidence: SimulatorTelemetryEvidence) -> SimulatorSmokeArmed {
        SimulatorSmokeArmed {
            lineage: self.lineage.advance_to(
                HardwareTransition::ArmSimulatorSmoke,
                HardwareValidationStage::SimulatorSmokeArmed,
                [evidence.into_evidence()],
            ),
        }
    }
}

impl SimulatorSmokeArmed {
    /// Complete simulator-to-output smoke and record the final zero.
    #[must_use]
    pub fn verify_smoke_ready(
        self,
        smoke: SimulatorSmokeEvidence,
        final_zero: FinalZeroEvidence,
    ) -> SmokeReady {
        SmokeReady {
            lineage: self.lineage.advance_to(
                HardwareTransition::VerifySmokeReady,
                HardwareValidationStage::SmokeReady,
                [smoke.into_evidence(), final_zero.into_evidence()],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_enumeration() -> Result<EnumerationEvidence, EvidenceError> {
        EnumerationEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/device-list.json",
            0x1234,
            0x5678,
            "example-device",
        )
    }

    fn sample_descriptor() -> Result<DescriptorTrustEvidence, EvidenceError> {
        DescriptorTrustEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/descriptor.json",
        )
    }

    fn sample_passive() -> Result<PassiveVerificationEvidence, EvidenceError> {
        PassiveVerificationEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/passive-verification.json",
        )
    }

    fn sample_zero() -> Result<ZeroOutputEvidence, EvidenceError> {
        ZeroOutputEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/zero-verification.json",
        )
    }

    fn sample_low_torque_arm() -> Result<LowTorqueArmEvidence, EvidenceError> {
        LowTorqueArmEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/low-torque-arm.json",
        )
    }

    fn sample_low_torque() -> Result<LowTorqueEvidence, EvidenceError> {
        LowTorqueEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/low-torque-proof.json",
        )
    }

    fn sample_final_zero(path: &str) -> Result<FinalZeroEvidence, EvidenceError> {
        FinalZeroEvidence::new(EvidenceSource::RealHardware, path)
    }

    fn sample_telemetry() -> Result<SimulatorTelemetryEvidence, EvidenceError> {
        SimulatorTelemetryEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/simulator-telemetry-proof.json",
        )
    }

    fn sample_smoke() -> Result<SimulatorSmokeEvidence, EvidenceError> {
        SimulatorSmokeEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/simulator-ffb-smoke.json",
        )
    }

    fn all_transition_evidence() -> Result<Vec<HardwareTransitionEvidence>, EvidenceError> {
        Ok(vec![
            HardwareTransitionEvidence::Enumerated(sample_enumeration()?),
            HardwareTransitionEvidence::DescriptorTrusted(sample_descriptor()?),
            HardwareTransitionEvidence::PassiveVerified(sample_passive()?),
            HardwareTransitionEvidence::ZeroOutputVerified(sample_zero()?),
            HardwareTransitionEvidence::LowTorqueArmed(sample_low_torque_arm()?),
            HardwareTransitionEvidence::LowTorqueVerified {
                proof: sample_low_torque()?,
                final_zero: sample_final_zero("ci/hardware/example/low-torque-final-zero.json")?,
            },
            HardwareTransitionEvidence::SimulatorSmokeArmed(sample_telemetry()?),
            HardwareTransitionEvidence::SmokeReady {
                smoke: sample_smoke()?,
                final_zero: sample_final_zero("ci/hardware/example/smoke-final-zero.json")?,
            },
        ])
    }

    fn machine_at(stage: HardwareValidationStage) -> HardwareValidationMachine {
        HardwareValidationMachine {
            lineage: ValidationLineage {
                stage,
                evidence: Vec::new(),
            },
        }
    }

    #[test]
    fn typed_flow_reaches_smoke_ready() -> Result<(), EvidenceError> {
        let smoke_ready = Disconnected::new()
            .enumerate(sample_enumeration()?)
            .trust_descriptor(sample_descriptor()?)
            .verify_passive(sample_passive()?)
            .verify_zero_output(sample_zero()?)
            .arm_low_torque(sample_low_torque_arm()?)
            .verify_low_torque(
                sample_low_torque()?,
                sample_final_zero("ci/hardware/example/low-torque-final-zero.json")?,
            )
            .arm_simulator_smoke(sample_telemetry()?)
            .verify_smoke_ready(
                sample_smoke()?,
                sample_final_zero("ci/hardware/example/smoke-final-zero.json")?,
            );

        assert_eq!(smoke_ready.stage(), HardwareValidationStage::SmokeReady);
        assert_eq!(smoke_ready.evidence().len(), 10);
        assert_eq!(
            smoke_ready.evidence().last().map(EvidenceRef::kind),
            Some(EvidenceKind::FinalZero)
        );
        Ok(())
    }

    #[test]
    fn runtime_machine_accepts_the_ordered_path() -> Result<(), EvidenceError> {
        let mut machine = HardwareValidationMachine::disconnected();

        for evidence in all_transition_evidence()? {
            let transition = evidence.transition();
            let expected = machine.stage().next(transition);
            let result = machine.apply(evidence);
            assert!(result.is_ok(), "transition {transition:?} should be valid");
            assert_eq!(Some(machine.stage()), expected);
        }

        assert_eq!(machine.stage(), HardwareValidationStage::SmokeReady);
        assert_eq!(machine.evidence().len(), 10);
        Ok(())
    }

    #[test]
    fn runtime_machine_rejects_every_invalid_transition() -> Result<(), EvidenceError> {
        for stage in HardwareValidationStage::ALL {
            for evidence in all_transition_evidence()? {
                let transition = evidence.transition();
                let mut machine = machine_at(stage);
                let result = machine.apply(evidence);

                if stage.next(transition).is_some() {
                    assert!(result.is_ok(), "{stage:?} should accept {transition:?}");
                } else {
                    assert_eq!(
                        result,
                        Err(TransitionError::InvalidTransition {
                            from: stage,
                            transition,
                        }),
                        "{stage:?} should reject {transition:?}"
                    );
                    assert_eq!(machine.stage(), stage);
                    assert!(machine.evidence().is_empty());
                }
            }
        }
        Ok(())
    }

    #[test]
    fn evidence_rejects_empty_artifact_paths() {
        let result = ZeroOutputEvidence::new(EvidenceSource::Synthetic, " ");
        assert_eq!(
            result,
            Err(EvidenceError::EmptyField {
                field: "artifact_path",
            })
        );
    }

    #[test]
    fn enumeration_rejects_empty_device_key() {
        let result = EnumerationEvidence::new(
            EvidenceSource::RealHardware,
            "ci/hardware/example/device-list.json",
            1,
            2,
            "",
        );
        assert_eq!(
            result,
            Err(EvidenceError::EmptyField {
                field: "device_key",
            })
        );
    }

    #[test]
    fn evidence_serializes_source_and_kind() -> Result<(), Box<dyn std::error::Error>> {
        let evidence = EvidenceRef::new(
            EvidenceKind::SimulatorTelemetry,
            EvidenceSource::Virtual,
            "target/virtual/simulator-telemetry-proof.json",
        )?
        .with_digest("sha256:test")?;

        let json = serde_json::to_string(&evidence)?;
        assert!(json.contains("simulator_telemetry"));
        assert!(json.contains("virtual"));
        assert!(json.contains("sha256:test"));
        Ok(())
    }
}
