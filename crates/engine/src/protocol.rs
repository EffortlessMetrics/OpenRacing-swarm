//! OWP-1 (Open Wheel Protocol) v0 Specification
//!
//! Implements the racing wheel communication protocol as specified in [ADR-0003: OWP-1 Protocol Specification].
//!
//! [ADR-0003]: file:///h:/Code/Rust/OpenRacing/docs/adr/0003-owp1-protocol.md
//!
//! This module implements the OWP-1 protocol for communication with racing wheel hardware.
//! The protocol uses HID reports for bidirectional communication with endian-safe structures,
//! sequence numbers, and CRC validation.

/// OWP-1 Protocol version
pub const OWP1_VERSION: u8 = 0;

/// HID Report IDs used by OWP-1
pub mod report_ids {
    /// Feature Report: Device Capabilities
    pub const CAPABILITIES: u8 = 0x01;

    /// Feature Report: Device Configuration
    pub const CONFIGURATION: u8 = 0x02;

    /// OUT Report: Torque Command (Host -> Device)
    pub const TORQUE_COMMAND: u8 = 0x20;

    /// IN Report: Device Telemetry (Device -> Host)
    pub const DEVICE_TELEMETRY: u8 = 0x21;

    /// IN Report: Configuration Acknowledgment
    pub const CONFIG_ACK: u8 = 0x22;

    /// Feature Report: Safety Interlock Challenge
    pub const SAFETY_CHALLENGE: u8 = 0x03;

    /// IN Report: Safety Interlock Acknowledgment
    pub const SAFETY_ACK: u8 = 0x23;
}

/// Fault flags for device telemetry
pub mod fault_flags {
    /// USB communication fault
    pub const USB_FAULT: u8 = 0x01;

    /// Encoder fault (NaN values, disconnection)
    pub const ENCODER_FAULT: u8 = 0x02;

    /// Thermal limit exceeded
    pub const THERMAL_FAULT: u8 = 0x04;

    /// Overcurrent protection triggered
    pub const OVERCURRENT_FAULT: u8 = 0x08;

    /// Power supply fault
    pub const POWER_FAULT: u8 = 0x10;

    /// Firmware fault
    pub const FIRMWARE_FAULT: u8 = 0x20;

    /// Safety interlock fault
    pub const SAFETY_FAULT: u8 = 0x40;

    /// Generic hardware fault
    pub const HARDWARE_FAULT: u8 = 0x80;
}

/// Torque command flags
pub mod torque_flags {
    /// Hands-on hint from host
    pub const HANDS_ON_HINT: u8 = 0x01;

    /// Saturation warning
    pub const SATURATION_WARNING: u8 = 0x02;

    /// Emergency stop
    pub const EMERGENCY_STOP: u8 = 0x04;

    /// High torque mode enabled
    pub const HIGH_TORQUE_MODE: u8 = 0x08;
}

/// HID OUT Report 0x20 - Torque Command (Host -> Device, 1kHz)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TorqueCommand {
    /// Report ID (0x20)
    pub report_id: u8,

    /// Torque value in milliNewton-meters
    /// Range: -32.768 to +32.767 Nm
    pub torque_mnm: i16,

    /// Command flags (see torque_flags module)
    pub flags: u8,

    /// Sequence number (wraps at 65535)
    pub sequence: u16,

    /// CRC8 checksum of the payload
    pub crc8: u8,
}

impl TorqueCommand {
    /// Create a new torque command
    pub fn new(torque_nm: f32, flags: u8, sequence: u16) -> Self {
        // Convert torque from Nm to milliNewton-meters
        let torque_mnm = (torque_nm * 1000.0).clamp(-32768.0, 32767.0) as i16;

        let mut cmd = Self {
            report_id: report_ids::TORQUE_COMMAND,
            torque_mnm,
            flags,
            sequence,
            crc8: 0,
        };

        // Calculate CRC8 over the payload (excluding report_id and crc8)
        cmd.crc8 = Self::calculate_crc8(&cmd);
        cmd
    }

    /// Get torque value in Newton-meters
    pub fn torque_nm(&self) -> f32 {
        (self.torque_mnm as f32) / 1000.0
    }

    /// Validate the CRC8 checksum
    pub fn validate_crc(&self) -> bool {
        let expected_crc = Self::calculate_crc8(self);
        self.crc8 == expected_crc
    }

    /// Calculate CRC8 checksum over the payload
    fn calculate_crc8(cmd: &Self) -> u8 {
        let payload = [
            (cmd.torque_mnm & 0xFF) as u8,
            (cmd.torque_mnm >> 8) as u8,
            cmd.flags,
            (cmd.sequence & 0xFF) as u8,
            (cmd.sequence >> 8) as u8,
        ];

        crc8(&payload)
    }

