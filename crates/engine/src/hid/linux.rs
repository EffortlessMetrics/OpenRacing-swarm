//! Linux HID adapter with /dev/hidraw* and RT optimizations
//!
//! This module implements HID device communication on Linux using:
//! - /dev/hidraw* with libudev for enumeration
//! - Non-blocking writes for RT performance
//! - SCHED_FIFO via rtkit for RT scheduling
//! - mlockall for memory locking
//! - udev rules guidance for device permissions

use super::{
    DeviceTelemetryReport, HidDeviceInfo, MAX_TORQUE_REPORT_SIZE, MozaInputState, Seqlock,
    encode_torque_report_for_device, vendor,
};
use crate::ports::{DeviceHealthStatus, DeviceInputsMozaExt, HidDevice, HidPort};
use crate::{DeviceEvent, DeviceInfo, RTResult, TelemetryData};
use async_trait::async_trait;
use crc32fast::Hasher as Crc32Hasher;
use parking_lot::RwLock;
use racing_wheel_hid_moza_protocol::VendorProtocol;
use racing_wheel_schemas::prelude::*;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::env;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Thread-safe cached device info accessor using OnceLock
fn get_cached_device_info(device_info: &HidDeviceInfo) -> &'static DeviceInfo {
    static CACHED_INFO: OnceLock<DeviceInfo> = OnceLock::new();
    CACHED_INFO.get_or_init(|| device_info.to_device_info())
}

const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;
const HIDRAW_IOCTL_TYPE: u8 = b'H';
const HIDIOC_NR_GRDESC_SIZE: u8 = 0x01;
const HIDIOC_NR_GRDESC: u8 = 0x02;
const HIDIOC_NR_GRRAWINFO: u8 = 0x03;
const HIDIOC_NR_GRRAWNAME: u8 = 0x04;
const HIDIOC_NR_SET_FEATURE: u8 = 0x06;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: u32 = 2;
const IOC_READ_WRITE: u32 = 3;
const MOZA_TRANSPORT_MODE_ENV: &str = "OPENRACING_MOZA_TRANSPORT_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MozaTransportMode {
    RawHidraw,
    KernelPidff,
}

impl MozaTransportMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::RawHidraw => "raw-hidraw",
            Self::KernelPidff => "kernel-pidff",
        }
    }

    fn uses_raw_torque(self) -> bool {
        matches!(self, Self::RawHidraw)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct HidrawDevInfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

impl Default for HidrawReportDescriptor {
    fn default() -> Self {
        Self {
            size: 0,
            value: [0u8; HID_MAX_DESCRIPTOR_SIZE],
        }
    }
}

const fn ioctl_code(direction: u32, kind: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((direction << IOC_DIRSHIFT)
        | ((kind as u32) << IOC_TYPESHIFT)
        | ((nr as u32) << IOC_NRSHIFT)
        | ((size as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

const fn ior_read<T>(kind: u8, nr: u8) -> libc::c_ulong {
    ioctl_code(IOC_READ, kind, nr, std::mem::size_of::<T>())
}

const fn ior_len(kind: u8, nr: u8, len: usize) -> libc::c_ulong {
    ioctl_code(IOC_READ, kind, nr, len)
}

const fn iorw_len(kind: u8, nr: u8, len: usize) -> libc::c_ulong {
    ioctl_code(IOC_READ_WRITE, kind, nr, len)
}

const HIDIOCGRAWINFO: libc::c_ulong =
    ior_read::<HidrawDevInfo>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRRAWINFO);
const HIDIOCGRDESCSIZE: libc::c_ulong =
    ior_read::<libc::c_int>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRDESC_SIZE);
const HIDIOCGRDESC: libc::c_ulong =
    ior_read::<HidrawReportDescriptor>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRDESC);

fn hidiocgrawname(len: usize) -> libc::c_ulong {
    ior_len(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRRAWNAME, len)
}

fn hidiocsfeature(len: usize) -> libc::c_ulong {
    iorw_len(HIDRAW_IOCTL_TYPE, HIDIOC_NR_SET_FEATURE, len)
}

fn parse_c_string(bytes: &[u8]) -> Option<String> {
    let nul_idx = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if nul_idx == 0 {
        return None;
    }

    Some(
        String::from_utf8_lossy(&bytes[..nul_idx])
            .trim()
            .to_string(),
    )
}

fn manufacturer_for_vendor(vendor_id: u16) -> Option<String> {
    let name = match vendor_id {
        0x046D => "Logitech",
        0x0EB7 => "Fanatec",
        0x044F => "Thrustmaster",
        0x346E => "Moza Racing",
        0x0483 | 0x16D0 | 0x3670 => "Simagic",
        0x04D8 => "Heusinkveld",
        0x2433 => "Asetek SimSports",
        0x3416 => "Cammus",
        0x1209 => "OpenFFBoard / Generic HID",
        0x045B => "FFBeast",
        0x1D50 => "Granite Devices",
        0x1DD2 => "Leo Bodnar",
        0x1FC9 => "SimExperience",
        0x1021 => "Oddor",
        0xF055 => "MMOS",
        0x16C0 => "SHH",
        0x2497 => "SimJack",
        0xCAFE => "SimNet",
        0x5487 => "SimRuito",
        0xDDFD => "SimSonn",
        0x03EB => "SimTrecs",
        _ => return None,
    };
    Some(name.to_string())
}

/// Best-effort parse of the first top-level Usage Page and Usage from a HID report descriptor.
fn parse_usage_page_and_usage(descriptor: &[u8]) -> (Option<u16>, Option<u16>) {
    let mut usage_page: Option<u16> = None;
    let mut usage: Option<u16> = None;
    let mut i = 0usize;
    while i < descriptor.len() {
        match descriptor[i] {
            0x05 if i + 1 < descriptor.len() => {
                usage_page = Some(descriptor[i + 1] as u16);
                i += 2;
            }
            0x06 if i + 2 < descriptor.len() => {
                usage_page = Some(u16::from_le_bytes([descriptor[i + 1], descriptor[i + 2]]));
                i += 3;
            }
            0x09 if i + 1 < descriptor.len() => {
                usage = Some(descriptor[i + 1] as u16);
                i += 2;
            }
            0x0A if i + 2 < descriptor.len() => {
                usage = Some(u16::from_le_bytes([descriptor[i + 1], descriptor[i + 2]]));
                i += 3;
            }
            _ => {
                i += 1;
            }
        }
        if usage_page.is_some() && usage.is_some() {
            break;
        }
    }
    (usage_page, usage)
}

/// Attempt to derive the USB interface number from the sysfs path for a hidraw node.
/// Parses a component like `1-2:1.0` and returns the trailing integer (0 here).
fn try_read_linux_interface_number(hid_path: &Path) -> Option<i32> {
    let lossy = hid_path.to_string_lossy();
    if !lossy.starts_with("/dev/hidraw") {
        return None;
    }
    let node = hid_path.file_name()?.to_str()?;
    let sysfs = std::path::Path::new("/sys/class/hidraw")
        .join(node)
        .join("device");
    let target = std::fs::read_link(&sysfs).ok()?;
    for comp in target.components() {
        let s = comp.as_os_str().to_string_lossy();
        if !s.contains(':') {
            continue;
        }
        if let Some(v) = s.rfind('.').and_then(|di| s[di + 1..].parse::<i32>().ok()) {
            return Some(v);
        }
    }
    None
}

fn parse_descriptor_flags(descriptor: &[u8]) -> (bool, bool) {
    // Usage Page (PID) appears as 0x05, 0x0F in many PID descriptors.
    let supports_pid = descriptor.windows(2).any(|win| win == [0x05, 0x0F]);

    // Vendor usage pages often use 16-bit form: 0x06 <low> <high> where high=0xFF.
    let uses_vendor_usage_page = descriptor
        .windows(3)
        .any(|win| win[0] == 0x06 && win[2] == 0xFF);

    (supports_pid, uses_vendor_usage_page)
}

fn parse_moza_transport_mode(value: &str) -> Option<MozaTransportMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "raw" | "hidraw" | "raw-hidraw" => Some(MozaTransportMode::RawHidraw),
        "kernel" | "kernelpidff" | "kernel-pidff" | "pidff" => Some(MozaTransportMode::KernelPidff),
        "0" => Some(MozaTransportMode::RawHidraw),
        "1" => Some(MozaTransportMode::KernelPidff),
        _ => None,
    }
}

