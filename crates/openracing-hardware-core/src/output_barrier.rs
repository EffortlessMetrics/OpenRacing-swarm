//! Hardware-family-neutral output write barrier.
//!
//! The barrier does not write HID reports. It evaluates already-encoded output
//! commands against capability, watchdog, and final-zero gates so device
//! adapters can share one safety decision model before they call an output
//! transport.

use serde::{Deserialize, Serialize};

use crate::{LowTorqueArmed, SimulatorSmokeArmed, ZeroOutputVerified};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputCommand {
    signed_percent: f32,
}

impl OutputCommand {
    pub const ZERO: Self = Self {
        signed_percent: 0.0,
    };

    pub fn new(signed_percent: f32) -> Result<Self, OutputBarrierError> {
        if !signed_percent.is_finite() {
            return Err(OutputBarrierError::NonFiniteCommand);
        }

        if signed_percent.abs() > 100.0 {
            return Err(OutputBarrierError::CommandOutsideAbsoluteRange { signed_percent });
        }

        Ok(Self { signed_percent })
    }

    #[must_use]
    pub const fn signed_percent(self) -> f32 {
        self.signed_percent
    }

    #[must_use]
    pub fn is_zero(self) -> bool {
        self.signed_percent.abs() <= f32::EPSILON
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputCapabilityStage {
    ZeroOnly,
    LowTorque,
    SimulatorSmoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputCapability {
    stage: OutputCapabilityStage,
    max_percent: f32,
}

impl OutputCapability {
    #[must_use]
    pub const fn zero_only() -> Self {
        Self {
            stage: OutputCapabilityStage::ZeroOnly,
            max_percent: 0.0,
        }
    }

    fn bounded(stage: OutputCapabilityStage, max_percent: f32) -> Result<Self, OutputBarrierError> {
        validate_max_percent(max_percent)?;
        Ok(Self { stage, max_percent })
    }

    #[must_use]
    pub const fn stage(self) -> OutputCapabilityStage {
        self.stage
    }

    #[must_use]
    pub const fn max_percent(self) -> f32 {
        self.max_percent
    }

    #[must_use]
    pub const fn allows_non_zero(self) -> bool {
        matches!(
            self.stage,
            OutputCapabilityStage::LowTorque | OutputCapabilityStage::SimulatorSmoke
        )
    }
}

impl ZeroOutputVerified {
    #[must_use]
    pub const fn zero_output_capability(&self) -> OutputCapability {
        OutputCapability::zero_only()
    }
}

impl LowTorqueArmed {
    pub fn low_torque_output_capability(
        &self,
        max_percent: f32,
    ) -> Result<OutputCapability, OutputBarrierError> {
        OutputCapability::bounded(OutputCapabilityStage::LowTorque, max_percent)
    }
}

impl SimulatorSmokeArmed {
    pub fn simulator_smoke_output_capability(
        &self,
        max_percent: f32,
    ) -> Result<OutputCapability, OutputBarrierError> {
        OutputCapability::bounded(OutputCapabilityStage::SimulatorSmoke, max_percent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputWatchdogState {
    active: bool,
    expired: bool,
    timeout_ms: Option<u64>,
}

impl OutputWatchdogState {
    #[must_use]
    pub const fn inactive() -> Self {
        Self {
            active: false,
            expired: false,
            timeout_ms: None,
        }
    }

    pub fn active(timeout_ms: u64) -> Result<Self, OutputBarrierError> {
        if timeout_ms == 0 {
            return Err(OutputBarrierError::InvalidWatchdogTimeout { timeout_ms });
        }

        Ok(Self {
            active: true,
            expired: false,
            timeout_ms: Some(timeout_ms),
        })
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn is_expired(self) -> bool {
        self.expired
    }

    #[must_use]
    pub const fn timeout_ms(self) -> Option<u64> {
        self.timeout_ms
    }

    #[must_use]
    pub const fn expired(mut self) -> Self {
        self.expired = true;
        self
    }

    #[must_use]
    pub const fn refreshed(mut self) -> Self {
        self.expired = false;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalZeroPolicy {
    required_after_non_zero: bool,
}

impl FinalZeroPolicy {
    #[must_use]
    pub const fn required() -> Self {
        Self {
            required_after_non_zero: true,
        }
    }

    #[must_use]
    pub const fn not_required() -> Self {
        Self {
            required_after_non_zero: false,
        }
    }

    #[must_use]
    pub const fn required_after_non_zero(self) -> bool {
        self.required_after_non_zero
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputWriteBarrier {
    capability: OutputCapability,
    watchdog: OutputWatchdogState,
    final_zero_policy: FinalZeroPolicy,
    final_zero_pending: bool,
    events: Vec<OutputBarrierEvent>,
}

impl OutputWriteBarrier {
    #[must_use]
    pub fn new(
        capability: OutputCapability,
        watchdog: OutputWatchdogState,
        final_zero_policy: FinalZeroPolicy,
    ) -> Self {
        Self {
            capability,
            watchdog,
            final_zero_policy,
            final_zero_pending: false,
            events: Vec::new(),
        }
    }

    #[must_use]
    pub const fn capability(&self) -> OutputCapability {
        self.capability
    }

    #[must_use]
    pub const fn watchdog(&self) -> OutputWatchdogState {
        self.watchdog
    }

    #[must_use]
    pub const fn final_zero_pending(&self) -> bool {
        self.final_zero_pending
    }

    #[must_use]
    pub fn events(&self) -> &[OutputBarrierEvent] {
        &self.events
    }

    pub fn set_watchdog(&mut self, watchdog: OutputWatchdogState) {
        self.watchdog = watchdog;
    }

    pub fn evaluate(
        &mut self,
        command: OutputCommand,
    ) -> Result<OutputBarrierDecision, OutputBarrierError> {
        if command.is_zero() {
            return Ok(self.accept_zero(command));
        }

        self.require_non_zero_capability(command)?;
        self.require_watchdog(command)?;
        self.require_within_limit(command)?;

        if self.final_zero_policy.required_after_non_zero() {
            self.final_zero_pending = true;
        }

        let decision = OutputBarrierDecision {
            command,
            reason: OutputBarrierDecisionReason::NonZeroAccepted,
            final_zero_required_after_write: self.final_zero_pending,
        };
        self.events.push(OutputBarrierEvent::accepted(decision));
        Ok(decision)
    }

    #[must_use]
    pub fn final_zero_command(&mut self) -> Option<OutputCommand> {
        if !self.final_zero_pending {
            return None;
        }

        self.final_zero_pending = false;
        self.events.push(OutputBarrierEvent {
            kind: OutputBarrierEventKind::FinalZeroRequired,
            command: Some(OutputCommand::ZERO),
            reason: Some(OutputBarrierDecisionReason::FinalZero),
            error: None,
        });
        Some(OutputCommand::ZERO)
    }

    fn accept_zero(&mut self, command: OutputCommand) -> OutputBarrierDecision {
        let reason = if self.final_zero_pending {
            self.final_zero_pending = false;
            OutputBarrierDecisionReason::FinalZero
        } else {
            OutputBarrierDecisionReason::ZeroAccepted
        };

        let decision = OutputBarrierDecision {
            command,
            reason,
            final_zero_required_after_write: self.final_zero_pending,
        };
        self.events.push(OutputBarrierEvent::accepted(decision));
        decision
    }

    fn require_non_zero_capability(
        &mut self,
        command: OutputCommand,
    ) -> Result<(), OutputBarrierError> {
        if self.capability.allows_non_zero() {
            Ok(())
        } else {
            let error = OutputBarrierError::NonZeroWithoutCapability {
                stage: self.capability.stage(),
                signed_percent: command.signed_percent(),
            };
            self.events
                .push(OutputBarrierEvent::blocked(command, &error));
            Err(error)
        }
    }

    fn require_watchdog(&mut self, command: OutputCommand) -> Result<(), OutputBarrierError> {
        let error = if !self.watchdog.is_active() {
            Some(OutputBarrierError::WatchdogNotActive)
        } else if self.watchdog.is_expired() {
            Some(OutputBarrierError::WatchdogExpired {
                timeout_ms: self.watchdog.timeout_ms(),
            })
        } else {
            None
        };

        if let Some(error) = error {
            self.events
                .push(OutputBarrierEvent::blocked(command, &error));
            Err(error)
        } else {
            Ok(())
        }
    }

    fn require_within_limit(&mut self, command: OutputCommand) -> Result<(), OutputBarrierError> {
        let requested_percent = command.signed_percent().abs();
        if requested_percent <= self.capability.max_percent() {
            return Ok(());
        }

        let error = OutputBarrierError::OutputExceedsCapability {
            requested_percent,
            max_percent: self.capability.max_percent(),
        };
        self.events
            .push(OutputBarrierEvent::blocked(command, &error));
        Err(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputBarrierDecision {
    command: OutputCommand,
    reason: OutputBarrierDecisionReason,
    final_zero_required_after_write: bool,
}

impl OutputBarrierDecision {
    #[must_use]
    pub const fn command(self) -> OutputCommand {
        self.command
    }

    #[must_use]
    pub const fn reason(self) -> OutputBarrierDecisionReason {
        self.reason
    }

    #[must_use]
    pub const fn final_zero_required_after_write(self) -> bool {
        self.final_zero_required_after_write
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputBarrierDecisionReason {
    ZeroAccepted,
    NonZeroAccepted,
    FinalZero,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBarrierEvent {
    kind: OutputBarrierEventKind,
    command: Option<OutputCommand>,
    reason: Option<OutputBarrierDecisionReason>,
    error: Option<String>,
}

impl OutputBarrierEvent {
    fn accepted(decision: OutputBarrierDecision) -> Self {
        Self {
            kind: OutputBarrierEventKind::Accepted,
            command: Some(decision.command()),
            reason: Some(decision.reason()),
            error: None,
        }
    }

    fn blocked(command: OutputCommand, error: &OutputBarrierError) -> Self {
        Self {
            kind: OutputBarrierEventKind::Blocked,
            command: Some(command),
            reason: None,
            error: Some(error.to_string()),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> OutputBarrierEventKind {
        self.kind
    }

    #[must_use]
    pub const fn command(&self) -> Option<OutputCommand> {
        self.command
    }

    #[must_use]
    pub const fn reason(&self) -> Option<OutputBarrierDecisionReason> {
        self.reason
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputBarrierEventKind {
    Accepted,
    Blocked,
    FinalZeroRequired,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum OutputBarrierError {
    #[error("output command percent must be finite")]
    NonFiniteCommand,

    #[error("output command {signed_percent}% is outside the absolute -100..=100 range")]
    CommandOutsideAbsoluteRange { signed_percent: f32 },

    #[error("max output percent must be finite and in the 0..=100 range, got {max_percent}")]
    InvalidMaxPercent { max_percent: f32 },

    #[error("non-zero output {signed_percent}% is not allowed by {stage:?}")]
    NonZeroWithoutCapability {
        stage: OutputCapabilityStage,
        signed_percent: f32,
    },

    #[error("output {requested_percent}% exceeds capability limit {max_percent}%")]
    OutputExceedsCapability {
        requested_percent: f32,
        max_percent: f32,
    },

    #[error("non-zero output requires an active watchdog")]
    WatchdogNotActive,

    #[error("watchdog expired before non-zero output; timeout_ms={timeout_ms:?}")]
    WatchdogExpired { timeout_ms: Option<u64> },

    #[error("watchdog timeout must be non-zero, got {timeout_ms}")]
    InvalidWatchdogTimeout { timeout_ms: u64 },
}

fn validate_max_percent(max_percent: f32) -> Result<(), OutputBarrierError> {
    if max_percent.is_finite() && max_percent > 0.0 && max_percent <= 100.0 {
        Ok(())
    } else {
        Err(OutputBarrierError::InvalidMaxPercent { max_percent })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FinalZeroPolicy, OutputBarrierDecisionReason, OutputBarrierError, OutputBarrierEventKind,
        OutputCapabilityStage, OutputCommand, OutputWatchdogState, OutputWriteBarrier,
    };
    use crate::{
        DescriptorTrustEvidence, Disconnected, EnumerationEvidence, EvidenceError, EvidenceSource,
        FinalZeroEvidence, LowTorqueArmEvidence, LowTorqueArmed, LowTorqueEvidence,
        LowTorqueVerified, PassiveVerificationEvidence, SimulatorSmokeArmed,
        SimulatorTelemetryEvidence, ZeroOutputEvidence, ZeroOutputVerified,
    };

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

    fn zero_verified() -> Result<ZeroOutputVerified, EvidenceError> {
        Ok(Disconnected::new()
            .enumerate(sample_enumeration()?)
            .trust_descriptor(sample_descriptor()?)
            .verify_passive(sample_passive()?)
            .verify_zero_output(sample_zero()?))
    }

    fn low_torque_armed() -> Result<LowTorqueArmed, EvidenceError> {
        Ok(zero_verified()?.arm_low_torque(sample_low_torque_arm()?))
    }

    fn low_torque_verified() -> Result<LowTorqueVerified, EvidenceError> {
        Ok(low_torque_armed()?.verify_low_torque(
            sample_low_torque()?,
            sample_final_zero("ci/hardware/example/low-torque-final-zero.json")?,
        ))
    }

    fn simulator_smoke_armed() -> Result<SimulatorSmokeArmed, EvidenceError> {
        Ok(low_torque_verified()?.arm_simulator_smoke(sample_telemetry()?))
    }

    #[test]
    fn zero_capability_accepts_zero_and_rejects_non_zero() -> Result<(), Box<dyn std::error::Error>>
    {
        let capability = zero_verified()?.zero_output_capability();
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::inactive(),
            FinalZeroPolicy::required(),
        );

        let zero = barrier.evaluate(OutputCommand::ZERO)?;
        assert_eq!(zero.reason(), OutputBarrierDecisionReason::ZeroAccepted);
        assert!(!barrier.final_zero_pending());

        let non_zero = OutputCommand::new(0.1)?;
        let result = barrier.evaluate(non_zero);

        assert_eq!(
            result,
            Err(OutputBarrierError::NonZeroWithoutCapability {
                stage: OutputCapabilityStage::ZeroOnly,
                signed_percent: 0.1,
            })
        );
        assert_eq!(barrier.events().len(), 2);
        let Some(blocked_event) = barrier.events().get(1) else {
            return Err("expected blocked event".into());
        };
        assert_eq!(blocked_event.kind(), OutputBarrierEventKind::Blocked);
        Ok(())
    }

    #[test]
    fn low_torque_capability_accepts_bounded_non_zero_and_requires_final_zero()
    -> Result<(), Box<dyn std::error::Error>> {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::active(100)?,
            FinalZeroPolicy::required(),
        );

        let decision = barrier.evaluate(OutputCommand::new(-1.5)?)?;
        assert_eq!(
            decision.reason(),
            OutputBarrierDecisionReason::NonZeroAccepted
        );
        assert!(decision.final_zero_required_after_write());
        assert!(barrier.final_zero_pending());

        let Some(final_zero) = barrier.final_zero_command() else {
            return Err("expected final zero command".into());
        };
        assert!(final_zero.is_zero());
        assert!(!barrier.final_zero_pending());

        let Some(event) = barrier.events().last() else {
            return Err("expected final zero event".into());
        };
        assert_eq!(event.kind(), OutputBarrierEventKind::FinalZeroRequired);
        Ok(())
    }

    #[test]
    fn simulator_smoke_capability_uses_its_own_stage() -> Result<(), Box<dyn std::error::Error>> {
        let capability = simulator_smoke_armed()?.simulator_smoke_output_capability(5.0)?;

        assert_eq!(capability.stage(), OutputCapabilityStage::SimulatorSmoke);
        assert!((capability.max_percent() - 5.0).abs() <= f32::EPSILON);
        Ok(())
    }

    #[test]
    fn barrier_rejects_output_above_capability() -> Result<(), Box<dyn std::error::Error>> {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::active(100)?,
            FinalZeroPolicy::required(),
        );

        let result = barrier.evaluate(OutputCommand::new(2.1)?);

        assert_eq!(
            result,
            Err(OutputBarrierError::OutputExceedsCapability {
                requested_percent: 2.1,
                max_percent: 2.0,
            })
        );
        assert!(!barrier.final_zero_pending());
        Ok(())
    }

    #[test]
    fn barrier_requires_active_watchdog_for_non_zero() -> Result<(), Box<dyn std::error::Error>> {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::inactive(),
            FinalZeroPolicy::required(),
        );

        let result = barrier.evaluate(OutputCommand::new(1.0)?);

        assert_eq!(result, Err(OutputBarrierError::WatchdogNotActive));
        assert!(!barrier.final_zero_pending());
        Ok(())
    }

    #[test]
    fn expired_watchdog_blocks_non_zero_but_allows_zero() -> Result<(), Box<dyn std::error::Error>>
    {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::active(100)?.expired(),
            FinalZeroPolicy::required(),
        );

        let result = barrier.evaluate(OutputCommand::new(1.0)?);
        assert_eq!(
            result,
            Err(OutputBarrierError::WatchdogExpired {
                timeout_ms: Some(100),
            })
        );

        let zero = barrier.evaluate(OutputCommand::ZERO)?;
        assert_eq!(zero.reason(), OutputBarrierDecisionReason::ZeroAccepted);
        Ok(())
    }

    #[test]
    fn observed_zero_clears_pending_final_zero() -> Result<(), Box<dyn std::error::Error>> {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::active(100)?,
            FinalZeroPolicy::required(),
        );

        barrier.evaluate(OutputCommand::new(1.0)?)?;
        let final_zero = barrier.evaluate(OutputCommand::ZERO)?;

        assert_eq!(final_zero.reason(), OutputBarrierDecisionReason::FinalZero);
        assert!(!barrier.final_zero_pending());
        assert!(barrier.final_zero_command().is_none());
        Ok(())
    }

    #[test]
    fn invalid_commands_and_limits_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            OutputCommand::new(f32::NAN),
            Err(OutputBarrierError::NonFiniteCommand)
        );
        assert_eq!(
            OutputCommand::new(101.0),
            Err(OutputBarrierError::CommandOutsideAbsoluteRange {
                signed_percent: 101.0,
            })
        );
        assert_eq!(
            low_torque_armed()?.low_torque_output_capability(0.0),
            Err(OutputBarrierError::InvalidMaxPercent { max_percent: 0.0 })
        );
        assert_eq!(
            OutputWatchdogState::active(0),
            Err(OutputBarrierError::InvalidWatchdogTimeout { timeout_ms: 0 })
        );
        Ok(())
    }

    #[test]
    fn barrier_receipt_events_serialize() -> Result<(), Box<dyn std::error::Error>> {
        let capability = low_torque_armed()?.low_torque_output_capability(2.0)?;
        let mut barrier = OutputWriteBarrier::new(
            capability,
            OutputWatchdogState::active(100)?,
            FinalZeroPolicy::required(),
        );

        barrier.evaluate(OutputCommand::new(1.0)?)?;
        let _ = barrier.final_zero_command();

        let json = serde_json::to_string(barrier.events())?;
        assert!(json.contains("accepted"));
        assert!(json.contains("final_zero_required"));
        Ok(())
    }
}
