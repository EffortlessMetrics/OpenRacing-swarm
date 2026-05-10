//! Static hardware capability ceilings and receipt-backed validation stages.
//!
//! The registry records what a device is allowed to do at most. It does not
//! discover hardware and it does not validate any stage by itself. Default
//! records intentionally start with no validated stages; higher layers must
//! attach real-hardware lineage before a record may claim receipt-backed
//! validation.

use serde::{Deserialize, Serialize};

use crate::{EvidenceSource, HardwareValidationStage, ValidationLineage};

const MOZA_VENDOR_ID: u16 = 0x346E;
const MOZA_R5_V1_PID: u16 = 0x0004;
const MOZA_R5_V2_PID: u16 = 0x0014;
const MOZA_SRP_PEDALS_PID: u16 = 0x0003;
const MOZA_HGP_SHIFTER_PID: u16 = 0x0020;
const MOZA_SGP_SHIFTER_PID: u16 = 0x0021;
const MOZA_HBP_HANDBRAKE_PID: u16 = 0x0022;

/// Broad hardware family for capability ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFamily {
    Moza,
    Simagic,
    Fanatec,
    Logitech,
    Heusinkveld,
    GenericHid,
    Unknown,
}

/// Coarse device role used for safety and output-capability decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCapabilityKind {
    Wheelbase,
    Pedals,
    Handbrake,
    Shifter,
    ButtonBox,
    Unknown,
}

/// A static capability ceiling plus receipt-backed validation state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCapabilityRecord {
    vendor_id: u16,
    product_id: u16,
    family: DeviceFamily,
    model: String,
    kind: DeviceCapabilityKind,
    input: bool,
    ffb_output: bool,
    serial_config: bool,
    firmware_dfu: bool,
    high_torque: bool,
    validated_stages: Vec<HardwareValidationStage>,
}

impl DeviceCapabilityRecord {
    /// Create a conservative passive-only record for an unknown HID device.
    pub fn unknown_passive(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            family: DeviceFamily::Unknown,
            model: "Unknown HID device".to_string(),
            kind: DeviceCapabilityKind::Unknown,
            input: true,
            ffb_output: false,
            serial_config: false,
            firmware_dfu: false,
            high_torque: false,
            validated_stages: Vec::new(),
        }
    }

    /// Attach real-hardware validation lineage to this record.
    ///
    /// Virtual and synthetic evidence are deliberately rejected so simulation
    /// receipts cannot satisfy real hardware gates.
    pub fn with_validated_lineage(
        mut self,
        lineage: &ValidationLineage,
    ) -> Result<Self, CapabilityRegistryError> {
        if lineage.stage() == HardwareValidationStage::Disconnected {
            return Err(CapabilityRegistryError::DisconnectedLineage);
        }

        if lineage.evidence().is_empty() {
            return Err(CapabilityRegistryError::MissingEvidence {
                stage: lineage.stage(),
            });
        }

        for evidence in lineage.evidence() {
            if evidence.source() != EvidenceSource::RealHardware {
                return Err(CapabilityRegistryError::NonRealHardwareEvidence {
                    evidence_source: evidence.source(),
                });
            }
        }

        self.validated_stages = validated_stages_through(lineage.stage()).to_vec();
        self.validate()?;
        Ok(self)
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
    pub const fn family(&self) -> DeviceFamily {
        self.family
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn kind(&self) -> DeviceCapabilityKind {
        self.kind
    }

    #[must_use]
    pub const fn input(&self) -> bool {
        self.input
    }

    #[must_use]
    pub const fn ffb_output(&self) -> bool {
        self.ffb_output
    }

    #[must_use]
    pub const fn serial_config(&self) -> bool {
        self.serial_config
    }

    #[must_use]
    pub const fn firmware_dfu(&self) -> bool {
        self.firmware_dfu
    }

    #[must_use]
    pub const fn high_torque(&self) -> bool {
        self.high_torque
    }

    #[must_use]
    pub fn validated_stages(&self) -> &[HardwareValidationStage] {
        &self.validated_stages
    }

    fn validate(&self) -> Result<(), CapabilityRegistryError> {
        if self.model.trim().is_empty() {
            return Err(CapabilityRegistryError::EmptyModel {
                vendor_id: self.vendor_id,
                product_id: self.product_id,
            });
        }

        if !self.ffb_output
            && self
                .validated_stages
                .iter()
                .any(stage_requires_output_capability)
        {
            return Err(
                CapabilityRegistryError::ValidatedOutputStageWithoutOutputCapability {
                    vendor_id: self.vendor_id,
                    product_id: self.product_id,
                },
            );
        }

        if self.high_torque && !self.ffb_output {
            return Err(CapabilityRegistryError::HighTorqueWithoutOutputCapability {
                vendor_id: self.vendor_id,
                product_id: self.product_id,
            });
        }

        Ok(())
    }
}

/// In-memory capability registry for known devices plus unknown fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCapabilityRegistry {
    records: Vec<DeviceCapabilityRecord>,
}