fn moza_transport_mode() -> MozaTransportMode {
    env::var(MOZA_TRANSPORT_MODE_ENV)
        .ok()
        .and_then(|value| parse_moza_transport_mode(&value))
        .unwrap_or(MozaTransportMode::RawHidraw)
}

fn is_moza_raw_transport_enabled(vendor_id: u16, product_id: u16) -> bool {
    if vendor_id == 0x346E {
        let is_wheelbase = vendor::moza::is_wheelbase_product(product_id);
        if !is_wheelbase {
            return true;
        }
        return moza_transport_mode().uses_raw_torque();
    }

    true
}

fn is_supported_by_descriptor(vendor_id: u16, product_id: u16, descriptor: &[u8]) -> bool {
    let has_descriptor = !descriptor.is_empty();
    let (supports_pid, uses_vendor_usage_page) = parse_descriptor_flags(descriptor);
    let has_force_hints = has_descriptor && (supports_pid || uses_vendor_usage_page);

    if !has_force_hints {
        return false;
    }

    match vendor_id {
        0x346E => {
            // Moza identity space used by wheelbases/peripherals is currently in low-byte space.
            (product_id & 0xFF00) == 0x0000
        }
        0x0483 | 0x16D0 | 0x3670 => true,
        _ => false,
    }
}

fn build_capabilities_from_identity(
    vendor_id: u16,
    product_id: u16,
    descriptor: &[u8],
) -> DeviceCapabilities {
    let (descriptor_pid, _) = parse_descriptor_flags(descriptor);

    if vendor_id == 0x346E {
        let protocol = vendor::moza::MozaProtocol::new(product_id);
        let identity = vendor::moza::identify_device(product_id);
        let config = protocol.get_ffb_config();
        let max_torque = config.max_torque_nm.clamp(0.0, TorqueNm::MAX_TORQUE);
        let max_torque = TorqueNm::new(max_torque).unwrap_or(TorqueNm::ZERO);

        return DeviceCapabilities {
            supports_pid: descriptor_pid || identity.supports_ffb,
            supports_raw_torque_1khz: identity.supports_ffb
                && is_moza_raw_transport_enabled(vendor_id, product_id),
            supports_health_stream: identity.supports_ffb,
            supports_led_bus: false,
            max_torque,
            encoder_cpr: u16::try_from(config.encoder_cpr).unwrap_or(u16::MAX),
            min_report_period_us: config.required_b_interval.unwrap_or(1) as u16 * 1000,
        };
    }

    if vendor_id == 0x0483 || vendor_id == 0x16D0 || vendor_id == 0x3670 {
        let protocol = vendor::simagic::SimagicProtocol::new(vendor_id, product_id);
        let config = protocol.get_ffb_config();
        let max_torque = config.max_torque_nm.clamp(0.0, TorqueNm::MAX_TORQUE);
        let max_torque = TorqueNm::new(max_torque).unwrap_or(TorqueNm::ZERO);

        return DeviceCapabilities {
            supports_pid: descriptor_pid,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque,
            encoder_cpr: u16::try_from(config.encoder_cpr).unwrap_or(u16::MAX),
            min_report_period_us: config.required_b_interval.unwrap_or(1) as u16 * 1000,
        };
    }

    if vendor_id == 0x0EB7 {
        let protocol = vendor::fanatec::FanatecProtocol::new(vendor_id, product_id);
        let config = protocol.get_ffb_config();
        let is_base = vendor::fanatec::is_wheelbase_product(product_id);
        let max_torque = config.max_torque_nm.clamp(0.0, TorqueNm::MAX_TORQUE);
        let max_torque = TorqueNm::new(max_torque).unwrap_or(TorqueNm::ZERO);

        return DeviceCapabilities {
            supports_pid: descriptor_pid,
            supports_raw_torque_1khz: is_base,
            supports_health_stream: is_base,
            supports_led_bus: is_base,
            max_torque,
            encoder_cpr: u16::try_from(config.encoder_cpr).unwrap_or(u16::MAX),
            min_report_period_us: config.required_b_interval.unwrap_or(1) as u16 * 1000,
        };
    }

    DeviceCapabilities {
        supports_pid: descriptor_pid,
        supports_raw_torque_1khz: false,
        supports_health_stream: false,
        supports_led_bus: false,
        max_torque: TorqueNm::new(10.0).unwrap_or(TorqueNm::ZERO),
        encoder_cpr: 4096,
        min_report_period_us: 1000,
    }
}

/// Linux-specific HID port implementation.
///
/// Manages device enumeration and hotplug monitoring for HID racing
/// peripherals on Linux using `/dev/hidraw*` device nodes and `libudev`.
///
/// # Device Lifecycle
///
/// 1. **Enumeration** — Scans `/dev/hidraw*` nodes, reads device info via
///    `ioctl`, and filters by known racing wheel vendor/product IDs.
/// 2. **Monitoring** — Watches udev events for hidraw device add/remove to
///    detect hotplug.
/// 3. **Teardown** — Dropping the port stops monitoring and releases file
///    descriptors.
///
/// # Permissions
///
/// Access to `/dev/hidraw*` requires appropriate udev rules. See
/// `packaging/linux/99-racing-wheel-suite.rules` for the recommended
/// configuration.
pub struct LinuxHidPort {
    devices: Arc<RwLock<HashMap<DeviceId, HidDeviceInfo>>>,
    monitoring: Arc<AtomicBool>,
    #[allow(dead_code)]
    event_sender: Option<mpsc::UnboundedSender<DeviceEvent>>,
}

