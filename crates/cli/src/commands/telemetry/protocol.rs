pub(super) const REGISTER_COMMAND_APPLICATION: u8 = 1;
pub(super) const PROTOCOL_VERSION: u8 = 4;
pub(super) const MSG_REGISTRATION_RESULT: u8 = 1;
pub(super) const MAX_PACKET_SIZE: usize = 4096;
pub(super) const CAPTURE_MAGIC: &[u8; 8] = b"ORACAPv1";
pub(super) const RECORD_COMMAND: &str = "wheelctl telemetry record";

#[cfg(test)]
pub(super) const DEFAULT_SIMHUB_PORT: u16 = 5555;