impl DeviceCapabilityRegistry {
    /// Build a registry from explicit records.
    pub fn new(
        records: impl IntoIterator<Item = DeviceCapabilityRecord>,
    ) -> Result<Self, CapabilityRegistryError> {
        let records = records.into_iter().collect::<Vec<_>>();
        for record in &records {
            record.validate()?;
        }
        Ok(Self { records })
    }

    /// Static OpenRacing capability ceilings known without real receipts.
    #[must_use]
    pub fn openracing_defaults() -> Self {
        Self {
            records: vec![
                moza_wheelbase(MOZA_R5_V1_PID, "Moza R5 V1"),
                moza_wheelbase(MOZA_R5_V2_PID, "Moza R5 V2"),
                moza_input_only(
                    MOZA_SRP_PEDALS_PID,
                    "Moza SR-P pedals",
                    DeviceCapabilityKind::Pedals,
                ),
                moza_input_only(
                    MOZA_HBP_HANDBRAKE_PID,
                    "Moza HBP handbrake",
                    DeviceCapabilityKind::Handbrake,
                ),
                moza_input_only(
                    MOZA_HGP_SHIFTER_PID,
                    "Moza HGP shifter",
                    DeviceCapabilityKind::Shifter,
                ),
                moza_input_only(
                    MOZA_SGP_SHIFTER_PID,
                    "Moza SGP shifter",
                    DeviceCapabilityKind::Shifter,
                ),
            ],
        }
    }

    /// All known records.
    #[must_use]
    pub fn records(&self) -> &[DeviceCapabilityRecord] {
        &self.records
    }

    /// Look up a known record or return a conservative passive-only fallback.
    #[must_use]
    pub fn lookup(&self, vendor_id: u16, product_id: u16) -> DeviceCapabilityRecord {
        self.records
            .iter()
            .find(|record| record.vendor_id == vendor_id && record.product_id == product_id)
            .cloned()
            .unwrap_or_else(|| DeviceCapabilityRecord::unknown_passive(vendor_id, product_id))
    }
}

impl Default for DeviceCapabilityRegistry {
    fn default() -> Self {
        Self::openracing_defaults()
    }
}

/// Errors while constructing or validating capability records.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityRegistryError {
    #[error("device capability record {vendor_id:#06x}:{product_id:#06x} has an empty model")]
    EmptyModel { vendor_id: u16, product_id: u16 },
    #[error("disconnected lineage cannot validate a device capability record")]
    DisconnectedLineage,
    #[error("lineage at stage {stage:?} has no receipt evidence")]
    MissingEvidence { stage: HardwareValidationStage },
    #[error("non-real-hardware evidence {evidence_source:?} cannot validate a capability record")]
    NonRealHardwareEvidence { evidence_source: EvidenceSource },
    #[error(
        "device capability record {vendor_id:#06x}:{product_id:#06x} validates output stages without output capability"
    )]
    ValidatedOutputStageWithoutOutputCapability { vendor_id: u16, product_id: u16 },
    #[error(
        "device capability record {vendor_id:#06x}:{product_id:#06x} enables high torque without output capability"
    )]
    HighTorqueWithoutOutputCapability { vendor_id: u16, product_id: u16 },
}