impl LinuxHidPort {
    /// Create a new Linux HID port.
    ///
    /// Initializes the port with an empty device list and no active
    /// monitoring. Call the port's enumeration method to discover
    /// connected devices.
    ///
    /// # Errors
    ///
    /// Returns an error if required system resources cannot be allocated.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            monitoring: Arc::new(AtomicBool::new(false)),
            event_sender: None,
        })
    }

    /// Enumerate HID devices using /dev/hidraw* and libudev
    fn enumerate_devices(&self) -> Result<Vec<HidDeviceInfo>, Box<dyn std::error::Error>> {
        let mut devices = Vec::new();

        // Known racing wheel vendor/product IDs
        let racing_wheel_ids = [
            (0x046D, 0xC299), // Logitech G25
            (0x046D, 0xC294), // Logitech Driving Force / Formula EX
            (0x046D, 0xC29B), // Logitech G27
            (0x046D, 0xC24F), // Logitech G29
            (0x046D, 0xC262), // Logitech G920
            (0x046D, 0xC266), // Logitech G923
            (0x046D, 0xC267), // Logitech G923 PS
            (0x046D, 0xC26D), // Logitech G923 Xbox (HID++)
            (0x046D, 0xC26E), // Logitech G923 Xbox
            (0x046D, 0xC268), // Logitech G PRO
            (0x046D, 0xC272), // Logitech G PRO Xbox
            // Logitech legacy wheels (oversteer, linux-steering-wheels)
            (0x046D, 0xC295), // Logitech MOMO Racing
            (0x046D, 0xC298), // Logitech Driving Force Pro
            (0x046D, 0xC29A), // Logitech Driving Force GT
            (0x046D, 0xC29C), // Logitech Speed Force Wireless
            // Logitech additional legacy (kernel hid-ids.h, oversteer)
            (0x046D, 0xCA03), // Logitech MOMO Racing 2
            (0x046D, 0xC293), // Logitech WingMan Formula Force GP
            (0x046D, 0xCA04), // Logitech Vibration Wheel
            (0x046D, 0xC291), // Logitech WingMan Formula Force
            // Fanatec (VID 0x0EB7 — Endor AG)
            // Verified: gotzl/hid-fanatecff, JacKeTUs/linux-steering-wheels,
            //           berarma/oversteer, linux-hardware.org
            (0x0EB7, 0x0001), // Fanatec ClubSport Wheel Base V2
            (0x0EB7, 0x0004), // Fanatec ClubSport Wheel Base V2.5
            (0x0EB7, 0x0005), // Fanatec CSL Elite Wheel Base (PS4)
            (0x0EB7, 0x0006), // Fanatec Podium Wheel Base DD1
            (0x0EB7, 0x0007), // Fanatec Podium Wheel Base DD2
            (0x0EB7, 0x0011), // Fanatec CSR Elite
            (0x0EB7, 0x0020), // Fanatec CSL DD
            (0x0EB7, 0x0024), // Fanatec Gran Turismo DD Pro (PS-mode PID; unconfirmed in community drivers)
            (0x0EB7, 0x01E9), // Fanatec ClubSport DD+ (unconfirmed in community drivers)
            (0x0EB7, 0x0E03), // Fanatec CSL Elite Wheel Base
            (0x0EB7, 0x1839), // Fanatec ClubSport Pedals V1/V2
            (0x0EB7, 0x183B), // Fanatec ClubSport Pedals V3
            (0x0EB7, 0x6204), // Fanatec CSL Elite Pedals
            (0x0EB7, 0x6205), // Fanatec CSL Pedals with Load Cell Kit
            (0x0EB7, 0x6206), // Fanatec CSL Pedals V2
            // Thrustmaster (VID 0x044F)
            // Verified: Kimplul/hid-tmff2, Linux kernel hid-thrustmaster.c,
            //           berarma/oversteer, JacKeTUs/linux-steering-wheels,
            //           linux-hardware.org, devicehunt.com
            (0x044F, 0xB65D), // Thrustmaster FFB Wheel (pre-init)
            (0x044F, 0xB65E), // Thrustmaster T500 RS
            (0x044F, 0xB66D), // Thrustmaster T300RS (PS4 mode)
            (0x044F, 0xB67F), // Thrustmaster TMX
            (0x044F, 0xB66E), // Thrustmaster T300RS
            (0x044F, 0xB66F), // Thrustmaster T300RS GT
            (0x044F, 0xB669), // Thrustmaster TX Racing
            (0x044F, 0xB677), // Thrustmaster T150
            (0x044F, 0xB696), // Thrustmaster T248
            (0x044F, 0xB689), // Thrustmaster TS-PC Racer
            (0x044F, 0xB692), // Thrustmaster TS-XW
            (0x044F, 0xB691), // Thrustmaster TS-XW (GIP mode)
            (0x044F, 0xB69A), // Thrustmaster T248X
            (0x044F, 0xB69B), // Thrustmaster T818 (unverified — hid-tmff2 issue #58)
            // Thrustmaster legacy wheels (oversteer, linux-steering-wheels, hid-tmff)
            (0x044F, 0xB605), // Thrustmaster NASCAR Pro FF2
            (0x044F, 0xB651), // Thrustmaster FGT Rumble Force
            (0x044F, 0xB653), // Thrustmaster RGT FF Clutch
            (0x044F, 0xB654), // Thrustmaster FGT Force Feedback
            (0x044F, 0xB65A), // Thrustmaster F430 Force Feedback
            (0x044F, 0xB668), // Thrustmaster T80 (no FFB)
            (0x044F, 0xB66A), // Thrustmaster T80 Ferrari 488 GTB (no FFB)
            (0x044F, 0xB664), // Thrustmaster TX Racing Wheel
            // NOTE: 0xB678/0xB679/0xB68D removed — HOTAS peripherals, not pedals
            // Moza Racing - V1
            (0x346E, 0x0005), // Moza R3
            (0x346E, 0x0004), // Moza R5
            (0x346E, 0x0002), // Moza R9
            (0x346E, 0x0006), // Moza R12
            (0x346E, 0x0000), // Moza R16/R21
            // Moza Racing - V2
            (0x346E, 0x0015), // Moza R3 V2
            (0x346E, 0x0014), // Moza R5 V2
            (0x346E, 0x0012), // Moza R9 V2
            (0x346E, 0x0016), // Moza R12 V2
            (0x346E, 0x0010), // Moza R16/R21 V2
            // Moza Racing peripherals
            (0x346E, 0x0003), // Moza SR-P Pedals
            (0x346E, 0x0020), // Moza HGP Shifter
            (0x346E, 0x0021), // Moza SGP Sequential Shifter
            (0x346E, 0x0022), // Moza HBP Handbrake
            // Simagic legacy devices
            (0x0483, 0x0522), // Simagic Alpha / Alpha Mini / Alpha Ultimate / M10
            // Simagic EVO generation (VID 0x3670)
            (0x3670, 0x0500), // Simagic EVO Sport
            (0x3670, 0x0501), // Simagic EVO
            (0x3670, 0x0502), // Simagic EVO Pro
            // VRS DirectForce Pro devices (share VID 0x0483 with Simagic)
            (0x0483, 0xA355), // VRS DirectForce Pro (✅ kernel hid-ids.h)
            (0x0483, 0xA3BE), // VRS Pedals (✅ simracing-hwdb)
            (0x0483, 0xA44C), // VRS R295 (✅ kernel hid-ids.h)
            // NOTE: Fabricated VRS PIDs removed from dispatch:
            //   0xA356 (DFP V2), 0xA357 (Pedals V1), 0xA358 (Pedals V2),
            //   0xA359 (Handbrake), 0xA35A (Shifter)
            // These were sequential guesses with zero external evidence.
            // VRS uses non-sequential PIDs (DFP=A355, Pedals=A3BE, R295=A44C).
            // Heusinkveld pedals (VID 0x04D8 — Microchip)
            (0x04D8, 0xF6D0), // Heusinkveld Sprint
            (0x04D8, 0xF6D2), // Heusinkveld Ultimate+
            (0x04D8, 0xF6D3), // Heusinkveld Pro
            // Simucube (VID 0x16D0, dispatched by product ID)
            (0x16D0, 0x0D5A), // Simucube 1
            (0x16D0, 0x0D5F), // Simucube 2 Ultimate
            (0x16D0, 0x0D60), // Simucube 2 Pro
            (0x16D0, 0x0D61), // Simucube 2 Sport
            (0x16D0, 0x0D66), // Simucube SC-Link Hub (ActivePedal)
            (0x16D0, 0x0D63), // Simucube Wireless Wheel (estimated PID)
            // Asetek SimSports (VID 0x2433)
            (0x2433, 0xF300), // Asetek Invicta
            (0x2433, 0xF301), // Asetek Forte
            (0x2433, 0xF303), // Asetek La Prima
            (0x2433, 0xF306), // Asetek Tony Kanaan Edition
            (0x2433, 0xF100), // Asetek Invicta Pedals
            (0x2433, 0xF101), // Asetek Forte Pedals
            (0x2433, 0xF102), // Asetek La Prima Pedals
            // Cammus (VID 0x3416)
            (0x3416, 0x0301), // Cammus C5
            (0x3416, 0x0302), // Cammus C12
            (0x3416, 0x1018), // Cammus CP5 Pedals
            (0x3416, 0x1019), // Cammus LC100 Pedals
            // OpenFFBoard (VID 0x1209, pid.codes shared VID)
            (0x1209, 0xFFB0), // OpenFFBoard (✅ pid.codes, firmware)
            // NOTE: 0xFFB1 removed — not registered on pid.codes, absent from firmware
            (0x1209, 0x1BBD), // Generic HID Button Box
            // FFBeast (VID 0x045B)
            (0x045B, 0x58F9), // FFBeast Joystick
            (0x045B, 0x5968), // FFBeast Rudder
            (0x045B, 0x59D7), // FFBeast Wheel
            // Granite Devices SimpleMotion V2 (Simucube 1, IONI, ARGON, OSW)
            (0x1D50, 0x6050), // Simucube 1 / IONI Servo Drive
            (0x1D50, 0x6051), // Simucube 2 / IONI Premium Servo Drive
            (0x1D50, 0x6052), // Simucube Sport / ARGON Servo Drive
            // Leo Bodnar sim racing interfaces
            (0x1DD2, 0x000E), // Leo Bodnar USB Sim Racing Wheel Interface
            (0x1DD2, 0x000C), // Leo Bodnar BBI-32 Button Box
            (0x1DD2, 0x1301), // Leo Bodnar SLI-Pro Shift Light Indicator
            (0x1DD2, 0x0001), // Leo Bodnar USB Joystick
            (0x1DD2, 0x000B), // Leo Bodnar BU0836A Joystick
            (0x1DD2, 0x000F), // Leo Bodnar FFB Joystick
            (0x1DD2, 0x0030), // Leo Bodnar BU0836X Joystick
            (0x1DD2, 0x0031), // Leo Bodnar BU0836 16-bit Joystick
            // SimExperience AccuForce Pro (NXP USB chip VID 0x1FC9)
            (0x1FC9, 0x804C), // SimExperience AccuForce Pro
            // Cube Controls PIDs removed from dispatch: fabricated placeholders (0x0C73–0x0C75)
            // with no hardware evidence. Re-add when real USB descriptors are captured.
            // See crates/hid-cube-controls-protocol/src/ids.rs for status.
            // PXN (Lite Star) — budget racing wheels with FFB
            // Verified: kernel hid-ids.h USB_VENDOR_ID_LITE_STAR + PIDs
            (0x11FF, 0x3245), // PXN V10
            (0x11FF, 0x1212), // PXN V12
            (0x11FF, 0x1112), // PXN V12 Lite
            (0x11FF, 0x1211), // PXN V12 Lite 2
            (0x11FF, 0x2141), // PXN GT987
            // FlashFire (VID 0x2F24) — budget FFB wheels (oversteer)
            (0x2F24, 0x010D), // FlashFire 900R
            // Guillemot (VID 0x06F8) — legacy Thrustmaster parent (oversteer, hid-tmff.c)
            (0x06F8, 0x0004), // Guillemot Force Feedback Racing Wheel
            // Thrustmaster Xbox controller division (VID 0x24C6)
            (0x24C6, 0x5B00), // Thrustmaster Ferrari 458 Italia (Xbox 360)
            // Oddor (VID 0x1021)
            (0x1021, 0x1888), // Oddor Handbrake
            // MMOS (VID 0xF055)
            (0xF055, 0x0FFB), // MMOS FFB Controller
            // SHH (VID 0x16C0, V-USB shared VID)
            (0x16C0, 0x05E1), // SHH Shifter
            // SimGrade (VID 0x1209, pid.codes shared VID)
            (0x1209, 0x3115), // SimGrade VX-Pro Pedals
            // SimJack (VID 0x2497)
            (0x2497, 0x5757), // SimJack PRO Pedals
            // SimLab (VID 0x04D8, Microchip shared VID)
            (0x04D8, 0xE760), // SimLab Handbrake XB1
            // SimNet (VID 0xCAFE)
            (0xCAFE, 0xA301), // SimNet SP Pedals
            // SimRuito (VID 0x5487)
            (0x5487, 0x5401), // SimRuito Pedals
            // SimSonn (VID 0xDDFD)
            (0xDDFD, 0x5008), // SimSonn Pedals
            (0xDDFD, 0x6011), // SimSonn Pedals Plus X
            // SimTrecs (VID 0x03EB, Atmel shared VID)
            (0x03EB, 0x2406), // SimTrecs ProPedal GT
        ];

        // Scan /dev/hidraw* devices
        let hidraw_dir = Path::new("/dev");
        if let Ok(entries) = std::fs::read_dir(hidraw_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(filename_str) =
                    path.file_name().and_then(|f| f.to_str()).map(str::to_owned)
                else {
                    continue;
                };
                if !filename_str.starts_with("hidraw") {
                    continue;
                }
                if let Ok((device_info, descriptor)) = self.probe_hidraw_device(&path) {
                    // Check if this is a racing wheel
                    let is_supported_by_id = racing_wheel_ids.iter().any(|(vid, pid)| {
                        device_info.vendor_id == *vid && device_info.product_id == *pid
                    });

                    // Alpha EVO-generation devices should be discoverable even when
                    // PID mapping is incomplete; descriptor capture confirms details.
                    let is_simagic_evo = device_info.vendor_id == 0x3670;
                    let is_supported_by_descriptor = is_supported_by_descriptor(
                        device_info.vendor_id,
                        device_info.product_id,
                        &descriptor,
                    );

                    if is_supported_by_id || is_simagic_evo || is_supported_by_descriptor {
                        devices.push(device_info);
                    }
                }
            }
        }

        // If no real devices found, add mock devices for tests.
        if cfg!(test) && devices.is_empty() {
            for (vid, pid) in racing_wheel_ids.iter().take(3) {
                let device_id = DeviceId::new(format!("hidraw_{:04X}_{:04X}", vid, pid))?;
                let path = format!("/dev/hidraw_mock_{:04X}_{:04X}", vid, pid);

                let capabilities = DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: false,
                    max_torque: TorqueNm::new(25.0).unwrap_or(TorqueNm::ZERO),
                    encoder_cpr: 4096,
                    min_report_period_us: 1000, // 1kHz
                };

                let device_info = HidDeviceInfo {
                    device_id: device_id.clone(),
                    vendor_id: *vid,
                    product_id: *pid,
                    serial_number: Some(format!("SN{:04X}{:04X}", vid, pid)),
                    manufacturer: Some(match *vid {
                        0x046D => "Logitech".to_string(),
                        0x0EB7 => "Fanatec".to_string(),
                        0x044F => "Thrustmaster".to_string(),
                        0x346E => "Moza Racing".to_string(),
                        0x0483 | 0x16D0 | 0x3670 => "Simagic".to_string(),
                        0x04D8 => "Heusinkveld".to_string(),
                        0x2433 => "Asetek SimSports".to_string(),
                        0x3416 => "Cammus".to_string(),
                        0x1209 => "OpenFFBoard / Generic HID".to_string(),
                        0x045B => "FFBeast".to_string(),
                        0x1D50 => "Granite Devices".to_string(),
                        0x1DD2 => "Leo Bodnar".to_string(),
                        0x1FC9 => "SimExperience".to_string(),
                        0x1021 => "Oddor".to_string(),
                        0xF055 => "MMOS".to_string(),
                        0x16C0 => "SHH".to_string(),
                        0x2497 => "SimJack".to_string(),
                        0xCAFE => "SimNet".to_string(),
                        0x5487 => "SimRuito".to_string(),
                        0xDDFD => "SimSonn".to_string(),
                        0x03EB => "SimTrecs".to_string(),
                        _ => "Unknown".to_string(),
                    }),
                    product_name: Some(format!("Racing Wheel {:04X}:{:04X}", vid, pid)),
                    path,
                    interface_number: None,
                    usage_page: None,
                    usage: None,
                    report_descriptor_len: None,
                    report_descriptor_crc32: None,
                    capabilities,
                };

                devices.push(device_info);
            }
        }

        debug!("Enumerated {} racing wheel devices on Linux", devices.len());
        Ok(devices)
    }

    /// Probe a hidraw device to get its information
    fn probe_hidraw_device(
        &self,
        path: &Path,
    ) -> Result<(HidDeviceInfo, Vec<u8>), Box<dyn std::error::Error>> {
        let file = OpenOptions::new().read(true).open(path)?;
        let fd = file.as_raw_fd();

        let mut raw_info = HidrawDevInfo::default();
        let raw_info_result = unsafe { libc::ioctl(fd, HIDIOCGRAWINFO, &mut raw_info) };
        if raw_info_result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let vendor_id = u16::from_ne_bytes(raw_info.vendor.to_ne_bytes());
        let product_id = u16::from_ne_bytes(raw_info.product.to_ne_bytes());

        let mut name_buf = [0u8; 256];
        let name_result =
            unsafe { libc::ioctl(fd, hidiocgrawname(name_buf.len()), name_buf.as_mut_ptr()) };
        let product_name = if name_result > 0 {
            parse_c_string(&name_buf)
        } else {
            None
        };

        let mut desc_size: libc::c_int = 0;
        let desc_size_result = unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE, &mut desc_size) };
        let descriptor = if desc_size_result >= 0 {
            let mut descriptor = HidrawReportDescriptor::default();
            let safe_size = desc_size.clamp(0, HID_MAX_DESCRIPTOR_SIZE as libc::c_int);
            descriptor.size = safe_size as u32;
            let desc_result = unsafe { libc::ioctl(fd, HIDIOCGRDESC, &mut descriptor) };
            if desc_result >= 0 {
                let used = usize::try_from(descriptor.size).unwrap_or(0);
                let used = used.min(HID_MAX_DESCRIPTOR_SIZE);
                descriptor.value[..used].to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let capabilities = build_capabilities_from_identity(vendor_id, product_id, &descriptor);
        let device_id = DeviceId::new(format!(
            "linux_{:04X}_{:04X}_{}",
            vendor_id,
            product_id,
            path.display()
        ))?;

        let (usage_page, usage) = parse_usage_page_and_usage(&descriptor);
        let interface_number = try_read_linux_interface_number(path);
        let report_descriptor_len = u32::try_from(descriptor.len()).ok();
        let report_descriptor_crc32 = if descriptor.is_empty() {
            None
        } else {
            let mut hasher = Crc32Hasher::new();
            hasher.update(&descriptor);
            Some(hasher.finalize())
        };

        Ok((
            HidDeviceInfo {
                device_id,
                vendor_id,
                product_id,
                serial_number: None,
                manufacturer: manufacturer_for_vendor(vendor_id),
                product_name: product_name
                    .or_else(|| Some(format!("Racing Wheel {:04X}:{:04X}", vendor_id, product_id))),
                path: path.to_string_lossy().to_string(),
                interface_number,
                usage_page,
                usage,
                report_descriptor_len,
                report_descriptor_crc32,
                capabilities,
            },
            descriptor,
        ))
    }
}