    /// Convert to byte array for HID transmission
    pub fn to_bytes(&self) -> [u8; 7] {
        let [torque_lo, torque_hi] = self.torque_mnm.to_le_bytes();
        let [sequence_lo, sequence_hi] = self.sequence.to_le_bytes();
        [
            self.report_id,
            torque_lo,
            torque_hi,
            self.flags,
            sequence_lo,
            sequence_hi,
            self.crc8,
        ]
    }

    /// Create from byte array received from HID
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let [
            report_id,
            torque_lo,
            torque_hi,
            flags,
            sequence_lo,
            sequence_hi,
            crc8,
            ..,
        ] = bytes
        else {
            return Err("Torque command too short".to_string());
        };

        let cmd = Self {
            report_id: *report_id,
            torque_mnm: i16::from_le_bytes([*torque_lo, *torque_hi]),
            flags: *flags,
            sequence: u16::from_le_bytes([*sequence_lo, *sequence_hi]),
            crc8: *crc8,
        };

        if cmd.report_id != report_ids::TORQUE_COMMAND {
            return Err(format!(
                "Invalid report ID: expected 0x{:02x}, got 0x{:02x}",
                report_ids::TORQUE_COMMAND,
                cmd.report_id
            ));
        }

        if !cmd.validate_crc() {
            return Err("CRC validation failed".to_string());
        }

        Ok(cmd)
    }
}

/// HID IN Report 0x21 - Device Telemetry (Device -> Host, 60-200Hz)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceTelemetryReport {
    /// Report ID (0x21)
    pub report_id: u8,

    /// Wheel angle in millidegrees
    pub wheel_angle_mdeg: i32,

    /// Wheel speed in milliradians per second
    pub wheel_speed_mrad_s: i16,

    /// Temperature in Celsius
    pub temp_c: u8,

    /// Fault flags (see fault_flags module)
    pub faults: u8,

    /// Hands-on detection (0 = off, 1 = on, 255 = unknown)
    pub hands_on: u8,

    /// Sequence number from last torque command
    pub last_torque_seq: u16,

    /// CRC8 checksum of the payload
    pub crc8: u8,
}

impl DeviceTelemetryReport {
    /// Create a new telemetry report
    pub fn new(
        wheel_angle_deg: f32,
        wheel_speed_rad_s: f32,
        temp_c: u8,
        faults: u8,
        hands_on: bool,
        last_torque_seq: u16,
    ) -> Self {
        let wheel_angle_mdeg =
            (wheel_angle_deg * 1000.0).clamp(i32::MIN as f32, i32::MAX as f32) as i32;
        let wheel_speed_mrad_s = (wheel_speed_rad_s * 1000.0).clamp(-32768.0, 32767.0) as i16;
        let hands_on_val = if hands_on { 1 } else { 0 };

        let mut report = Self {
            report_id: report_ids::DEVICE_TELEMETRY,
            wheel_angle_mdeg,
            wheel_speed_mrad_s,
            temp_c,
            faults,
            hands_on: hands_on_val,
            last_torque_seq,
            crc8: 0,
        };

        // Calculate CRC8 over the payload
        report.crc8 = Self::calculate_crc8(&report);
        report
    }

    /// Get wheel angle in degrees
    pub fn wheel_angle_deg(&self) -> f32 {
        (self.wheel_angle_mdeg as f32) / 1000.0
    }

    /// Get wheel speed in radians per second
    pub fn wheel_speed_rad_s(&self) -> f32 {
        (self.wheel_speed_mrad_s as f32) / 1000.0
    }

    /// Check if hands are detected on the wheel
    pub fn hands_on(&self) -> Option<bool> {
        match self.hands_on {
            0 => Some(false),
            1 => Some(true),
            _ => None, // Unknown
        }
    }

    /// Get temperature in Celsius (new-name accessor for the wire-level field)
    pub fn temperature_c(&self) -> u8 {
        let Self { temp_c: val, .. } = *self;
        val
    }

    /// Get fault flags (new-name accessor for the wire-level field)
    pub fn fault_flags(&self) -> u8 {
        let Self { faults: val, .. } = *self;
        val
    }

    /// Check if any faults are present
    pub fn has_faults(&self) -> bool {
        self.faults != 0
    }

    /// Validate the CRC8 checksum
    pub fn validate_crc(&self) -> bool {
        let expected_crc = Self::calculate_crc8(self);
        self.crc8 == expected_crc
    }