fn moza_wheelbase(product_id: u16, model: &'static str) -> DeviceCapabilityRecord {
    DeviceCapabilityRecord {
        vendor_id: MOZA_VENDOR_ID,
        product_id,
        family: DeviceFamily::Moza,
        model: model.to_string(),
        kind: DeviceCapabilityKind::Wheelbase,
        input: true,
        ffb_output: true,
        serial_config: false,
        firmware_dfu: false,
        high_torque: false,
        validated_stages: Vec::new(),
    }
}

fn moza_input_only(
    product_id: u16,
    model: &'static str,
    kind: DeviceCapabilityKind,
) -> DeviceCapabilityRecord {
    DeviceCapabilityRecord {
        vendor_id: MOZA_VENDOR_ID,
        product_id,
        family: DeviceFamily::Moza,
        model: model.to_string(),
        kind,
        input: true,
        ffb_output: false,
        serial_config: false,
        firmware_dfu: false,
        high_torque: false,
        validated_stages: Vec::new(),
    }
}

fn validated_stages_through(stage: HardwareValidationStage) -> &'static [HardwareValidationStage] {
    match stage {
        HardwareValidationStage::Disconnected => &[],
        HardwareValidationStage::Enumerated => &HardwareValidationStage::ALL[1..=1],
        HardwareValidationStage::DescriptorTrusted => &HardwareValidationStage::ALL[1..=2],
        HardwareValidationStage::PassiveVerified => &HardwareValidationStage::ALL[1..=3],
        HardwareValidationStage::ZeroOutputVerified => &HardwareValidationStage::ALL[1..=4],
        HardwareValidationStage::LowTorqueArmed => &HardwareValidationStage::ALL[1..=5],
        HardwareValidationStage::LowTorqueVerified => &HardwareValidationStage::ALL[1..=6],
        HardwareValidationStage::SimulatorSmokeArmed => &HardwareValidationStage::ALL[1..=7],
        HardwareValidationStage::SmokeReady => &HardwareValidationStage::ALL[1..=8],
    }
}