#[async_trait]
impl HidPort for LinuxHidPort {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
        let device_infos = self.enumerate_devices()?;
        let mut devices = self.devices.write();
        devices.clear();

        let mut result = Vec::new();
        for device_info in device_infos {
            devices.insert(device_info.device_id.clone(), device_info.clone());
            result.push(device_info.to_device_info());
        }

        Ok(result)
    }

    async fn open_device(
        &self,
        id: &DeviceId,
    ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.read();
        let device_info = devices
            .get(id)
            .ok_or_else(|| format!("Device not found: {}", id))?;

        let device = LinuxHidDevice::new(device_info.clone())?;
        Ok(Box::new(device))
    }

    async fn monitor_devices(
        &self,
    ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        // Use bounded channel to prevent unbounded memory growth.
        // Buffer 100 events - if receiver falls behind, events are dropped with warning.
        let (sender, receiver) = mpsc::channel(100);

        // Start device monitoring using inotify on /dev
        let devices = self.devices.clone();
        let monitoring = self.monitoring.clone();
        let sender_clone = sender.clone();

        monitoring.store(true, Ordering::Relaxed);

        tokio::spawn(async move {
            let mut last_devices: HashMap<DeviceId, HidDeviceInfo> = HashMap::new();

            while monitoring.load(Ordering::Relaxed) {
                // Check for device changes every 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;

                // In a real implementation, this would use inotify to watch /dev
                // for hidraw device creation/removal
                let current_devices = devices.read().clone();

                // Track whether any events were dropped or channel closed
                let mut dropped = false;
                let mut closed = false;

                // Check for new devices
                for (id, info) in &current_devices {
                    if !last_devices.contains_key(id) {
                        let event = DeviceEvent::Connected(info.to_device_info());
                        // Use try_send to avoid blocking monitor loop if receiver is slow
                        match sender_clone.try_send(event) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                dropped = true;
                                warn!(
                                    "Device monitor channel full, dropping connect event for {}",
                                    id
                                );
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                closed = true;
                                break;
                            }
                        }
                    }
                }

                // Check for removed devices (skip if channel closed)
                if !closed {
                    for (id, info) in &last_devices {
                        if !current_devices.contains_key(id) {
                            let event = DeviceEvent::Disconnected(info.to_device_info());
                            // Use try_send to avoid blocking monitor loop if receiver is slow
                            match sender_clone.try_send(event) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    dropped = true;
                                    warn!(
                                        "Device monitor channel full, dropping disconnect event for {}",
                                        id
                                    );
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    closed = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                // Exit if channel closed
                if closed {
                    break;
                }

                // Only update last_devices if all events were delivered.
                // If any were dropped, we'll retry them on the next iteration.
                if !dropped {
                    last_devices = current_devices;
                }
            }
        });

        Ok(receiver)
    }

    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.list_devices().await?;
        Ok(())
    }
}