    /// Calculate CRC8 checksum over the payload
    fn calculate_crc8(report: &Self) -> u8 {
        let payload = [
            (report.wheel_angle_mdeg & 0xFF) as u8,
            ((report.wheel_angle_mdeg >> 8) & 0xFF) as u8,
            ((report.wheel_angle_mdeg >> 16) & 0xFF) as u8,
            ((report.wheel_angle_mdeg >> 24) & 0xFF) as u8,
            (report.wheel_speed_mrad_s & 0xFF) as u8,
            (report.wheel_speed_mrad_s >> 8) as u8,
            report.temp_c,
            report.faults,
            report.hands_on,
            (report.last_torque_seq & 0xFF) as u8,
            (report.last_torque_seq >> 8) as u8,
        ];

        crc8(&payload)
    }

    /// Convert to byte array for HID transmission
    pub fn to_bytes(&self) -> [u8; 13] {
        let Self {
            report_id,
            wheel_angle_mdeg,
            wheel_speed_mrad_s,
            temp_c,
            faults,
            hands_on,
            last_torque_seq,
            crc8,
        } = *self;
        let [angle_0, angle_1, angle_2, angle_3] = wheel_angle_mdeg.to_le_bytes();
        let [speed_0, speed_1] = wheel_speed_mrad_s.to_le_bytes();
        let [seq_0, seq_1] = last_torque_seq.to_le_bytes();
        [
            report_id, angle_0, angle_1, angle_2, angle_3, speed_0, speed_1, temp_c, faults,
            hands_on, seq_0, seq_1, crc8,
        ]
    }

    /// Create from byte array received from HID
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let [
            report_id,
            angle_0,
            angle_1,
            angle_2,
            angle_3,
            speed_0,
            speed_1,
            temp_c,
            faults,
            hands_on,
            seq_0,
            seq_1,
            crc8,
            ..,
        ] = bytes
        else {
            return Err("Telemetry report too short".to_string());
        };

        let report = Self {
            report_id: *report_id,
            wheel_angle_mdeg: i32::from_le_bytes([*angle_0, *angle_1, *angle_2, *angle_3]),
            wheel_speed_mrad_s: i16::from_le_bytes([*speed_0, *speed_1]),
            temp_c: *temp_c,
            faults: *faults,
            hands_on: *hands_on,
            last_torque_seq: u16::from_le_bytes([*seq_0, *seq_1]),
            crc8: *crc8,
        };

        if report.report_id != report_ids::DEVICE_TELEMETRY {
            return Err(format!(
                "Invalid report ID: expected 0x{:02x}, got 0x{:02x}",
                report_ids::DEVICE_TELEMETRY,
                report.report_id
            ));
        }

        if !report.validate_crc() {
            return Err("CRC validation failed".to_string());
        }

        Ok(report)
    }
}

/// Feature Report 0x01 - Device Capabilities
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceCapabilitiesReport {
    /// Report ID (0x01)
    pub report_id: u8,

    /// Capability flags
    pub flags: u8,

    /// Maximum torque in centi-Newton-meters
    pub max_torque_cnm: u16,

    /// Encoder counts per revolution
    pub encoder_cpr: u16,

    /// Minimum report period in microseconds
    pub min_report_period_us: u16,

    /// Protocol version
    pub protocol_version: u8,
}

impl DeviceCapabilitiesReport {
    /// Capability flag: Supports HID PID
    pub const SUPPORTS_PID: u8 = 0x01;

    /// Capability flag: Supports raw torque at 1kHz
    pub const SUPPORTS_RAW_TORQUE_1KHZ: u8 = 0x02;

    /// Capability flag: Supports health stream
    pub const SUPPORTS_HEALTH_STREAM: u8 = 0x04;

    /// Capability flag: Supports LED bus
    pub const SUPPORTS_LED_BUS: u8 = 0x08;

    /// Create a new capabilities report
    pub fn new(
        supports_pid: bool,
        supports_raw_torque_1khz: bool,
        supports_health_stream: bool,
        supports_led_bus: bool,
        max_torque_cnm: u16,
        encoder_cpr: u16,
        min_report_period_us: u16,
    ) -> Self {
        let mut flags = 0u8;
        if supports_pid {
            flags |= Self::SUPPORTS_PID;
        }
        if supports_raw_torque_1khz {
            flags |= Self::SUPPORTS_RAW_TORQUE_1KHZ;
        }
        if supports_health_stream {
            flags |= Self::SUPPORTS_HEALTH_STREAM;
        }
        if supports_led_bus {
            flags |= Self::SUPPORTS_LED_BUS;
        }

        Self {
            report_id: report_ids::CAPABILITIES,
            flags,
            max_torque_cnm,
            encoder_cpr,
            min_report_period_us,
            protocol_version: OWP1_VERSION,
        }
    }

    /// Check if device supports PID
    pub fn supports_pid(&self) -> bool {
        (self.flags & Self::SUPPORTS_PID) != 0
    }

