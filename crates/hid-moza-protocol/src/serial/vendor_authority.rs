//! Semantic-only Moza vendor authority registry.
//!
//! This module intentionally does not encode, send, or authorize vendor frames.
//! It pins command identity and risk policy before later codec/probe work.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MozaRiskClass {
    SafeObserve,
    VendorStatus,
    StandardPidff,
    VendorControlCandidate,
    VendorOutputCandidate,
    ConfigurationCandidate,
    FirmwareOrDfuForbidden,
    UnknownDoNotSend,
}

impl MozaRiskClass {
    pub const fn as_registry_str(self) -> &'static str {
        match self {
            Self::SafeObserve => "safe_observe",
            Self::VendorStatus => "vendor_status",
            Self::StandardPidff => "standard_pidff",
            Self::VendorControlCandidate => "vendor_control_candidate",
            Self::VendorOutputCandidate => "vendor_output_candidate",
            Self::ConfigurationCandidate => "configuration_candidate",
            Self::FirmwareOrDfuForbidden => "firmware_or_dfu_forbidden",
            Self::UnknownDoNotSend => "unknown_do_not_send",
        }
    }

    pub const fn is_encodable(self) -> bool {
        !matches!(self, Self::FirmwareOrDfuForbidden | Self::UnknownDoNotSend)
    }

    pub const fn can_send_without_exact_authorization(self) -> bool {
        matches!(
            self,
            Self::SafeObserve | Self::VendorStatus | Self::StandardPidff
        )
    }

    pub const fn requires_exact_authorization(self) -> bool {
        matches!(
            self,
            Self::VendorControlCandidate
                | Self::VendorOutputCandidate
                | Self::ConfigurationCandidate
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MozaSerialCodecStatus {
    SemanticOnly,
    FixtureDecodeOnly,
    RoundTripVerified,
    HardwareWriteEligible,
}

impl MozaSerialCodecStatus {
    pub const fn allows_hardware_writes(self) -> bool {
        matches!(self, Self::HardwareWriteEligible)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MozaVendorCommand {
    pub id: &'static str,
    pub family: &'static str,
    pub group: u8,
    pub device_id: u8,
    pub command: u8,
    pub name: &'static str,
    pub risk_class: MozaRiskClass,
    pub read_only_status_probe_allowed: bool,
}

pub const CODEC_STATUS: MozaSerialCodecStatus = MozaSerialCodecStatus::SemanticOnly;

pub const REQUIRED_VENDOR_COMMANDS: &[MozaVendorCommand] = &[
    MozaVendorCommand {
        id: "estop_set_ffb",
        family: "authority_state",
        group: 70,
        device_id: 28,
        command: 0,
        name: "EstopCtrl_SetFfb",
        risk_class: MozaRiskClass::VendorOutputCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "estop_get_ffb",
        family: "authority_state",
        group: 70,
        device_id: 28,
        command: 1,
        name: "EstopCtrl_GetFfb",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "main_misc_set_ffb_status",
        family: "authority_state",
        group: 33,
        device_id: 18,
        command: 6,
        name: "MainMiscCtrl_SetFfbStatus",
        risk_class: MozaRiskClass::VendorOutputCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "main_misc_get_ffb_status",
        family: "authority_state",
        group: 33,
        device_id: 18,
        command: 7,
        name: "MainMiscCtrl_GetFfbStatus",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "base_gain_set_overall_strength",
        family: "gain_safety",
        group: 41,
        device_id: 19,
        command: 2,
        name: "BaseGain_SetOverallStrength",
        risk_class: MozaRiskClass::ConfigurationCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "base_gain_get_overall_strength",
        family: "gain_safety",
        group: 40,
        device_id: 19,
        command: 2,
        name: "BaseGain_GetOverallStrength",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "base_gain_set_speed_dependent_damping",
        family: "gain_safety",
        group: 41,
        device_id: 19,
        command: 13,
        name: "BaseGain_SetSpeedDependentDamping",
        risk_class: MozaRiskClass::ConfigurationCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "base_gain_get_speed_dependent_damping",
        family: "gain_safety",
        group: 40,
        device_id: 19,
        command: 13,
        name: "BaseGain_GetSpeedDependentDamping",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "base_gain_set_hands_off_protection",
        family: "gain_safety",
        group: 41,
        device_id: 19,
        command: 18,
        name: "BaseGain_SetHandsOffProtection",
        risk_class: MozaRiskClass::ConfigurationCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "base_gain_get_hands_off_protection",
        family: "gain_safety",
        group: 40,
        device_id: 19,
        command: 18,
        name: "BaseGain_GetHandsOffProtection",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "temperature_get_mosfet",
        family: "temperatures",
        group: 43,
        device_id: 19,
        command: 4,
        name: "Temperature_GetMosfet",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "temperature_get_motor",
        family: "temperatures",
        group: 43,
        device_id: 19,
        command: 5,
        name: "Temperature_GetMotor",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "temperature_get_board",
        family: "temperatures",
        group: 43,
        device_id: 19,
        command: 6,
        name: "Temperature_GetBoard",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
    MozaVendorCommand {
        id: "compatibility_set_mode",
        family: "compatibility_mode",
        group: 31,
        device_id: 18,
        command: 19,
        name: "Compatibility_SetMode",
        risk_class: MozaRiskClass::ConfigurationCandidate,
        read_only_status_probe_allowed: false,
    },
    MozaVendorCommand {
        id: "compatibility_get_mode",
        family: "compatibility_mode",
        group: 31,
        device_id: 18,
        command: 23,
        name: "Compatibility_GetMode",
        risk_class: MozaRiskClass::VendorStatus,
        read_only_status_probe_allowed: true,
    },
];

pub const FORBIDDEN_VENDOR_CLASSES: &[(&str, MozaRiskClass)] = &[
    ("group_10_eeprom", MozaRiskClass::ConfigurationCandidate),
    ("firmware_or_dfu", MozaRiskClass::FirmwareOrDfuForbidden),
    ("hid_report_0xaf", MozaRiskClass::UnknownDoNotSend),
    ("unknown_host_to_device", MozaRiskClass::UnknownDoNotSend),
];

pub fn command_by_group_command(group: u8, command: u8) -> Option<&'static MozaVendorCommand> {
    REQUIRED_VENDOR_COMMANDS
        .iter()
        .find(|candidate| candidate.group == group && candidate.command == command)
}