/// hidraw feature-report writer used for vendor initialization handshakes
struct HidrawVendorWriter {
    fd: RawFd,
}

impl vendor::DeviceWriter for HidrawVendorWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        const MAX_FEATURE_REPORT_BYTES: usize = 64;
        if data.len() > MAX_FEATURE_REPORT_BYTES {
            return Err(
                format!("feature report too large for hidraw: {} bytes", data.len()).into(),
            );
        }

        let mut report = [0u8; MAX_FEATURE_REPORT_BYTES];
        report[..data.len()].copy_from_slice(data);

        let rc = unsafe { libc::ioctl(self.fd, hidiocsfeature(data.len()), report.as_mut_ptr()) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        Ok(rc as usize)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        let rc = unsafe { libc::write(self.fd, data.as_ptr() as *const libc::c_void, data.len()) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        Ok(rc as usize)
    }
}

/// Linux-specific HID device implementation with non-blocking I/O.
///
/// Communicates with a single HID racing peripheral via `/dev/hidraw*`
/// using non-blocking file I/O (`O_NONBLOCK`). Designed for the RT force
/// feedback path where writes must complete without blocking.
///
/// # Device Lifecycle
///
/// 1. **Open** — [`LinuxHidDevice::new`] opens the hidraw node for reading
///    and writing, initializes vendor-specific protocols (e.g., Moza raw
///    transport), and sets up atomic health tracking fields.
/// 2. **Operation** — Torque commands are written via non-blocking `write(2)`.
///    Telemetry is read from the device. Health status (temperature, faults,
///    communication errors) is tracked atomically.
/// 3. **Close** — Dropping the device closes the file descriptors.
///
/// # Error Recovery
///
/// If the device node cannot be opened (e.g., permissions, device
/// disconnected), the device is created in a degraded state with `None`
/// file handles and a warning is logged.
///
/// # RT-Safety
///
/// All health-tracking fields use atomics for lock-free access from the
/// RT thread. Write operations use `O_NONBLOCK` to avoid stalling the
/// 1kHz loop.
pub struct LinuxHidDevice {
    device_info: HidDeviceInfo,
    connected: AtomicBool,
    last_seq: AtomicU16,
    created_at: Instant,
    last_communication_us: AtomicU64,
    communication_errors: AtomicU32,
    temperature_c: AtomicU8,
    fault_flags: AtomicU8,
    hands_on: AtomicBool,
    moza_raw_transport_enabled: bool,
    moza_protocol: Option<vendor::moza::MozaProtocol>,
    has_moza_input: AtomicBool,
    moza_input_seq: AtomicU32,
    moza_input_state: Seqlock<MozaInputState>,
    write_file: Option<File>,
    read_file: Option<File>,
}