    /// Check if device supports raw torque at 1kHz
    pub fn supports_raw_torque_1khz(&self) -> bool {
        (self.flags & Self::SUPPORTS_RAW_TORQUE_1KHZ) != 0
    }

    /// Check if device supports health stream
    pub fn supports_health_stream(&self) -> bool {
        (self.flags & Self::SUPPORTS_HEALTH_STREAM) != 0
    }

    /// Check if device supports LED bus
    pub fn supports_led_bus(&self) -> bool {
        (self.flags & Self::SUPPORTS_LED_BUS) != 0
    }

    /// Get maximum torque in Newton-meters
    pub fn max_torque_nm(&self) -> f32 {
        (self.max_torque_cnm as f32) / 100.0
    }

    /// Get maximum update rate in Hz
    pub fn max_update_rate_hz(&self) -> f32 {
        1_000_000.0 / (self.min_report_period_us as f32)
    }

    /// Convert to byte array for HID transmission
    pub fn to_bytes(&self) -> [u8; 9] {
        let [max_torque_0, max_torque_1] = self.max_torque_cnm.to_le_bytes();
        let [encoder_0, encoder_1] = self.encoder_cpr.to_le_bytes();
        let [period_0, period_1] = self.min_report_period_us.to_le_bytes();
        [
            self.report_id,
            self.flags,
            max_torque_0,
            max_torque_1,
            encoder_0,
            encoder_1,
            period_0,
            period_1,
            self.protocol_version,
        ]
    }

    /// Create from byte array received from HID
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let [
            report_id,
            flags,
            max_torque_0,
            max_torque_1,
            encoder_0,
            encoder_1,
            period_0,
            period_1,
            protocol_version,
            ..,
        ] = bytes
        else {
            return Err("Capabilities report too short".to_string());
        };

        let report = Self {
            report_id: *report_id,
            flags: *flags,
            max_torque_cnm: u16::from_le_bytes([*max_torque_0, *max_torque_1]),
            encoder_cpr: u16::from_le_bytes([*encoder_0, *encoder_1]),
            min_report_period_us: u16::from_le_bytes([*period_0, *period_1]),
            protocol_version: *protocol_version,
        };

        if report.report_id != report_ids::CAPABILITIES {
            return Err(format!(
                "Invalid report ID: expected 0x{:02x}, got 0x{:02x}",
                report_ids::CAPABILITIES,
                report.report_id
            ));
        }

        Ok(report)
    }
}

/// Feature Report 0x03 - Safety Interlock Challenge
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyInterlockChallenge {
    /// Report ID (0x03)
    pub report_id: u8,

    /// Challenge token
    pub challenge_token: u32,

    /// Required button combo (0 = both clutch paddles, 1+ = custom)
    pub combo_type: u8,

    /// Required hold duration in milliseconds
    pub hold_duration_ms: u16,

    /// Challenge expires at (Unix timestamp, seconds since epoch)
    pub expires_unix_secs: u32,
}

impl SafetyInterlockChallenge {
    /// Both clutch paddles combo type
    pub const COMBO_BOTH_CLUTCH: u8 = 0;

    /// Custom combo type (device-specific)
    pub const COMBO_CUSTOM: u8 = 1;

    /// Create a new safety interlock challenge
    pub fn new(
        challenge_token: u32,
        combo_type: u8,
        hold_duration_ms: u16,
        expires_unix_secs: u32,
    ) -> Self {
        Self {
            report_id: report_ids::SAFETY_CHALLENGE,
            challenge_token,
            combo_type,
            hold_duration_ms,
            expires_unix_secs,
        }
    }

    /// Convert to byte array for HID transmission
    pub fn to_bytes(&self) -> [u8; 12] {
        let [challenge_0, challenge_1, challenge_2, challenge_3] =
            self.challenge_token.to_le_bytes();
        let [hold_0, hold_1] = self.hold_duration_ms.to_le_bytes();
        let [expires_0, expires_1, expires_2, expires_3] = self.expires_unix_secs.to_le_bytes();
        [
            self.report_id,
            challenge_0,
            challenge_1,
            challenge_2,
            challenge_3,
            self.combo_type,
            hold_0,
            hold_1,
            expires_0,
            expires_1,
            expires_2,
            expires_3,
        ]
    }

    /// Create from byte array received from HID
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let [
            report_id,
            challenge_0,
            challenge_1,
            challenge_2,
            challenge_3,
            combo_type,
            hold_0,
            hold_1,
            expires_0,
            expires_1,
            expires_2,
            expires_3,
            ..,
        ] = bytes
        else {
            return Err("Safety challenge too short".to_string());
        };