fn stage_requires_output_capability(stage: &HardwareValidationStage) -> bool {
    matches!(
        stage,
        HardwareValidationStage::ZeroOutputVerified
            | HardwareValidationStage::LowTorqueArmed
            | HardwareValidationStage::LowTorqueVerified
            | HardwareValidationStage::SimulatorSmokeArmed
            | HardwareValidationStage::SmokeReady
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DescriptorTrustEvidence, Disconnected, EnumerationEvidence, EvidenceError,
        LowTorqueArmEvidence, PassiveVerificationEvidence,
    };

    fn real_enumerated_lineage() -> Result<ValidationLineage, EvidenceError> {
        Ok(Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/device-list.json",
                MOZA_VENDOR_ID,
                MOZA_R5_V2_PID,
                "moza-r5",
            )?)
            .lineage()
            .clone())
    }

    fn real_passive_lineage() -> Result<ValidationLineage, EvidenceError> {
        Ok(Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/device-list.json",
                MOZA_VENDOR_ID,
                MOZA_R5_V2_PID,
                "moza-r5",
            )?)
            .trust_descriptor(DescriptorTrustEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/descriptor.json",
            )?)
            .verify_passive(PassiveVerificationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/passive-verification.json",
            )?)
            .lineage()
            .clone())
    }

    fn virtual_passive_lineage() -> Result<ValidationLineage, EvidenceError> {
        Ok(Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::Virtual,
                "target/openracing/virtual/device-list.json",
                MOZA_VENDOR_ID,
                MOZA_R5_V2_PID,
                "virtual-moza-r5",
            )?)
            .trust_descriptor(DescriptorTrustEvidence::new(
                EvidenceSource::Virtual,
                "target/openracing/virtual/descriptor.json",
            )?)
            .verify_passive(PassiveVerificationEvidence::new(
                EvidenceSource::Virtual,
                "target/openracing/virtual/passive-verification.json",
            )?)
            .lineage()
            .clone())
    }

    fn synthetic_enumerated_lineage() -> Result<ValidationLineage, EvidenceError> {
        Ok(Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::Synthetic,
                "crates/openracing-hardware-core/fixtures/synthetic-device.json",
                MOZA_VENDOR_ID,
                MOZA_R5_V2_PID,
                "synthetic-moza-r5",
            )?)
            .lineage()
            .clone())
    }

    #[test]
    fn defaults_include_steven_moza_stack_without_validated_stages() {
        let registry = DeviceCapabilityRegistry::openracing_defaults();

        let r5_v1 = registry.lookup(MOZA_VENDOR_ID, MOZA_R5_V1_PID);
        let r5_v2 = registry.lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID);
        let srp = registry.lookup(MOZA_VENDOR_ID, MOZA_SRP_PEDALS_PID);
        let hbp = registry.lookup(MOZA_VENDOR_ID, MOZA_HBP_HANDBRAKE_PID);

        assert_eq!(r5_v1.family(), DeviceFamily::Moza);
        assert_eq!(r5_v1.kind(), DeviceCapabilityKind::Wheelbase);
        assert!(r5_v1.input());
        assert!(r5_v1.ffb_output());
        assert!(r5_v1.validated_stages().is_empty());

        assert_eq!(r5_v2.model(), "Moza R5 V2");
        assert!(!r5_v2.high_torque());
        assert!(!r5_v2.serial_config());
        assert!(!r5_v2.firmware_dfu());

        assert_eq!(srp.kind(), DeviceCapabilityKind::Pedals);
        assert!(srp.input());
        assert!(!srp.ffb_output());

        assert_eq!(hbp.kind(), DeviceCapabilityKind::Handbrake);
        assert!(hbp.input());
        assert!(!hbp.ffb_output());
        assert!(
            registry
                .records()
                .iter()
                .all(|record| record.validated_stages().is_empty())
        );
    }

    #[test]
    fn shifters_are_input_only_and_do_not_claim_high_torque() {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let hgp = registry.lookup(MOZA_VENDOR_ID, MOZA_HGP_SHIFTER_PID);
        let sgp = registry.lookup(MOZA_VENDOR_ID, MOZA_SGP_SHIFTER_PID);

        assert_eq!(hgp.kind(), DeviceCapabilityKind::Shifter);
        assert_eq!(sgp.kind(), DeviceCapabilityKind::Shifter);
        assert!(hgp.input());
        assert!(sgp.input());
        assert!(!hgp.ffb_output());
        assert!(!sgp.ffb_output());
        assert!(!hgp.high_torque());
        assert!(!sgp.high_torque());
    }

    #[test]
    fn unknown_devices_are_passive_only_by_default() {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let unknown = registry.lookup(0xCAFE, 0xBABE);

        assert_eq!(unknown.vendor_id(), 0xCAFE);
        assert_eq!(unknown.product_id(), 0xBABE);
        assert_eq!(unknown.family(), DeviceFamily::Unknown);
        assert_eq!(unknown.kind(), DeviceCapabilityKind::Unknown);
        assert!(unknown.input());
        assert!(!unknown.ffb_output());
        assert!(!unknown.serial_config());
        assert!(!unknown.firmware_dfu());
        assert!(!unknown.high_torque());
        assert!(unknown.validated_stages().is_empty());
    }

    #[test]
    fn real_lineage_can_attach_passive_validation_to_output_capable_record()
    -> Result<(), Box<dyn std::error::Error>> {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry
            .lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID)
            .with_validated_lineage(&real_passive_lineage()?)?;

        assert_eq!(
            record.validated_stages(),
            &[
                HardwareValidationStage::Enumerated,
                HardwareValidationStage::DescriptorTrusted,
                HardwareValidationStage::PassiveVerified,
            ]
        );
        assert!(!record.high_torque());
        Ok(())
    }

    #[test]
    fn real_enumeration_lineage_claims_only_enumeration() -> Result<(), Box<dyn std::error::Error>>
    {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry
            .lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID)
            .with_validated_lineage(&real_enumerated_lineage()?)?;

        assert_eq!(
            record.validated_stages(),
            &[HardwareValidationStage::Enumerated]
        );
        Ok(())
    }

    #[test]
    fn virtual_lineage_cannot_attach_validation() -> Result<(), EvidenceError> {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let result = registry
            .lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID)
            .with_validated_lineage(&virtual_passive_lineage()?);

        assert_eq!(
            result,
            Err(CapabilityRegistryError::NonRealHardwareEvidence {
                evidence_source: EvidenceSource::Virtual,
            })
        );
        Ok(())
    }

    #[test]
    fn synthetic_lineage_cannot_attach_validation() -> Result<(), EvidenceError> {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let result = registry
            .lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID)
            .with_validated_lineage(&synthetic_enumerated_lineage()?);

        assert_eq!(
            result,
            Err(CapabilityRegistryError::NonRealHardwareEvidence {
                evidence_source: EvidenceSource::Synthetic,
            })
        );
        Ok(())
    }

    #[test]
    fn disconnected_lineage_cannot_attach_validation() {
        let record = DeviceCapabilityRecord::unknown_passive(1, 2);
        let result = record.with_validated_lineage(&ValidationLineage::disconnected());

        assert_eq!(result, Err(CapabilityRegistryError::DisconnectedLineage));
    }

    #[test]
    fn input_only_record_rejects_output_validation_stages() -> Result<(), EvidenceError> {
        let srp = DeviceCapabilityRegistry::openracing_defaults()
            .lookup(MOZA_VENDOR_ID, MOZA_SRP_PEDALS_PID);
        let lineage = Disconnected::new()
            .enumerate(EnumerationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/device-list.json",
                MOZA_VENDOR_ID,
                MOZA_SRP_PEDALS_PID,
                "moza-srp",
            )?)
            .trust_descriptor(DescriptorTrustEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/descriptor.json",
            )?)
            .verify_passive(PassiveVerificationEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/passive-verification.json",
            )?)
            .verify_zero_output(crate::ZeroOutputEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/zero-verification.json",
            )?)
            .arm_low_torque(LowTorqueArmEvidence::new(
                EvidenceSource::RealHardware,
                "ci/hardware/moza-r5/2026-05-10/low-torque-arm.json",
            )?)
            .lineage()
            .clone();

        assert_eq!(
            srp.with_validated_lineage(&lineage),
            Err(
                CapabilityRegistryError::ValidatedOutputStageWithoutOutputCapability {
                    vendor_id: MOZA_VENDOR_ID,
                    product_id: MOZA_SRP_PEDALS_PID,
                }
            )
        );
        Ok(())
    }

    #[test]
    fn custom_registry_rejects_invalid_records() {
        let mut record = DeviceCapabilityRecord::unknown_passive(1, 2);
        record.model.clear();

        let result = DeviceCapabilityRegistry::new([record]);
        assert_eq!(
            result,
            Err(CapabilityRegistryError::EmptyModel {
                vendor_id: 1,
                product_id: 2,
            })
        );
    }

    #[test]
    fn capability_record_serializes_claim_ceiling_fields() -> Result<(), serde_json::Error> {
        let registry = DeviceCapabilityRegistry::openracing_defaults();
        let record = registry.lookup(MOZA_VENDOR_ID, MOZA_R5_V2_PID);

        let json = serde_json::to_string(&record)?;

        assert!(json.contains("\"vendor_id\":13422"));
        assert!(json.contains("\"product_id\":20"));
        assert!(json.contains("\"family\":\"moza\""));
        assert!(json.contains("\"model\":\"Moza R5 V2\""));
        assert!(json.contains("\"ffb_output\":true"));
        assert!(json.contains("\"high_torque\":false"));
        assert!(json.contains("\"validated_stages\":[]"));
        Ok(())
    }
}