impl LinuxHidDevice {
    /// Create a new Linux HID device and open its hidraw node.
    ///
    /// Opens the device path for both reading and writing. The write handle
    /// uses `O_NONBLOCK` for RT-safe torque output. If the path contains
    /// `"mock"`, file handles are skipped for testing.
    ///
    /// For Moza devices, the transport mode is determined by the
    /// `OPENRACING_MOZA_TRANSPORT_MODE` environment variable (defaults to
    /// raw hidraw).
    ///
    /// # Errors
    ///
    /// Returns an error if the device path cannot be opened and the device
    /// is not a mock. Individual open failures (read or write) are logged
    /// as warnings and the corresponding handle is set to `None`.
    pub fn new(device_info: HidDeviceInfo) -> Result<Self, Box<dyn std::error::Error>> {
        let created_at = Instant::now();

        // In a real implementation, open the hidraw device
        let write_file = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for writing with non-blocking flag
            match OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&device_info.path)
            {
                Ok(file) => Some(file),
                Err(e) => {
                    warn!("Failed to open {} for writing: {}", device_info.path, e);
                    None
                }
            }
        };

        let read_file = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for reading
            match OpenOptions::new().read(true).open(&device_info.path) {
                Ok(file) => Some(file),
                Err(e) => {
                    warn!("Failed to open {} for reading: {}", device_info.path, e);
                    None
                }
            }
        };

        let moza_raw_transport_enabled =
            is_moza_raw_transport_enabled(device_info.vendor_id, device_info.product_id);
        if device_info.vendor_id == 0x346E {
            let transport_mode = moza_transport_mode();
            if transport_mode.uses_raw_torque() {
                info!(
                    "Opening Moza device {} in transport mode: {}",
                    device_info.device_id,
                    transport_mode.as_str()
                );
            } else {
                info!(
                    "Opening Moza device {} in transport mode: {} (raw torque disabled)",
                    device_info.device_id,
                    transport_mode.as_str()
                );
            }
        }

        Self::initialize_vendor_protocol(&device_info, &write_file, moza_raw_transport_enabled);

        let moza_protocol = (device_info.vendor_id == 0x346E)
            .then_some(vendor::moza::MozaProtocol::new(device_info.product_id));

        Ok(Self {
            device_info,
            connected: AtomicBool::new(true),
            last_seq: AtomicU16::new(0),
            created_at,
            last_communication_us: AtomicU64::new(0),
            communication_errors: AtomicU32::new(0),
            temperature_c: AtomicU8::new(25),
            fault_flags: AtomicU8::new(0),
            hands_on: AtomicBool::new(false),
            moza_raw_transport_enabled,
            moza_protocol,
            has_moza_input: AtomicBool::new(false),
            moza_input_seq: AtomicU32::new(0),
            moza_input_state: Seqlock::new(MozaInputState::empty(0)),
            write_file,
            read_file,
        })
    }

    fn initialize_vendor_protocol(
        device_info: &HidDeviceInfo,
        write_file: &Option<File>,
        moza_raw_transport_enabled: bool,
    ) {
        if device_info.vendor_id == 0x346E && !moza_raw_transport_enabled {
            debug!(
                "Skipping Moza vendor initialization for {} because transport mode is {}",
                device_info.device_id,
                MozaTransportMode::KernelPidff.as_str()
            );
            return;
        }

        let Some(write_file) = write_file.as_ref() else {
            debug!(
                "Skipping vendor initialization for {} (VID={:04X}, PID={:04X}) - no writable handle",
                device_info.device_id, device_info.vendor_id, device_info.product_id
            );
            return;
        };

        // For Moza wheelbases in raw-transport mode, apply signature-based policy before
        // constructing the protocol handler (so high torque + direct mode are gated).
        if device_info.vendor_id == 0x346E
            && moza_raw_transport_enabled
            && vendor::moza::is_wheelbase_product(device_info.product_id)
        {
            let crc32 = device_info.report_descriptor_crc32;
            let requested_mode = vendor::moza::default_ffb_mode();
            let effective_mode = vendor::moza::effective_ffb_mode(requested_mode, crc32);
            let high_torque_opt_in = vendor::moza::effective_high_torque_opt_in(crc32);

            if vendor::moza::default_high_torque_enabled() && !high_torque_opt_in {
                warn!(
                    "Moza high torque requested but signature not trusted (crc32={:?}) for {}. \
                     Add CRC32 to {} or set {}=1 to override.",
                    crc32,
                    device_info.device_id,
                    "OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST",
                    "OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE"
                );
            }
            if requested_mode != effective_mode {
                warn!(
                    "Moza FFB mode {:?} requested but signature not trusted (crc32={:?}) for {}; \
                     using {:?}.",
                    requested_mode, crc32, device_info.device_id, effective_mode
                );
            }

            let protocol = vendor::moza::MozaProtocol::new_with_config(
                device_info.product_id,
                effective_mode,
                high_torque_opt_in,
            );
            let mut writer = HidrawVendorWriter {
                fd: write_file.as_raw_fd(),
            };
            if let Err(e) = protocol.initialize_device(&mut writer) {
                warn!(
                    "Moza vendor initialization failed for {} (VID={:04X}, PID={:04X}): {}",
                    device_info.device_id, device_info.vendor_id, device_info.product_id, e
                );
            }
            return;
        }

        let Some(protocol) =
            vendor::get_vendor_protocol(device_info.vendor_id, device_info.product_id)
        else {
            return;
        };

        let mut writer = HidrawVendorWriter {
            fd: write_file.as_raw_fd(),
        };
        if let Err(e) = protocol.initialize_device(&mut writer) {
            warn!(
                "Vendor initialization failed for {} (VID={:04X}, PID={:04X}): {}",
                device_info.device_id, device_info.vendor_id, device_info.product_id, e
            );
        }
    }

    #[inline]
    fn mark_communication(&self) {
        let elapsed = self.created_at.elapsed().as_micros();
        let elapsed_u64 = if elapsed > u64::MAX as u128 {
            u64::MAX
        } else {
            elapsed as u64
        };
        self.last_communication_us
            .store(elapsed_u64, Ordering::Relaxed);
    }

    /// Perform non-blocking write operation (RT-safe)
    fn write_nonblocking(&mut self, data: &[u8]) -> RTResult {
        let fd = match self.write_file.as_ref() {
            Some(file) => file.as_raw_fd(),
            None => {
                // Mock device - simulate successful write
                debug!("Writing {} bytes to mock HID device", data.len());
                self.mark_communication();
                return Ok(());
            }
        };

        // Perform non-blocking write
        let result = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                // Write would block - this is expected in RT context
                debug!("HID write would block (EAGAIN)");
                return Ok(());
            } else if errno == libc::ENODEV || errno == libc::EPIPE {
                // Device disconnected
                self.connected.store(false, Ordering::Relaxed);
                self.communication_errors.fetch_add(1, Ordering::Relaxed);
                return Err(crate::RTError::DeviceDisconnected);
            } else {
                // Other error
                warn!("HID write error: errno {}", errno);
                self.communication_errors.fetch_add(1, Ordering::Relaxed);
                return Err(crate::RTError::PipelineFault);
            }
        }

        if result as usize != data.len() {
            warn!("Partial HID write: {} of {} bytes", result, data.len());
        }

        self.mark_communication();

        debug!("Wrote {} bytes to HID device", result);
        Ok(())
    }

    fn publish_moza_input_state(&self, mut state: MozaInputState) {
        state.tick = self.moza_input_seq.fetch_add(1, Ordering::Relaxed);

        self.moza_input_state.write(state);
        self.mark_communication();
        self.has_moza_input.store(true, Ordering::Relaxed);
    }

    /// Read telemetry data (non-RT, can block)
    fn read_telemetry_blocking(&mut self) -> Option<TelemetryData> {
        let fd = match self.read_file.as_ref() {
            Some(file) => file.as_raw_fd(),
            None => {
                // Mock device - simulate telemetry data
                let report = DeviceTelemetryReport {
                    report_id: DeviceTelemetryReport::REPORT_ID,
                    wheel_angle_mdeg: 0,
                    wheel_speed_mrad_s: 0,
                    temp_c: 30,
                    faults: 0,
                    hands_on: 1,
                    reserved: [0; 2],
                };
                self.temperature_c.store(report.temp_c, Ordering::Relaxed);
                self.fault_flags.store(report.faults, Ordering::Relaxed);
                self.hands_on.store(report.hands_on != 0, Ordering::Relaxed);
                self.mark_communication();
                return Some(report.to_telemetry_data());
            }
        };

        // Read telemetry report
        let mut buffer = [0u8; 64]; // Typical HID report size
        let result =
            unsafe { libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len()) };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::ENODEV {
                self.connected.store(false, Ordering::Relaxed);
            }
            self.communication_errors.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        if result == 0 {
            return None;
        }

        let result_bytes = &buffer[..result as usize];

        if let Some(report) = DeviceTelemetryReport::from_bytes(result_bytes) {
            self.temperature_c.store(report.temp_c, Ordering::Relaxed);
            self.fault_flags.store(report.faults, Ordering::Relaxed);
            self.hands_on.store(report.hands_on != 0, Ordering::Relaxed);
            self.mark_communication();
            Some(report.to_telemetry_data())
        } else if let Some(moza_protocol) = self.moza_protocol.as_ref() {
            if let Some(state) = moza_protocol.parse_input_state(result_bytes) {
                self.publish_moza_input_state(state);
            }
            None
        } else if self.device_info.vendor_id == vendor::fanatec::FANATEC_VENDOR_ID {
            if let Some(state) = vendor::fanatec::parse_extended_report(result_bytes) {
                self.temperature_c
                    .store(state.motor_temp_c, Ordering::Relaxed);
                self.fault_flags.store(state.fault_flags, Ordering::Relaxed);
                self.mark_communication();
            }
            None
        } else {
            None
        }
    }
}

impl Drop for LinuxHidDevice {
    fn drop(&mut self) {
        let Some(write_file) = &self.write_file else {
            return;
        };
        let Some(protocol) =
            vendor::get_vendor_protocol(self.device_info.vendor_id, self.device_info.product_id)
        else {
            return;
        };
        let mut writer = HidrawVendorWriter {
            fd: write_file.as_raw_fd(),
        };
        if let Err(e) = protocol.shutdown_device(&mut writer) {
            debug!(
                "Vendor shutdown failed for {} (VID={:04X}, PID={:04X}): {}",
                self.device_info.device_id,
                self.device_info.vendor_id,
                self.device_info.product_id,
                e
            );
        }
    }
}