        let challenge = Self {
            report_id: *report_id,
            challenge_token: u32::from_le_bytes([
                *challenge_0,
                *challenge_1,
                *challenge_2,
                *challenge_3,
            ]),
            combo_type: *combo_type,
            hold_duration_ms: u16::from_le_bytes([*hold_0, *hold_1]),
            expires_unix_secs: u32::from_le_bytes([*expires_0, *expires_1, *expires_2, *expires_3]),
        };

        if challenge.report_id != report_ids::SAFETY_CHALLENGE {
            return Err(format!(
                "Invalid report ID: expected 0x{:02x}, got 0x{:02x}",
                report_ids::SAFETY_CHALLENGE,
                challenge.report_id
            ));
        }

        Ok(challenge)
    }
}

/// IN Report 0x23 - Safety Interlock Acknowledgment (Device -> Host)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyInterlockAck {
    /// Report ID (0x23)
    pub report_id: u8,

    /// Challenge token being acknowledged
    pub challenge_token: u32,

    /// Device-generated token (persists until power cycle)
    pub device_token: u32,

    /// Combo type that was completed
    pub combo_completed: u8,

    /// Actual hold duration in milliseconds
    pub actual_hold_duration_ms: u16,

    /// Timestamp when combo was completed (device ticks)
    pub completion_timestamp: u32,

    /// CRC8 checksum of the payload
    pub crc8: u8,
}

impl SafetyInterlockAck {
    /// Create a new safety interlock acknowledgment
    pub fn new(
        challenge_token: u32,
        device_token: u32,
        combo_completed: u8,
        actual_hold_duration_ms: u16,
        completion_timestamp: u32,
    ) -> Self {
        let mut ack = Self {
            report_id: report_ids::SAFETY_ACK,
            challenge_token,
            device_token,
            combo_completed,
            actual_hold_duration_ms,
            completion_timestamp,
            crc8: 0,
        };

        // Calculate CRC8 over the payload
        ack.crc8 = Self::calculate_crc8(&ack);
        ack
    }

    /// Validate the CRC8 checksum
    pub fn validate_crc(&self) -> bool {
        let expected_crc = Self::calculate_crc8(self);
        self.crc8 == expected_crc
    }

    /// Calculate CRC8 checksum over the payload
    fn calculate_crc8(ack: &Self) -> u8 {
        let payload = [
            (ack.challenge_token & 0xFF) as u8,
            ((ack.challenge_token >> 8) & 0xFF) as u8,
            ((ack.challenge_token >> 16) & 0xFF) as u8,
            ((ack.challenge_token >> 24) & 0xFF) as u8,
            (ack.device_token & 0xFF) as u8,
            ((ack.device_token >> 8) & 0xFF) as u8,
            ((ack.device_token >> 16) & 0xFF) as u8,
            ((ack.device_token >> 24) & 0xFF) as u8,
            ack.combo_completed,
            (ack.actual_hold_duration_ms & 0xFF) as u8,
            (ack.actual_hold_duration_ms >> 8) as u8,
            (ack.completion_timestamp & 0xFF) as u8,
            ((ack.completion_timestamp >> 8) & 0xFF) as u8,
            ((ack.completion_timestamp >> 16) & 0xFF) as u8,
            ((ack.completion_timestamp >> 24) & 0xFF) as u8,
        ];

        crc8(&payload)
    }

    /// Convert to byte array for HID transmission
    pub fn to_bytes(&self) -> [u8; 17] {
        let [challenge_0, challenge_1, challenge_2, challenge_3] =
            self.challenge_token.to_le_bytes();
        let [device_0, device_1, device_2, device_3] = self.device_token.to_le_bytes();
        let [hold_0, hold_1] = self.actual_hold_duration_ms.to_le_bytes();
        let [completed_0, completed_1, completed_2, completed_3] =
            self.completion_timestamp.to_le_bytes();
        [
            self.report_id,
            challenge_0,
            challenge_1,
            challenge_2,
            challenge_3,
            device_0,
            device_1,
            device_2,
            device_3,
            self.combo_completed,
            hold_0,
            hold_1,
            completed_0,
            completed_1,
            completed_2,
            completed_3,
            self.crc8,
        ]
    }

    /// Create from byte array received from HID
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let [
            report_id,
            challenge_0,
            challenge_1,
            challenge_2,
            challenge_3,
            device_0,
            device_1,
            device_2,
            device_3,
            combo_completed,
            hold_0,
            hold_1,
            completed_0,
            completed_1,
            completed_2,
            completed_3,
            crc8,
            ..,
        ] = bytes
        else {
            return Err("Safety acknowledgment too short".to_string());
        };