impl HidDevice for LinuxHidDevice {
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(crate::RTError::DeviceDisconnected);
        }
        if self.moza_protocol.is_some() && !self.moza_raw_transport_enabled {
            debug!(
                "Skipping Moza raw torque write for {} because transport mode is {}",
                self.device_info.device_id,
                MozaTransportMode::KernelPidff.as_str()
            );
            return Ok(());
        }

        // Sequence tracking stays lock-free for RT determinism.
        self.last_seq.store(seq, Ordering::Relaxed);

        let mut report = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(
            self.device_info.vendor_id,
            self.device_info.product_id,
            self.device_info.capabilities.max_torque.value(),
            torque_nm,
            seq,
            &mut report,
        );

        // Perform non-blocking write (RT-safe)
        self.write_nonblocking(&report[..len])
    }

    fn read_telemetry(&mut self) -> Option<TelemetryData> {
        if !self.connected.load(Ordering::Relaxed) {
            return None;
        }

        self.read_telemetry_blocking()
    }

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.device_info.capabilities
    }

    fn device_info(&self) -> &DeviceInfo {
        get_cached_device_info(&self.device_info)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn health_status(&self) -> DeviceHealthStatus {
        let elapsed_us = self.last_communication_us.load(Ordering::Relaxed);
        let last_communication = match self
            .created_at
            .checked_add(Duration::from_micros(elapsed_us))
        {
            Some(ts) => ts,
            None => self.created_at,
        };

        DeviceHealthStatus {
            temperature_c: self.temperature_c.load(Ordering::Relaxed),
            fault_flags: self.fault_flags.load(Ordering::Relaxed),
            hands_on: self.hands_on.load(Ordering::Relaxed),
            last_communication,
            communication_errors: self.communication_errors.load(Ordering::Relaxed),
        }
    }

    fn moza_input_state(&self) -> Option<MozaInputState> {
        if !self.has_moza_input.load(Ordering::Relaxed) {
            return None;
        }

        Some(self.moza_input_state.read())
    }

    fn read_inputs(&self) -> Option<crate::DeviceInputs> {
        self.moza_input_state()
            .map(|s| crate::DeviceInputs::from_moza_input_state(&s))
    }
}

/// Apply Linux-specific RT optimizations for the HID thread.
///
/// Configures the current thread and process for low-latency HID
/// communication:
///
/// - **`mlockall`** — Locks all current and future memory pages to prevent
///   page faults in the RT path.
/// - **`SCHED_FIFO`** — Attempts to set real-time scheduling priority 50
///   via direct `sched_setscheduler` (requires `CAP_SYS_NICE`) or rtkit.
///
/// Non-critical failures are logged as warnings but do not cause the
/// function to return an error.
///
/// # Errors
///
/// Currently always returns `Ok(())`. Individual optimizations that fail
/// are logged as warnings.
pub fn apply_linux_rt_setup() -> Result<(), Box<dyn std::error::Error>> {
    info!("Applying Linux RT optimizations");

    // Lock memory pages to prevent swapping
    unsafe {
        let result = libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE);
        if result == 0 {
            info!("Locked memory pages with mlockall");
        } else {
            warn!(
                "Failed to lock memory pages: errno {}",
                *libc::__errno_location()
            );
        }
    }

    // Try to set SCHED_FIFO priority via rtkit
    // In a real implementation, this would use D-Bus to communicate with rtkit
    info!("Attempting to acquire RT scheduling via rtkit");

    // For now, try direct sched_setscheduler (requires CAP_SYS_NICE or rtkit)
    unsafe {
        let param = libc::sched_param {
            sched_priority: 50, // Mid-range RT priority
        };

        let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
        if result == 0 {
            info!("Set SCHED_FIFO priority 50");
        } else {
            let errno = *libc::__errno_location();
            if errno == libc::EPERM {
                info!(
                    "No permission for SCHED_FIFO, consider using rtkit or adding user to realtime group"
                );
            } else {
                warn!("Failed to set SCHED_FIFO: errno {}", errno);
            }
        }
    }

    // Set CPU affinity to isolate RT thread
    // In a real implementation, this would pin to isolated CPUs
    info!("Consider isolating CPUs with isolcpus= kernel parameter for better RT performance");

    // Guidance for udev rules
    info!("For device permissions, create /etc/udev/rules.d/99-racing-wheel.rules:");
    info!(
        "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"046d\", ATTRS{{idProduct}}==\"c294\", MODE=\"0666\", GROUP=\"input\""
    );
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"0eb7\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"044f\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"346e\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"0483\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"16d0\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"3670\", MODE=\"0666\", GROUP=\"input\"");
    info!("Then run: sudo udevadm control --reload-rules && sudo udevadm trigger");

    // Guidance for rtkit setup
    info!("For RT scheduling without root, install rtkit and add user to realtime group:");
    info!("sudo usermod -a -G realtime $USER");
    info!("Then logout and login again");

    Ok(())
}

/// Revert Linux RT optimizations applied by [`apply_linux_rt_setup`].
///
/// Unlocks memory pages and resets the scheduling policy to `SCHED_OTHER`
/// (normal). Non-critical failures are logged as warnings.
///
/// # Errors
///
/// Currently always returns `Ok(())`.
pub fn revert_linux_rt_setup() -> Result<(), Box<dyn std::error::Error>> {
    info!("Reverting Linux RT optimizations");

    // Unlock memory pages
    unsafe {
        let result = libc::munlockall();
        if result == 0 {
            info!("Unlocked memory pages");
        }
    }

    // Reset to normal scheduling
    unsafe {
        let param = libc::sched_param { sched_priority: 0 };

        let result = libc::sched_setscheduler(0, libc::SCHED_OTHER, &param);
        if result == 0 {
            info!("Reset to SCHED_OTHER");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    /// Mutex to serialize tests that manipulate the MOZA_TRANSPORT_MODE_ENV var.
    /// Without this, parallel tests race on the shared env var and produce flakes.
    static TRANSPORT_MODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_moza_transport_mode<T, F>(mode: Option<&str>, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let _guard = TRANSPORT_MODE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = env::var(MOZA_TRANSPORT_MODE_ENV).ok();
        #[allow(clippy::panic)]
        match mode {
            Some(value) => unsafe { env::set_var(MOZA_TRANSPORT_MODE_ENV, value) },
            None => unsafe { env::remove_var(MOZA_TRANSPORT_MODE_ENV) },
        }

        let result = test();

        match previous {
            Some(value) => unsafe { env::set_var(MOZA_TRANSPORT_MODE_ENV, value) },
            None => unsafe { env::remove_var(MOZA_TRANSPORT_MODE_ENV) },
        }

        result
    }

    #[tokio::test]
    async fn test_linux_hid_port_creation() -> TestResult {
        let port = LinuxHidPort::new()?;
        assert!(!port.monitoring.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn test_device_enumeration() -> TestResult {
        let port = LinuxHidPort::new()?;
        let devices = port.list_devices().await?;

        // Should find some mock devices
        assert!(!devices.is_empty());

        for device in &devices {
            assert!(!device.name.is_empty());
            assert!(device.vendor_id != 0);
            assert!(device.product_id != 0);
        }
        Ok(())
    }

    #[test]
    fn test_hidiocgrawname_direction_is_read() {
        let code = hidiocgrawname(256);
        let direction = (code >> IOC_DIRSHIFT) & 0x3;
        assert_eq!(direction, IOC_READ as libc::c_ulong);
    }

    #[tokio::test]
    async fn test_device_opening() -> TestResult {
        let port = LinuxHidPort::new()?;
        let devices = port.list_devices().await?;

        if let Some(device_info) = devices.first() {
            let device = port.open_device(&device_info.id).await?;
            assert!(device.is_connected());
            assert!(device.capabilities().max_torque.value() > 0.0);
        }
        Ok(())
    }

    #[test]
    fn test_simagic_capabilities_from_identity() -> TestResult {
        let caps = build_capabilities_from_identity(0x0483, 0x0522, &[]);
        assert!(caps.supports_raw_torque_1khz);
        assert!(caps.supports_health_stream);
        assert!((caps.max_torque.value() - 15.0).abs() < 0.01);
        assert_eq!(caps.encoder_cpr, u16::MAX);
        Ok(())
    }

    #[test]
    fn test_linux_hid_device_creation() -> TestResult {
        let device_id = must("test-device".parse::<DeviceId>());
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: must(TorqueNm::new(25.0)),
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        };

        let device_info = HidDeviceInfo {
            device_id,
            vendor_id: 0x046D,
            product_id: 0xC294,
            serial_number: Some("TEST123".to_string()),
            manufacturer: Some("Test Manufacturer".to_string()),
            product_name: Some("Test Racing Wheel".to_string()),
            path: "/dev/hidraw_mock".to_string(),
            interface_number: None,
            usage_page: None,
            usage: None,
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            capabilities,
        };

        let device = LinuxHidDevice::new(device_info)?;
        assert!(device.is_connected());
        assert_eq!(device.capabilities().max_torque.value(), 25.0);
        Ok(())
    }

    #[test]
    fn test_ffb_report_writing() -> TestResult {
        let device_id = must("test-device".parse::<DeviceId>());
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: must(TorqueNm::new(25.0)),
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        };

        let device_info = HidDeviceInfo {
            device_id,
            vendor_id: 0x046D,
            product_id: 0xC294,
            serial_number: Some("TEST123".to_string()),
            manufacturer: Some("Test Manufacturer".to_string()),
            product_name: Some("Test Racing Wheel".to_string()),
            path: "/dev/hidraw_mock".to_string(),
            interface_number: None,
            usage_page: None,
            usage: None,
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            capabilities,
        };

        let mut device = LinuxHidDevice::new(device_info)?;
        device.write_ffb_report(5.0, 123)?;
        Ok(())
    }

    #[test]
    fn test_rt_setup_functions() {
        // These functions should not panic
        let _ = apply_linux_rt_setup();
        let _ = revert_linux_rt_setup();
    }

    #[test]
    fn test_hidraw_device_probing() -> TestResult {
        let port = LinuxHidPort::new()?;
        let path = Path::new("/dev/hidraw0");

        // This should not panic even if the device doesn't exist
        let _ = port.probe_hidraw_device(path);
        Ok(())
    }

    #[test]
    fn test_is_supported_by_descriptor_moza() {
        let descriptor_with_pid = [0x05u8, 0x0F, 0x09, 0x30, 0x26, 0xFF];
        let descriptor_with_vendor_usage = [0x06u8, 0xC0, 0xFF, 0x09, 0x30];

        assert!(is_supported_by_descriptor(
            0x346E,
            0x0004,
            &descriptor_with_pid
        ));
        assert!(is_supported_by_descriptor(
            0x346E,
            0x0022,
            &descriptor_with_vendor_usage
        ));
        assert!(!is_supported_by_descriptor(
            0x346E,
            0x1234,
            &descriptor_with_pid
        ));
        assert!(!is_supported_by_descriptor(
            0x046D,
            0xC294,
            &descriptor_with_pid
        ));
    }

    #[test]
    fn test_is_supported_by_descriptor_requires_feature_hints() {
        let empty_descriptor = [0u8; 4];
        let no_hint_descriptor = [0x01, 0x02, 0x03, 0x04];

        assert!(!is_supported_by_descriptor(
            0x346E,
            0x0004,
            &empty_descriptor
        ));
        assert!(!is_supported_by_descriptor(
            0x346E,
            0x0004,
            &no_hint_descriptor
        ));
        assert!(!is_supported_by_descriptor(
            0x0000,
            0x0004,
            &no_hint_descriptor
        ));
    }

    #[test]
    fn test_moza_transport_mode_defaults_to_raw() {
        with_moza_transport_mode(None, || {
            let caps = build_capabilities_from_identity(
                0x346E,
                vendor::moza::product_ids::R5_V1,
                &[0x05, 0x0F, 0x09, 0x30],
            );
            assert!(caps.supports_raw_torque_1khz);
        });
    }

    #[test]
    fn test_moza_transport_mode_kernel_disables_raw_capability() {
        with_moza_transport_mode(Some("kernel"), || {
            let caps = build_capabilities_from_identity(
                0x346E,
                vendor::moza::product_ids::R5_V1,
                &[0x05, 0x0F, 0x09, 0x30],
            );
            assert!(!caps.supports_raw_torque_1khz);
        });
    }

    #[test]
    fn test_moza_transport_mode_invalid_value_defaults_to_raw() {
        with_moza_transport_mode(Some("mystery"), || {
            let caps = build_capabilities_from_identity(
                0x346E,
                vendor::moza::product_ids::R5_V1,
                &[0x05, 0x0F, 0x09, 0x30],
            );
            assert!(caps.supports_raw_torque_1khz);
        });
    }

    #[test]
    fn test_fanatec_capabilities_gt_dd_pro() {
        for pid in [0x0020u16, 0x0024u16] {
            let caps = build_capabilities_from_identity(0x0EB7, pid, &[]);
            assert!(
                caps.supports_raw_torque_1khz,
                "GT DD Pro PID {pid:#06x} should support raw torque"
            );
            assert!(
                caps.supports_health_stream,
                "GT DD Pro PID {pid:#06x} should support health stream"
            );
            assert!(
                caps.supports_led_bus,
                "GT DD Pro PID {pid:#06x} should have LED bus"
            );
            assert!(
                (caps.max_torque.value() - 8.0).abs() < 0.1,
                "GT DD Pro PID {pid:#06x} expected 8 Nm, got {}",
                caps.max_torque.value()
            );
            assert_eq!(caps.min_report_period_us, 1000);
        }
    }

    #[test]
    fn test_fanatec_capabilities_non_wheelbase_uses_defaults() {
        // Unknown Fanatec PID (e.g. pedals/accessory) should still report safe defaults
        let caps = build_capabilities_from_identity(0x0EB7, 0x9999, &[]);
        assert!(!caps.supports_raw_torque_1khz);
        assert!(!caps.supports_health_stream);
    }

    #[test]
    fn test_fanatec_pedal_capabilities_no_ffb() {
        // Standalone pedal devices: no raw torque, no health stream, no LED bus
        for pid in [0x1839u16, 0x183B, 0x6205, 0x6206] {
            let caps = build_capabilities_from_identity(0x0EB7, pid, &[]);
            assert!(
                !caps.supports_raw_torque_1khz,
                "PID {pid:#06x}: pedal must not support raw torque"
            );
            assert!(
                !caps.supports_health_stream,
                "PID {pid:#06x}: pedal must not expose health stream"
            );
            assert!(
                !caps.supports_led_bus,
                "PID {pid:#06x}: pedal must not have LED bus"
            );
        }
    }

    #[test]
    fn test_fanatec_wheelbase_capabilities_have_led_bus() {
        // Wheelbases always have LED bus support
        for pid in [0x0006u16, 0x0007, 0x0020, 0x0024] {
            let caps = build_capabilities_from_identity(0x0EB7, pid, &[]);
            assert!(
                caps.supports_led_bus,
                "PID {pid:#06x}: wheelbase must have LED bus"
            );
            assert!(
                caps.supports_raw_torque_1khz,
                "PID {pid:#06x}: wheelbase must support raw torque"
            );
        }
    }

    #[test]
    fn test_moza_transport_mode_kernel_disables_ffb_write() -> TestResult {
        with_moza_transport_mode(Some("kernel-pidff"), || -> TestResult {
            let device_id = must("test-moza-kernel-pidff".parse::<DeviceId>());
            let capabilities = DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: false,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque: must(TorqueNm::new(5.5)),
                encoder_cpr: 65_535,
                min_report_period_us: 1000,
            };

            let device_info = HidDeviceInfo {
                device_id,
                vendor_id: 0x346E,
                product_id: vendor::moza::product_ids::R5_V1,
                serial_number: Some("SN346E0004".to_string()),
                manufacturer: Some("Moza Racing".to_string()),
                product_name: Some("Moza R5".to_string()),
                path: "/dev/hidraw_mock".to_string(),
                interface_number: None,
                usage_page: None,
                usage: None,
                report_descriptor_len: None,
                report_descriptor_crc32: None,
                capabilities,
            };

            let mut device = LinuxHidDevice::new(device_info)?;
            assert!(device.write_ffb_report(5.0, 123).is_ok());
            Ok(())
        })
    }
}