        let ack = Self {
            report_id: *report_id,
            challenge_token: u32::from_le_bytes([
                *challenge_0,
                *challenge_1,
                *challenge_2,
                *challenge_3,
            ]),
            device_token: u32::from_le_bytes([*device_0, *device_1, *device_2, *device_3]),
            combo_completed: *combo_completed,
            actual_hold_duration_ms: u16::from_le_bytes([*hold_0, *hold_1]),
            completion_timestamp: u32::from_le_bytes([
                *completed_0,
                *completed_1,
                *completed_2,
                *completed_3,
            ]),
            crc8: *crc8,
        };

        if ack.report_id != report_ids::SAFETY_ACK {
            return Err(format!(
                "Invalid report ID: expected 0x{:02x}, got 0x{:02x}",
                report_ids::SAFETY_ACK,
                ack.report_id
            ));
        }

        if !ack.validate_crc() {
            return Err("CRC validation failed".to_string());
        }

        Ok(ack)
    }
}

/// Simple CRC8 implementation for OWP-1 protocol
/// Uses polynomial 0x07 (x^8 + x^2 + x + 1)
fn crc8(data: &[u8]) -> u8 {
    const CRC8_POLY: u8 = 0x07;
    let mut crc = 0u8;

    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ CRC8_POLY;
            } else {
                crc <<= 1;
            }
        }
    }

    crc
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::mem;

    /// Golden test data for OWP-1 protocol validation
    #[allow(dead_code)]
    mod golden_data {
        /// Golden torque command: 10.5 Nm, hands-on hint, sequence 1234
        pub const TORQUE_CMD_GOLDEN: [u8; 7] = [
            0x20, // Report ID
            0x04, 0x29, // 10500 mNm (10.5 Nm) in little-endian
            0x01, // Hands-on hint flag
            0xD2, 0x04, // Sequence 1234 in little-endian
            0x00, // CRC8 (will be calculated)
        ];

        /// Golden telemetry report: 45.5°, 2.5 rad/s, 42°C, no faults, hands-on, seq 1234
        pub const TELEMETRY_GOLDEN: [u8; 13] = [
            0x21, // Report ID
            0xBC, 0xB1, 0x00, 0x00, // 45500 mdeg (45.5°) in little-endian
            0xC4, 0x09, // 2500 mrad/s (2.5 rad/s) in little-endian
            0x2A, // 42°C
            0x00, // No faults
            0x01, // Hands on
            0xD2, 0x04, // Last torque sequence 1234
            0x00, // CRC8 (will be calculated)
        ];

        /// Golden capabilities report: All features, 25 Nm, 10000 CPR, 1kHz
        pub const CAPABILITIES_GOLDEN: [u8; 9] = [
            0x01, // Report ID
            0x0F, // All capability flags set
            0xC4, 0x09, // 2500 cNm (25.0 Nm) in little-endian
            0x10, 0x27, // 10000 CPR in little-endian
            0xE8, 0x03, // 1000 µs (1kHz) in little-endian
            0x00, // Protocol version 0
        ];
    }

    #[test]
    fn test_torque_command_golden() {
        // Test creating a torque command and verifying round-trip consistency
        let cmd = TorqueCommand::new(10.5, torque_flags::HANDS_ON_HINT, 1234);
        let bytes = cmd.to_bytes();

        // Verify the structure is correct
        assert_eq!(bytes[0], report_ids::TORQUE_COMMAND);
        assert_eq!(cmd.torque_nm(), 10.5);
        assert_eq!(cmd.flags, torque_flags::HANDS_ON_HINT);
        assert!(cmd.validate_crc());

        // Test parsing the generated data
        let parsed = TorqueCommand::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.torque_nm(), 10.5);
        assert_eq!(parsed.flags, torque_flags::HANDS_ON_HINT);
        let sequence = parsed.sequence;
        assert_eq!(sequence, 1234);
        assert!(parsed.validate_crc());
    }

    #[test]
    fn test_telemetry_report_golden() {
        // Test creating a telemetry report and verifying round-trip consistency
        let report = DeviceTelemetryReport::new(45.5, 2.5, 42, 0, true, 1234);
        let bytes = report.to_bytes();

        // Verify the structure is correct
        assert_eq!(bytes[0], report_ids::DEVICE_TELEMETRY);
        assert_eq!(report.wheel_angle_deg(), 45.5);
        assert_eq!(report.wheel_speed_rad_s(), 2.5);
        assert_eq!(report.temp_c, 42);
        assert!(report.validate_crc());

        // Test parsing the generated data
        let parsed = DeviceTelemetryReport::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.wheel_angle_deg(), 45.5);
        assert_eq!(parsed.wheel_speed_rad_s(), 2.5);
        assert_eq!(parsed.temp_c, 42);
        assert_eq!(parsed.faults, 0);
        assert_eq!(parsed.hands_on(), Some(true));
        let last_torque_seq = parsed.last_torque_seq;
        assert_eq!(last_torque_seq, 1234);
        assert!(parsed.validate_crc());
    }

    #[test]
    fn test_capabilities_report_golden() {
        // Test creating a capabilities report and verifying round-trip consistency
        let report = DeviceCapabilitiesReport::new(true, true, true, true, 2500, 10000, 1000);
        let bytes = report.to_bytes();

        // Verify the structure is correct
        assert_eq!(bytes[0], report_ids::CAPABILITIES);
        assert!(report.supports_pid());
        assert!(report.supports_raw_torque_1khz());
        assert!(report.supports_health_stream());
        assert!(report.supports_led_bus());
        assert_eq!(report.max_torque_nm(), 25.0);

        // Test parsing the generated data
        let parsed = DeviceCapabilitiesReport::from_bytes(&bytes).unwrap();
        assert!(parsed.supports_pid());
        assert!(parsed.supports_raw_torque_1khz());
        assert!(parsed.supports_health_stream());
        assert!(parsed.supports_led_bus());
        assert_eq!(parsed.max_torque_nm(), 25.0);
        let encoder_cpr = parsed.encoder_cpr;
        let min_report_period_us = parsed.min_report_period_us;
        assert_eq!(encoder_cpr, 10000);
        assert_eq!(min_report_period_us, 1000);
        assert_eq!(parsed.max_update_rate_hz(), 1000.0);
    }

    #[test]
    fn test_torque_command_validation() {
        let cmd = TorqueCommand::new(5.0, 0, 100);
        assert!(cmd.validate_crc());

        // Test with corrupted data
        let mut bytes = cmd.to_bytes();
        bytes[1] = 0xFF; // Corrupt torque value
        let corrupted = TorqueCommand::from_bytes(&bytes);
        assert!(corrupted.is_err());
    }

    #[test]
    fn test_telemetry_report_validation() {
        let report = DeviceTelemetryReport::new(0.0, 0.0, 25, 0, false, 0);
        assert!(report.validate_crc());

        // Test with corrupted data
        let mut bytes = report.to_bytes();
        bytes[5] = 0xFF; // Corrupt wheel speed
        let corrupted = DeviceTelemetryReport::from_bytes(&bytes);
        assert!(corrupted.is_err());
    }

    #[test]
    fn test_torque_command_range_limits() {
        // i16 range is -32768 to 32767 mNm, i.e., ±32.768 Nm max

        // Value well within positive range: 10 Nm = 10000 mNm
        let cmd_normal = TorqueCommand::new(10.0, 0, 0);
        let torque_normal = cmd_normal.torque_mnm;
        assert_eq!(torque_normal, 10000);

        // Value well within negative range: -15 Nm = -15000 mNm
        let cmd_neg = TorqueCommand::new(-15.0, 0, 0);
        let torque_neg = cmd_neg.torque_mnm;
        assert_eq!(torque_neg, -15000);

        // Clamp above positive range: 50 Nm should clamp to 32767 mNm
        let cmd_over = TorqueCommand::new(50.0, 0, 0);
        let torque_over = cmd_over.torque_mnm;
        assert_eq!(torque_over, 32767);

        // Clamp below negative range: -50 Nm should clamp to -32768 mNm
        let cmd_under = TorqueCommand::new(-50.0, 0, 0);
        let torque_under = cmd_under.torque_mnm;
        assert_eq!(torque_under, -32768);
    }

    #[test]
    fn test_telemetry_hands_on_detection() {
        let report_on = DeviceTelemetryReport::new(0.0, 0.0, 25, 0, true, 0);
        assert_eq!(report_on.hands_on(), Some(true));

        let report_off = DeviceTelemetryReport::new(0.0, 0.0, 25, 0, false, 0);
        assert_eq!(report_off.hands_on(), Some(false));

        // Test unknown state
        let mut report_unknown = report_off;
        report_unknown.hands_on = 255;
        assert_eq!(report_unknown.hands_on(), None);
    }

    #[test]
    fn test_fault_flags() {
        let report = DeviceTelemetryReport::new(
            0.0,
            0.0,
            85,
            fault_flags::THERMAL_FAULT | fault_flags::ENCODER_FAULT,
            true,
            0,
        );

        assert!(report.has_faults());
        assert_eq!(
            report.faults & fault_flags::THERMAL_FAULT,
            fault_flags::THERMAL_FAULT
        );
        assert_eq!(
            report.faults & fault_flags::ENCODER_FAULT,
            fault_flags::ENCODER_FAULT
        );
        assert_eq!(report.faults & fault_flags::USB_FAULT, 0);
    }

    #[test]
    fn test_crc8_implementation() {
        // Test basic CRC8 properties
        assert_eq!(crc8(&[]), 0);
        assert_eq!(crc8(&[0x00]), 0);

        // Test that different data produces different CRCs
        let data1 = [0x01, 0x02, 0x03];
        let data2 = [0x01, 0x02, 0x04];
        assert_ne!(crc8(&data1), crc8(&data2));

        // Test that the same data produces the same CRC
        assert_eq!(crc8(&data1), crc8(&data1));

        // Test with some known patterns
        let test_data = [0xAA, 0x55, 0xFF, 0x00];
        let crc1 = crc8(&test_data);
        let crc2 = crc8(&test_data);
        assert_eq!(crc1, crc2);
    }

    #[test]
    fn test_endian_safety() {
        // Test that our protocol structures work correctly on little-endian systems
        let cmd = TorqueCommand::new(12.345, 0x05, 0x1234);
        let bytes = cmd.to_bytes();
        let parsed = TorqueCommand::from_bytes(&bytes).unwrap();

        assert_eq!(cmd.torque_nm(), parsed.torque_nm());
        assert_eq!(cmd.flags, parsed.flags);
        let cmd_seq = cmd.sequence;
        let parsed_seq = parsed.sequence;
        assert_eq!(cmd_seq, parsed_seq);
    }

    #[test]
    fn test_report_size_constraints() {
        // Ensure our structures have the expected sizes for HID reports
        assert_eq!(mem::size_of::<TorqueCommand>(), 7);
        assert_eq!(mem::size_of::<DeviceTelemetryReport>(), 13);
        assert_eq!(mem::size_of::<DeviceCapabilitiesReport>(), 9);
        assert_eq!(mem::size_of::<SafetyInterlockChallenge>(), 12);
        assert_eq!(mem::size_of::<SafetyInterlockAck>(), 17);
    }

    #[test]
    fn test_safety_interlock_challenge() {
        let challenge = SafetyInterlockChallenge::new(
            0x12345678,
            SafetyInterlockChallenge::COMBO_BOTH_CLUTCH,
            2000,
            1640995200, // 2022-01-01 00:00:00 UTC
        );

        let bytes = challenge.to_bytes();
        assert_eq!(bytes[0], report_ids::SAFETY_CHALLENGE);

        let parsed = SafetyInterlockChallenge::from_bytes(&bytes).unwrap();
        // Copy fields to avoid packed struct alignment issues
        let challenge_token = parsed.challenge_token;
        let combo_type = parsed.combo_type;
        let hold_duration_ms = parsed.hold_duration_ms;
        let expires_unix_secs = parsed.expires_unix_secs;

        assert_eq!(challenge_token, 0x12345678);
        assert_eq!(combo_type, SafetyInterlockChallenge::COMBO_BOTH_CLUTCH);
        assert_eq!(hold_duration_ms, 2000);
        assert_eq!(expires_unix_secs, 1640995200);
    }

    #[test]
    fn test_safety_interlock_ack() {
        let ack = SafetyInterlockAck::new(
            0x12345678,
            0x87654321,
            SafetyInterlockChallenge::COMBO_BOTH_CLUTCH,
            2100,
            1000000,
        );

        assert!(ack.validate_crc());

        let bytes = ack.to_bytes();
        assert_eq!(bytes[0], report_ids::SAFETY_ACK);

        let parsed = SafetyInterlockAck::from_bytes(&bytes).unwrap();
        // Copy fields to avoid packed struct alignment issues
        let challenge_token = parsed.challenge_token;
        let device_token = parsed.device_token;
        let combo_completed = parsed.combo_completed;
        let actual_hold_duration_ms = parsed.actual_hold_duration_ms;
        let completion_timestamp = parsed.completion_timestamp;

        assert_eq!(challenge_token, 0x12345678);
        assert_eq!(device_token, 0x87654321);
        assert_eq!(combo_completed, SafetyInterlockChallenge::COMBO_BOTH_CLUTCH);
        assert_eq!(actual_hold_duration_ms, 2100);
        assert_eq!(completion_timestamp, 1000000);
        assert!(parsed.validate_crc());
    }

    #[test]
    fn test_safety_ack_crc_validation() {
        let ack = SafetyInterlockAck::new(0x12345678, 0x87654321, 0, 2000, 1000000);
        assert!(ack.validate_crc());

        // Test with corrupted data
        let mut bytes = ack.to_bytes();
        bytes[5] = 0xFF; // Corrupt device token
        let corrupted = SafetyInterlockAck::from_bytes(&bytes);
        assert!(corrupted.is_err());
        assert!(corrupted.unwrap_err().contains("CRC validation failed"));
    }
}
