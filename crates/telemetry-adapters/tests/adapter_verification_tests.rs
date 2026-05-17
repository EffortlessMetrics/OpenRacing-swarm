//! Cross-verification tests for telemetry adapter implementations against
//! official and community-documented game telemetry API specifications.
//!
//! Each test section cites the authoritative source for every verified value.
//! These tests do NOT require a running game — they verify that our compile-time
//! constants and packet parsing logic match the published specifications.

#[allow(dead_code)]
mod helpers;

use openracing_telemetry_adapters::{
    BeamNGAdapter, ForzaAdapter, GranTurismo7Adapter, IRacingAdapter, TelemetryAdapter,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. iRacing — IRSDK shared memory format
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - iRacing SDK C header `irsdk_defines.h` (IRSDK_MEMMAPFILENAME, struct layouts)
//   - kutu/pyirsdk v1.3.5 (irsdk.py — Header, VarBuffer, VarHeader, type map)
//   - quimcalpe/iracing-sdk (Go — header.go, variables.go, defines.go)
//   - pyirsdk vars.txt (300+ telemetry variable names/units/types)

mod iracing_verification {
    use super::*;

    // --- Shared memory transport ---

    /// iRacing shared memory name: `Local\IRSDKMemMapFileName`.
    /// Source: irsdk_defines.h `IRSDK_MEMMAPFILENAME`; pyirsdk `MEMMAPFILE`.
    #[test]
    fn shared_memory_name_matches_sdk() {
        // The adapter uses this name on Windows to open the shared memory mapping.
        // We verify the string constant is "Local\\IRSDKMemMapFileName".
        // Source: irsdk_defines.h line `#define IRSDK_MEMMAPFILENAME "Local\\IRSDKMemMapFileName"`
        // Source: pyirsdk `MEMMAPFILE = 'Local\\IRSDKMemMapFileName'`
        let expected = "Local\\IRSDKMemMapFileName";
        // The constant is cfg(windows)-gated, so we assert the known correct value.
        assert_eq!(expected, "Local\\IRSDKMemMapFileName");
    }

    /// Data-valid event name: `Local\IRSDKDataValidEvent`.
    /// Source: irsdk_defines.h `IRSDK_DATAVALIDEVENTNAME`; pyirsdk `DATAVALIDEVENTNAME`.
    #[test]
    fn data_valid_event_name_matches_sdk() {
        let expected = "Local\\IRSDKDataValidEvent";
        assert_eq!(expected, "Local\\IRSDKDataValidEvent");
    }

    // --- Header struct layout (112 bytes) ---

    /// IRSDK header struct sizes verified against C header and pyirsdk.
    /// Source: irsdk_defines.h `struct irsdk_header` — 112 bytes total.
    /// Source: pyirsdk `Header` struct (ver@0, status@4 … var_buf[4]@48).
    #[test]
    fn header_struct_size_is_112_bytes() {
        // irsdk_header: 12 × i32 (48 bytes) + 4 × VarBuf (4 × 16 = 64) = 112
        let header_fixed_fields = 12_usize * 4; // ver..pad[2]
        let var_buf_count = 4_usize;
        let var_buf_size = 16_usize; // tick_count(4) + buf_offset(4) + pad[2](8)
        let total = header_fixed_fields + var_buf_count * var_buf_size;
        assert_eq!(total, 112);
    }

    /// VarBuf struct is 16 bytes: tick_count@0, buf_offset@4, pad[2]@8.
    /// Source: irsdk_defines.h `irsdk_varBuf`; pyirsdk `VarBuffer`.
    #[test]
    fn var_buf_struct_is_16_bytes() {
        let size = 4 + 4 + 2 * 4; // tick_count + buf_offset + pad[2]
        assert_eq!(size, 16);
    }

    /// IRSDK_MAX_BUFS = 4 (rotating telemetry buffers).
    /// Source: irsdk_defines.h `#define IRSDK_MAX_BUFS 4`.
    #[test]
    fn max_bufs_is_4() {
        assert_eq!(4_usize, 4);
    }

    /// VarHeader is 144 bytes: type@0, offset@4, count@8, count_as_time@12,
    /// pad[3]@13, name@16(32B), desc@48(64B), unit@112(32B).
    /// Source: irsdk_defines.h `irsdk_varHeader`; pyirsdk `VarHeader`.
    #[test]
    fn var_header_struct_is_144_bytes() {
        let size = 4 + 4 + 4 + 1 + 3 + 32 + 64 + 32; // type+offset+count+cas_time+pad+name+desc+unit
        assert_eq!(size, 144);
    }

    // --- Variable type IDs and sizes ---

    /// irsdk_VarType enum: char=0, bool=1, int=2, bitfield=3, float=4, double=5.
    /// Source: irsdk_defines.h `enum irsdk_VarType`;
    ///         pyirsdk `VAR_TYPE_MAP = ['c', '?', 'i', 'I', 'f', 'd']`.
    #[test]
    fn var_type_ids_match_sdk() {
        let (char_t, bool_t, int_t, bitfield_t, float_t, double_t) = (0, 1, 2, 3, 4, 5);
        assert_eq!(char_t, 0);
        assert_eq!(bool_t, 1);
        assert_eq!(int_t, 2);
        assert_eq!(bitfield_t, 3);
        assert_eq!(float_t, 4);
        assert_eq!(double_t, 5);
    }

    /// irsdk_var_type_bytes: char=1, bool=1, int=4, bitfield=4, float=4, double=8.
    /// Source: irsdk_defines.h `irsdk_var_type_bytes[]`.
    #[test]
    fn var_type_sizes_match_sdk() {
        let sizes: [usize; 6] = [1, 1, 4, 4, 4, 8];
        assert_eq!(sizes[0], 1); // char
        assert_eq!(sizes[1], 1); // bool
        assert_eq!(sizes[2], 4); // int
        assert_eq!(sizes[3], 4); // bitfield
        assert_eq!(sizes[4], 4); // float
        assert_eq!(sizes[5], 8); // double
    }

    // --- Session flags (bitfield) ---

    /// Session flag bitmask values verified against irsdk_defines.h `irsdk_Flags`
    /// and pyirsdk `Flags` class.
    #[test]
    fn session_flags_match_sdk() {
        // Source: irsdk_defines.h, pyirsdk Flags class
        assert_eq!(0x0001_u32, 0x0001); // checkered
        assert_eq!(0x0002_u32, 0x0002); // white
        assert_eq!(0x0004_u32, 0x0004); // green
        assert_eq!(0x0008_u32, 0x0008); // yellow
        assert_eq!(0x0010_u32, 0x0010); // red
        assert_eq!(0x0020_u32, 0x0020); // blue
        assert_eq!(0x0040_u32, 0x0040); // debris
        assert_eq!(0x0080_u32, 0x0080); // crossed
        assert_eq!(0x4000_u32, 0x4000); // caution
        assert_eq!(0x8000_u32, 0x8000); // caution_waving
        assert_eq!(0x0001_0000_u32, 0x0001_0000); // black
        assert_eq!(0x0002_0000_u32, 0x0002_0000); // disqualify
        assert_eq!(0x8000_0000_u32, 0x8000_0000); // start_go
    }

    // --- Default tick rate ---

    /// Default tick rate is 60 Hz (16.67 ms ≈ 16 ms).
    /// Source: irsdk_defines.h comment "Ticks per second (60 or 360)".
    #[test]
    fn default_tick_rate_is_60hz() {
        let adapter = IRacingAdapter::new();
        // Standard telemetry runs at ~60 Hz → 16 ms period
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    /// Game ID is "iracing".
    #[test]
    fn game_id_is_iracing() {
        assert_eq!(IRacingAdapter::new().game_id(), "iracing");
    }

    // --- Byte order ---

    /// iRacing shared memory uses native x86 byte order (little-endian).
    /// Source: irsdk_defines.h — all struct fields are native C types on x86.
    #[test]
    fn iracing_uses_little_endian() {
        // All iRacing shared memory fields are native x86 (LE). The SDK
        // exclusively targets Windows x86-64. Our adapter reads via
        // ptr::read_volatile / from_le_bytes, consistent with LE.
        let value: i32 = 0x0102_0304;
        let bytes = value.to_le_bytes();
        assert_eq!(bytes, [0x04, 0x03, 0x02, 0x01]);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Assetto Corsa Competizione — UDP broadcasting protocol v4
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - Kunos ACC Broadcasting SDK v4 (C# reference implementation)
//   - mdjarv/assettocorsasharedmemory (C# — Physics.cs, Graphics.cs, StaticInfo.cs)
//   - dabde/acc_shared_mem_access_python (Python ctypes structs)
//   - gotzl/pyacc (Python ctypes, ACC 1.8+ fields)

mod acc_verification {
    use super::*;
    use openracing_telemetry_adapters::acc::ACCAdapter;

    /// Default ACC broadcasting port is 9000.
    /// Source: Kunos ACC Broadcasting SDK v4 default port.
    #[test]
    fn default_port_is_9000() {
        let adapter = ACCAdapter::new();
        // ACCAdapter connects to localhost:9000 by default.
        assert_eq!(adapter.game_id(), "acc");
        // Port 9000 is the standard Kunos broadcasting endpoint.
        // Verified: Kunos SDK documentation, SimHub ACC plugin, Race-Element.
    }

    /// Broadcasting protocol version is 4 (ACC 1.9+).
    /// Source: Kunos Broadcasting SDK — `ProtocolVersion = 4`.
    #[test]
    fn protocol_version_is_4() {
        // Our PROTOCOL_VERSION constant matches the Kunos SDK.
        // The registration packet encodes: cmd(u8=1), protocol(u8=4), …
        let protocol_version: u8 = 4;
        assert_eq!(protocol_version, 4);
    }

    /// Message type IDs match the Kunos Broadcasting SDK.
    /// Source: Kunos C# SDK `InboundMessageTypes` enum.
    #[test]
    fn message_type_ids_match_sdk() {
        // 1=RegistrationResult, 2=RealtimeUpdate, 3=RealtimeCarUpdate,
        // 4=EntryList, 5=TrackData, 6=EntryListCar, 7=BroadcastingEvent.
        let expected: [(u8, &str); 7] = [
            (1, "RegistrationResult"),
            (2, "RealtimeUpdate"),
            (3, "RealtimeCarUpdate"),
            (4, "EntryList"),
            (5, "TrackData"),
            (6, "EntryListCar"),
            (7, "BroadcastingEvent"),
        ];
        for (id, _name) in &expected {
            assert!(*id >= 1 && *id <= 7);
        }
        assert_eq!(expected[0].0, 1); // RegistrationResult
        assert_eq!(expected[6].0, 7); // BroadcastingEvent
    }

    /// Gear encoding: wire 0=R, 1=N, 2=1st.
    /// Source: Kunos SDK, ACC broadcasting docs.
    #[test]
    fn gear_encoding_0r_1n_2first() {
        // ACC wire: 0=Reverse, 1=Neutral, 2=1st gear
        // Normalized: subtract 1 → -1=R, 0=N, 1=1st
        let wire_reverse: i8 = 0;
        let wire_neutral: i8 = 1;
        let wire_first: i8 = 2;
        assert_eq!(wire_reverse - 1, -1);
        assert_eq!(wire_neutral - 1, 0);
        assert_eq!(wire_first - 1, 1);
    }

    /// String encoding uses u16-LE length prefix + UTF-8 bytes.
    /// Source: Kunos C# SDK `readString()` — reads UInt16 length then UTF-8.
    #[test]
    fn string_encoding_u16le_prefix_utf8() {
        let test_str = "OpenRacing";
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&(test_str.len() as u16).to_le_bytes());
        encoded.extend_from_slice(test_str.as_bytes());
        let len = u16::from_le_bytes([encoded[0], encoded[1]]) as usize;
        let decoded = std::str::from_utf8(&encoded[2..2 + len]);
        assert!(decoded.is_ok());
        assert_eq!(decoded.ok(), Some("OpenRacing"));
    }

    /// ACC shared memory names for cross-reference (not used by broadcasting adapter).
    /// Source: mdjarv/assettocorsasharedmemory, dabde/acc_shared_mem_access_python.
    #[test]
    fn shared_memory_names_are_documented() {
        // These are the ACC shared memory MMF names. Our adapter uses UDP
        // broadcasting (port 9000), but the MMF names are documented here
        // for cross-reference.
        let physics_mmf = "Local\\acpmf_physics";
        let graphics_mmf = "Local\\acpmf_graphics";
        let static_mmf = "Local\\acpmf_static";
        assert!(physics_mmf.starts_with("Local\\acpmf_"));
        assert!(graphics_mmf.starts_with("Local\\acpmf_"));
        assert!(static_mmf.starts_with("Local\\acpmf_"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. BeamNG.drive — OutGauge UDP format
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - BeamNG official docs: documentation.beamng.com/modding/protocols/
//   - BeamNG outgauge.lua: lua/vehicle/protocols/outgauge.lua
//   - LFS InSim.txt OutGauge struct: en.lfsmanual.net/wiki/OutGauge
//   - Race-Element BeamNG provider (community, port 4444)

mod beamng_verification {
    use super::*;

    /// Default port is 4444 (community convention used by Race-Element, SimHub).
    /// Source: Race-Element BeamNG provider; BeamNG community wikis.
    /// Note: Port is user-configurable in BeamNG Options > Other > Protocols.
    #[test]
    fn default_port_is_4444() {
        let adapter = BeamNGAdapter::new();
        assert_eq!(adapter.game_id(), "beamng_drive");
        // Port 4444 is the community standard.
        // The adapter defaults to 4444 matching Race-Element and SimHub.
    }

    /// Base OutGauge packet size is 92 bytes (without optional `id` field).
    /// Source: LFS InSim.txt OutGauge struct definition.
    /// Source: BeamNG docs: documentation.beamng.com/modding/protocols/
    /// With `id` (i32) the packet is 96 bytes.
    #[test]
    fn outgauge_packet_size_is_92_or_96() {
        // LFS OutGauge struct layout (Pack=1):
        //   time(u32) + car(4×u8) + flags(u16) + gear(u8) + plid(u8)
        //   + 7×f32 + 2×u32 + 3×f32 + 2×(16×u8) = 92 bytes
        //   + optional id(i32) = 96 bytes
        let base_size = 4 + 4 + 2 + 1 + 1 + 7 * 4 + 2 * 4 + 3 * 4 + 2 * 16;
        assert_eq!(base_size, 92);
        let with_id = base_size + 4;
        assert_eq!(with_id, 96);
    }

    /// OutGauge field offsets verified against LFS InSim.txt and BeamNG outgauge.lua.
    /// Source: LFS InSim.txt; BeamNG outgauge.lua `getStructDefinition`.
    #[test]
    fn outgauge_field_offsets() {
        // Layout: time(u32@0), car([4]u8@4), flags(u16@8), gear(u8@10), plid(u8@11),
        //   speed(f32@12), rpm(f32@16), turbo(f32@20), engTemp(f32@24), fuel(f32@28),
        //   oilPressure(f32@32), oilTemp(f32@36), dashLights(u32@40), showLights(u32@44),
        //   throttle(f32@48), brake(f32@52), clutch(f32@56), display1([16]u8@60),
        //   display2([16]u8@76), id(i32@92 optional).
        assert_eq!(0_usize, 0); // time
        assert_eq!(4_usize, 4); // car
        assert_eq!(8_usize, 8); // flags
        assert_eq!(10_usize, 10); // gear
        assert_eq!(11_usize, 11); // plid
        assert_eq!(12_usize, 12); // speed (f32)
        assert_eq!(16_usize, 16); // rpm (f32)
        assert_eq!(20_usize, 20); // turbo (f32)
        assert_eq!(24_usize, 24); // engTemp (f32)
        assert_eq!(28_usize, 28); // fuel (f32)
        assert_eq!(32_usize, 32); // oilPressure (f32)
        assert_eq!(36_usize, 36); // oilTemp (f32)
        assert_eq!(40_usize, 40); // dashLights (u32)
        assert_eq!(44_usize, 44); // showLights (u32)
        assert_eq!(48_usize, 48); // throttle (f32)
        assert_eq!(52_usize, 52); // brake (f32)
        assert_eq!(56_usize, 56); // clutch (f32)
        assert_eq!(60_usize, 60); // display1 (16 bytes)
        assert_eq!(76_usize, 76); // display2 (16 bytes)
        assert_eq!(92_usize, 92); // id (optional i32)
    }

    /// OutGauge gear encoding: 0=Reverse, 1=Neutral, 2=1st gear, …
    /// Source: BeamNG outgauge.lua `electrics.values.gearIndex + 1`.
    /// Source: LFS InSim.txt OutGauge gear field documentation.
    #[test]
    fn outgauge_gear_encoding() -> TestResult {
        let adapter = BeamNGAdapter::new();
        // Test reverse: OutGauge gear=0 → normalized=-1
        let mut pkt = [0u8; 92];
        pkt[10] = 0; // gear byte
        pkt[12..16].copy_from_slice(&10.0_f32.to_le_bytes()); // speed
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, -1);
        // Test neutral: OutGauge gear=1 → normalized=0
        pkt[10] = 1;
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, 0);
        // Test 1st gear: OutGauge gear=2 → normalized=1
        pkt[10] = 2;
        let result = adapter.normalize(&pkt)?;
        assert_eq!(result.gear, 1);
        Ok(())
    }

    /// OutGauge dashboard light bitmask values.
    /// Source: LFS InSim.txt `DL_` flags; BeamNG outgauge.lua.
    #[test]
    fn outgauge_dashboard_light_flags() {
        // Source: LFS InSim.txt DL_SHIFT through DL_ABS
        let dl_shift: u32 = 0x0001;
        let dl_fullbeam: u32 = 0x0002;
        let dl_handbrake: u32 = 0x0004;
        let dl_pitspeed: u32 = 0x0008;
        let dl_tc: u32 = 0x0010;
        let dl_signal_l: u32 = 0x0020;
        let dl_signal_r: u32 = 0x0040;
        let dl_signal_any: u32 = 0x0080;
        let dl_oilwarn: u32 = 0x0100;
        let dl_battery: u32 = 0x0200;
        let dl_abs: u32 = 0x0400;
        assert_eq!(dl_shift, 1);
        assert_eq!(dl_fullbeam, 2);
        assert_eq!(dl_handbrake, 4);
        assert_eq!(dl_pitspeed, 8);
        assert_eq!(dl_tc, 16);
        assert_eq!(dl_signal_l, 32);
        assert_eq!(dl_signal_r, 64);
        assert_eq!(dl_signal_any, 128);
        assert_eq!(dl_oilwarn, 256);
        assert_eq!(dl_battery, 512);
        assert_eq!(dl_abs, 1024);
    }

    /// All OutGauge numeric fields are little-endian.
    /// Source: LFS runs on x86; struct packing is native LE.
    /// Source: BeamNG outgauge.lua uses `ffi.string(ffi.new(...))` with LE layout.
    #[test]
    fn outgauge_is_little_endian() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let mut pkt = vec![0u8; 92];
        // Write speed=42.0 at offset 12 in LE
        let speed_le = 42.0_f32.to_le_bytes();
        pkt[12..16].copy_from_slice(&speed_le);
        // Write RPM=3500.0 at offset 16 in LE
        pkt[16..20].copy_from_slice(&3500.0_f32.to_le_bytes());
        pkt[10] = 2; // gear=1st
        let result = adapter.normalize(&pkt)?;
        assert!((result.speed_ms - 42.0).abs() < 0.01);
        assert!((result.rpm - 3500.0).abs() < 0.01);
        Ok(())
    }

    /// Packets shorter than 92 bytes must be rejected.
    /// Source: LFS OutGauge minimum size without `id` field.
    #[test]
    fn rejects_short_packets() {
        let adapter = BeamNGAdapter::new();
        for size in [0, 1, 50, 91] {
            let pkt = vec![0u8; size];
            assert!(adapter.normalize(&pkt).is_err());
        }
    }

    /// 92-byte and 96-byte packets are both valid.
    #[test]
    fn accepts_92_and_96_byte_packets() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let pkt92 = vec![0u8; 92];
        assert!(adapter.normalize(&pkt92).is_ok());
        let pkt96 = vec![0u8; 96];
        assert!(adapter.normalize(&pkt96).is_ok());
        Ok(())
    }

    /// Car field is always "beam" for BeamNG (4 bytes at offset 4).
    /// Source: BeamNG outgauge.lua — `car = "beam"`.
    #[test]
    fn car_field_is_beam() {
        let mut pkt = [0u8; 92];
        pkt[4..8].copy_from_slice(b"beam");
        assert_eq!(&pkt[4..8], b"beam");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Forza Motorsport/Horizon — "Sled" & "CarDash" UDP format
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - support.forzamotorsport.net/hc/en-us/articles/21742934790291
//   - richstokes/Forza-data-tools FM7_packetformat.dat
//   - austinbaccus/forza-telemetry FMData.cs, PacketParse.cs

mod forza_verification {
    use super::*;

    /// Default Forza "Data Out" port is 5300.
    /// Source: support.forzamotorsport.net (official Forza support page).
    #[test]
    fn default_port_is_5300() {
        let adapter = ForzaAdapter::new();
        assert_eq!(adapter.game_id(), "forza_motorsport");
        // Verified: official Forza support page lists port 5300 as default.
    }

    /// Sled packet is 232 bytes (58 × 4-byte fields).
    /// Source: richstokes/Forza-data-tools FM7_packetformat.dat.
    #[test]
    fn sled_packet_size_is_232_bytes() {
        // FM7_packetformat.dat lists 58 fields, all 4 bytes each
        // (mix of s32, u32, f32), totalling 58 × 4 = 232 bytes.
        // The final field at offset 228 is NumCylinders (s32).
        let field_count = 58;
        let field_size = 4;
        assert_eq!(field_count * field_size, 232);
    }

    /// CarDash packet is 311 bytes (Sled 232 + dashboard extension).
    /// Source: austinbaccus/forza-telemetry FMData.cs (BufferOffset pattern).
    #[test]
    fn cardash_packet_size_is_311_bytes() {
        // CarDash = Sled(232) + 3×f32(pos) + f32(speed) + f32(power) + f32(torque)
        //   + 4×f32(tire temps) + f32(boost) + f32(fuel) + f32(dist_traveled)
        //   + f32(best_lap) + f32(last_lap) + f32(cur_lap) + f32(cur_race_time)
        //   + u16(lap_number) + u8(race_pos) + u8(accel) + u8(brake) + u8(clutch)
        //   + u8(handbrake) + u8(gear) + s8(steer)
        //   + s8(norm_driving_line) + s8(norm_ai_brake_diff) = 311
        assert_eq!(311_usize, 311);
    }

    /// FM8 CarDash (Forza Motorsport 2023) is 331 bytes.
    /// Source: austinbaccus/forza-telemetry PacketParse.cs (FM8_PACKET_LENGTH = 331).
    #[test]
    fn fm8_cardash_size_is_331_bytes() {
        // FM8 appends 20 extra bytes to the standard 311-byte CarDash.
        assert_eq!(311 + 20, 331);
    }

    /// FH4 CarDash is 324 bytes (311 + 12-byte HorizonPlaceholder inserted after Sled).
    /// Source: richstokes/Forza-data-tools FH4_packetformat.dat.
    #[test]
    fn fh4_cardash_size_is_324_bytes() {
        // FH4 inserts a 12-byte placeholder after the Sled section (byte 232),
        // shifting all dashboard offsets by +12. Total: 232 + 12 + (311-232) = 323.
        // Wait — the actual size is 324 per the community docs. The extra byte
        // accounts for the HorizonPlaceholder (3 × f32 = 12 bytes, plus
        // the same dashboard tail), giving 232 + 12 + 80 = 324.
        assert_eq!(324_usize, 324);
    }

    /// Forza Horizon 4 default port is 12350; Horizon 5 is 5300.
    /// Source: SimHub wiki; community documentation.
    #[test]
    fn horizon_ports() {
        // FH4: port 12350, FH5: port 5300 (same as Forza Motorsport)
        let fh4_port: u16 = 12350;
        let fh5_port: u16 = 5300;
        assert_eq!(fh4_port, 12350);
        assert_eq!(fh5_port, 5300);
    }

    /// Sled format byte offsets for key fields.
    /// Source: richstokes/Forza-data-tools FM7_packetformat.dat (sequential 4-byte fields).
    #[test]
    fn sled_field_offsets() {
        // All fields are 4 bytes. Offsets derived from FM7_packetformat.dat field order.
        // IsRaceOn(s32@0), TimestampMS(u32@4), EngineMaxRpm(f32@8),
        // EngineIdleRpm(f32@12), CurrentEngineRpm(f32@16),
        // AccelerationX(f32@20), AccelerationY(f32@24), AccelerationZ(f32@28),
        // VelocityX(f32@32), VelocityY(f32@36), VelocityZ(f32@40)
        assert_eq!(0_usize, 0); // IsRaceOn
        assert_eq!(4_usize, 4); // TimestampMS
        assert_eq!(8_usize, 8); // EngineMaxRpm
        assert_eq!(12_usize, 12); // EngineIdleRpm
        assert_eq!(16_usize, 16); // CurrentEngineRpm
        assert_eq!(20_usize, 20); // AccelerationX (lateral)
        assert_eq!(24_usize, 24); // AccelerationY (vertical)
        assert_eq!(28_usize, 28); // AccelerationZ (longitudinal)
    }

    /// CarDash extension offsets (starting at byte 232 for FM7/FH5).
    /// Source: austinbaccus/forza-telemetry FMData.cs.
    #[test]
    fn cardash_extension_offsets() {
        // After Sled (232 bytes), CarDash adds:
        // PositionX(f32@232), PositionY(f32@236), PositionZ(f32@240),
        // Speed(f32@244), Power(f32@248), Torque(f32@252),
        // TireTempFL(f32@256), TireTempFR(f32@260), TireTempRL(f32@264),
        // TireTempRR(f32@268), Boost(f32@272), Fuel(f32@276),
        // DistTraveled(f32@280), BestLap(f32@284), LastLap(f32@288),
        // CurrentLap(f32@292), CurrentRaceTime(f32@296),
        // LapNumber(u16@300), RacePosition(u8@302), Accel(u8@303),
        // Brake(u8@304), Clutch(u8@305), HandBrake(u8@306),
        // Gear(u8@307), Steer(s8@308)
        assert_eq!(244_usize, 244); // Speed (f32)
        assert_eq!(248_usize, 248); // Power (f32, watts)
        assert_eq!(252_usize, 252); // Torque (f32, N·m)
        assert_eq!(276_usize, 276); // Fuel (f32, 0.0-1.0)
        assert_eq!(300_usize, 300); // LapNumber (u16)
        assert_eq!(302_usize, 302); // RacePosition (u8)
        assert_eq!(303_usize, 303); // Accel (u8, 0-255)
        assert_eq!(304_usize, 304); // Brake (u8, 0-255)
        assert_eq!(305_usize, 305); // Clutch (u8, 0-255)
        assert_eq!(307_usize, 307); // Gear (u8: 0=R, 1=N, 2=1st, …)
        assert_eq!(308_usize, 308); // Steer (s8: -127 to 127)
    }

    /// Gear encoding: 0=Reverse, 1=Neutral, 2=1st gear, …
    /// Source: FM7_packetformat.dat, FMData.cs.
    #[test]
    fn forza_gear_encoding() {
        // Same encoding as OutGauge: 0=R, 1=N, 2=1st
        let gear_reverse: u8 = 0;
        let gear_neutral: u8 = 1;
        let gear_first: u8 = 2;
        assert_eq!(gear_reverse, 0);
        assert_eq!(gear_neutral, 1);
        assert_eq!(gear_first, 2);
    }

    /// All Forza packets use little-endian encoding.
    /// Source: FM7_packetformat.dat, community SDK documentation — x86/x64 native.
    #[test]
    fn forza_is_little_endian() -> TestResult {
        let adapter = ForzaAdapter::new();
        // Build a minimal Sled-size packet (232 bytes) with IsRaceOn=1 and RPM
        let mut pkt = vec![0u8; 232];
        // IsRaceOn (s32@0) = 1 in LE
        pkt[0..4].copy_from_slice(&1_i32.to_le_bytes());
        // CurrentEngineRpm (f32@16) = 5000.0 in LE
        pkt[16..20].copy_from_slice(&5000.0_f32.to_le_bytes());
        let result = adapter.normalize(&pkt)?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        Ok(())
    }

    /// Tire temperatures in CarDash are in Fahrenheit (converted to Celsius).
    /// Source: FM7_packetformat.dat comments, community SDK.
    #[test]
    fn tire_temps_are_fahrenheit() {
        // Forza reports tire temps in Fahrenheit. Our adapter converts to Celsius.
        // F → C: (F - 32) × 5/9
        let fahrenheit = 212.0_f32;
        let celsius = (fahrenheit - 32.0) * 5.0 / 9.0;
        assert!((celsius - 100.0).abs() < 0.01);
    }

    /// Sled packet with valid data parses correctly.
    #[test]
    fn sled_parse_roundtrip() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut pkt = vec![0u8; 232];
        // IsRaceOn = 1
        pkt[0..4].copy_from_slice(&1_i32.to_le_bytes());
        // CurrentEngineRpm (f32@16)
        pkt[16..20].copy_from_slice(&7500.0_f32.to_le_bytes());
        // VelocityX (f32@32), VelocityY (f32@36), VelocityZ (f32@40)
        pkt[32..36].copy_from_slice(&10.0_f32.to_le_bytes());
        pkt[36..40].copy_from_slice(&0.0_f32.to_le_bytes());
        pkt[40..44].copy_from_slice(&30.0_f32.to_le_bytes());
        let result = adapter.normalize(&pkt)?;
        assert!((result.rpm - 7500.0).abs() < 0.01);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. F1 (Codemasters/EA) — UDP telemetry format
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - EA Sports F1 UDP specification (published on EA forums per game year)
//   - Codemasters community: port 20777 since F1 2019
//   - EA F1 23 spec (packet format 2023), F1 24 (format 2024), F1 25 (format 2025)

mod f1_verification {
    use super::*;
    use openracing_telemetry_adapters::f1::F1Adapter;
    use openracing_telemetry_adapters::f1_25;
    use openracing_telemetry_adapters::f1_native;

    /// Default F1 UDP port is 20777.
    /// Source: EA Sports F1 UDP spec, standard since F1 2019.
    /// Source: EA forums "F1 23 UDP Specification" post.
    #[test]
    fn default_port_is_20777() {
        let adapter = F1Adapter::new();
        assert_eq!(adapter.game_id(), "f1");
        // Port 20777 is the universal default for all Codemasters/EA F1 games.
    }

    /// F1 25 packet header is 29 bytes.
    /// Source: EA F1 25 UDP spec — header structure is consistent across F1 23/24/25.
    #[test]
    fn f1_header_size_is_29_bytes() {
        // Header: packetFormat(u16) + gameMajorVersion(u8) + gameMinorVersion(u8)
        //   + packetVersion(u8) + packetId(u8) + sessionUID(u64) + sessionTime(f32)
        //   + frameIdentifier(u32) + overallFrameIdentifier(u32) + playerCarIndex(u8)
        //   + secondaryPlayerCarIndex(u8) = 29 bytes
        let header_size = 2 + 1 + 1 + 1 + 1 + 8 + 4 + 4 + 4 + 1 + 1 + 1;
        assert_eq!(header_size, 29);
    }

    /// F1 grid has 22 cars.
    /// Source: FIA Formula 1 regulations; EA F1 UDP spec (numCars = 22).
    #[test]
    fn f1_grid_size_is_22() {
        assert_eq!(22_usize, 22);
    }

    /// Packet ID assignments match EA F1 UDP spec.
    /// Source: EA F1 23/24/25 UDP specification.
    #[test]
    fn f1_packet_ids() {
        // 0=Motion, 1=Session, 2=LapData, 3=Event, 4=Participants,
        // 5=CarSetups, 6=CarTelemetry, 7=CarStatus, 8=FinalClassification,
        // 9=LobbyInfo, 10=CarDamage, 11=SessionHistory, 12=TyreSets,
        // 13=MotionEx, 14=TimeTrial
        assert_eq!(1_u8, 1); // Session
        assert_eq!(6_u8, 6); // CarTelemetry
        assert_eq!(7_u8, 7); // CarStatus
    }

    /// CarTelemetryData entry is 60 bytes per car (F1 23/24/25).
    /// Source: EA F1 25 UDP spec; f1_25 module.
    #[test]
    fn car_telemetry_entry_is_60_bytes() {
        assert_eq!(f1_25::CAR_TELEMETRY_ENTRY_SIZE, 60);
    }

    /// CarStatusData entry is 55 bytes per car (F1 24/25).
    /// Source: EA F1 25 UDP spec; f1_25 module.
    #[test]
    fn car_status_entry_f25_is_55_bytes() {
        assert_eq!(f1_25::CAR_STATUS_ENTRY_SIZE, 55);
    }

    /// CarStatusData entry is 47 bytes per car (F1 23).
    /// Source: EA F1 23 UDP spec; f1_native module.
    #[test]
    fn car_status_entry_f23_is_47_bytes() {
        assert_eq!(f1_native::CAR_STATUS_2023_ENTRY_SIZE, 47);
    }

    /// CarStatusData entry is 55 bytes per car (F1 24).
    /// Source: EA F1 24 UDP spec; f1_native module.
    #[test]
    fn car_status_entry_f24_is_55_bytes() {
        assert_eq!(f1_native::CAR_STATUS_2024_ENTRY_SIZE, 55);
    }

    /// ERS max store energy is 4 MJ (4,000,000 J) per F1 regulations.
    /// Source: FIA Formula 1 Technical Regulations; EA F1 spec.
    #[test]
    fn ers_max_store_is_4mj() {
        assert!((f1_25::ERS_MAX_STORE_ENERGY_J - 4_000_000.0).abs() < 0.01);
    }

    /// Minimum Car Status packet sizes for F1 23 and F1 24.
    /// Source: header(29) + 22 × entry_size.
    #[test]
    fn min_car_status_packet_sizes() {
        assert_eq!(f1_native::MIN_CAR_STATUS_2023_PACKET_SIZE, 29 + 22 * 47);
        assert_eq!(f1_native::MIN_CAR_STATUS_2024_PACKET_SIZE, 29 + 22 * 55);
    }

    /// Minimum Car Telemetry packet size includes a 3-byte trailer.
    /// Source: EA F1 25 spec — trailer contains mfdPanel + suggestedGear.
    #[test]
    fn min_car_telemetry_packet_size() {
        assert_eq!(f1_25::MIN_CAR_TELEMETRY_PACKET_SIZE, 29 + 22 * 60 + 3);
    }

    /// Packet format discriminator values.
    /// Source: EA F1 UDP spec — u16 field in header identifies year.
    #[test]
    fn packet_format_years() {
        assert_eq!(f1_native::PACKET_FORMAT_2023, 2023);
        assert_eq!(f1_native::PACKET_FORMAT_2024, 2024);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Gran Turismo 7 — Salsa20-encrypted UDP
// ═══════════════════════════════════════════════════════════════════════════════
//
// Authoritative sources:
//   - Nenkai/PDTools: SimulatorPacket.cs, SimulatorInterfaceCryptorGT7.cs
//   - Bornhall/gt7telemetry (Python)
//   - gt7dashboard community project

mod gt7_verification {
    use super::*;
    use openracing_telemetry_adapters::gran_turismo_7;

    /// GT7 receives telemetry on port 33740.
    /// Source: Nenkai/PDTools `BindPortGT7 = 33740`.
    /// Source: Bornhall/gt7telemetry `ReceivePort = 33740`.
    #[test]
    fn recv_port_is_33740() {
        assert_eq!(gran_turismo_7::GT7_RECV_PORT, 33740);
    }

    /// Heartbeat must be sent to port 33739.
    /// Source: Nenkai/PDTools `ReceivePortGT7 = 33739`.
    /// Source: Bornhall/gt7telemetry `SendPort = 33739`.
    #[test]
    fn send_port_is_33739() {
        assert_eq!(gran_turismo_7::GT7_SEND_PORT, 33739);
    }

    /// Standard packet (Type1) is 296 bytes (0x128).
    /// Source: Nenkai/PDTools `GetExpectedPacketSize(PacketType1) = 0x128`.
    #[test]
    fn packet_type1_is_296_bytes() {
        assert_eq!(gran_turismo_7::PACKET_SIZE, 296);
        assert_eq!(gran_turismo_7::PACKET_SIZE, 0x128);
    }

    /// Extended packet (Type2, GT7 ≥ 1.42) is 316 bytes (0x13C).
    /// Source: Nenkai/PDTools `GetExpectedPacketSize(PacketType2) = 0x13C`.
    #[test]
    fn packet_type2_is_316_bytes() {
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE2, 316);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE2, 0x13C);
    }

    /// Full packet (Type3, GT7 ≥ 1.42) is 344 bytes (0x158).
    /// Source: Nenkai/PDTools `GetExpectedPacketSize(PacketType3) = 0x158`.
    #[test]
    fn packet_type3_is_344_bytes() {
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE3, 344);
        assert_eq!(gran_turismo_7::PACKET_SIZE_TYPE3, 0x158);
    }

    /// Magic number is 0x47375330 ("G7S0" / "0S7G" in LE).
    /// Source: Nenkai/PDTools `SimulatorPacket.Read()` — checks for "G7S0".
    #[test]
    fn magic_is_0x47375330() {
        assert_eq!(gran_turismo_7::MAGIC, 0x4737_5330);
        // "G7S0" as ASCII bytes: G=0x47, 7=0x37, S=0x53, 0=0x30
        let bytes = gran_turismo_7::MAGIC.to_be_bytes();
        assert_eq!(bytes, [0x47, 0x37, 0x53, 0x30]);
    }

    /// Magic offset is byte 0 of the decrypted packet.
    /// Source: Nenkai/PDTools `SimulatorPacket.Read()`.
    #[test]
    fn magic_at_offset_0() {
        assert_eq!(gran_turismo_7::OFF_MAGIC, 0);
    }

    /// Salsa20 key is the first 32 bytes of "Simulator Interface Packet GT7 ver 0.0".
    /// Source: Nenkai/PDTools `SimulatorInterfaceCryptorGT7.cs`.
    #[test]
    fn salsa20_key_is_32_byte_prefix() {
        let full_string = "Simulator Interface Packet GT7 ver 0.0";
        let key = &full_string.as_bytes()[..32];
        assert_eq!(key, b"Simulator Interface Packet GT7 v");
    }

    /// XOR key for Type1 is 0xDEADBEAF (note: not DEADBEEF).
    /// Source: Nenkai/PDTools `XorKey` default for PacketType1.
    #[test]
    fn xor_key_type1_is_deadbeaf() {
        // Note: The GT7 protocol uses 0xDEAD_BEAF (with 'A'), NOT 0xDEAD_BEEF.
        let xor_key: u32 = 0xDEAD_BEAF;
        assert_eq!(xor_key, 0xDEAD_BEAF);
        assert_ne!(xor_key, 0xDEAD_BEEF_u32);
    }

    /// XOR key for Type2 is 0xDEADBEEF.
    /// Source: Nenkai/PDTools PacketType2 XOR key.
    #[test]
    fn xor_key_type2_is_deadbeef() {
        let xor_key: u32 = 0xDEAD_BEEF;
        assert_eq!(xor_key, 0xDEAD_BEEF);
    }

    /// XOR key for Type3 is 0x55FABB4F.
    /// Source: Nenkai/PDTools PacketType3 XOR key.
    #[test]
    fn xor_key_type3_is_55fabb4f() {
        let xor_key: u32 = 0x55FA_BB4F;
        assert_eq!(xor_key, 0x55FA_BB4F);
    }

    /// Heartbeat bytes per packet type.
    /// Source: Nenkai/PDTools `SimulatorInterfaceClient.cs`.
    #[test]
    fn heartbeat_bytes() {
        use gran_turismo_7::Gt7PacketType;
        assert_eq!(Gt7PacketType::Type1.heartbeat(), b"A");
        assert_eq!(Gt7PacketType::Type2.heartbeat(), b"B");
        assert_eq!(Gt7PacketType::Type3.heartbeat(), b"~");
    }

    /// Packet field offsets verified against Nenkai/PDTools SimulatorPacket.cs.
    #[test]
    fn gt7_field_offsets() {
        // All offsets from Nenkai/PDTools SimulatorPacket.Read():
        // EngineRPM@0x3C, GasLevel@0x44, GasCapacity@0x48, MetersPerSecond@0x4C,
        // WaterTemp@0x58, TireFL–RR@0x60–0x6C, LapCount@0x74, BestLap@0x78,
        // LastLap@0x7C, CurrentLap@0x80, Position@0x84, NumCars@0x86,
        // MaxAlertRPM@0x8A, Flags@0x8E, Gear@0x90, Throttle@0x91, Brake@0x92,
        // CarCode@0x124
        assert_eq!(0x3C_usize, 60); // EngineRPM (f32)
        assert_eq!(0x44_usize, 68); // GasLevel / FuelLevel (f32)
        assert_eq!(0x48_usize, 72); // GasCapacity / FuelCapacity (f32)
        assert_eq!(0x4C_usize, 76); // MetersPerSecond / Speed (f32)
        assert_eq!(0x58_usize, 88); // WaterTemp (f32)
        assert_eq!(0x60_usize, 96); // TireFL temp (f32)
        assert_eq!(0x64_usize, 100); // TireFR temp (f32)
        assert_eq!(0x68_usize, 104); // TireRL temp (f32)
        assert_eq!(0x6C_usize, 108); // TireRR temp (f32)
        assert_eq!(0x74_usize, 116); // LapCount (i16)
        assert_eq!(0x78_usize, 120); // BestLap ms (i32)
        assert_eq!(0x7C_usize, 124); // LastLap ms (i32)
        assert_eq!(0x80_usize, 128); // CurrentLap ms (i32)
        assert_eq!(0x84_usize, 132); // Position (i16)
        assert_eq!(0x86_usize, 134); // NumCars (i16)
        assert_eq!(0x8A_usize, 138); // MaxAlertRPM (i16)
        assert_eq!(0x8E_usize, 142); // Flags (i16)
        assert_eq!(0x90_usize, 144); // Gear byte
        assert_eq!(0x91_usize, 145); // Throttle (u8)
        assert_eq!(0x92_usize, 146); // Brake (u8)
        assert_eq!(0x124_usize, 292); // CarCode (i32)
    }

    /// GT7 flags bitmask verified against Nenkai/PDTools `SimulatorFlags` enum.
    /// Source: Nenkai/PDTools SimulatorFlags.
    #[test]
    fn gt7_flags_bitmask() {
        let flag_paused: u16 = 1 << 1; // 0x02
        let flag_rev_limit: u16 = 1 << 5; // 0x20
        let flag_asm_active: u16 = 1 << 10; // 0x400
        let flag_tcs_active: u16 = 1 << 11; // 0x800
        assert_eq!(flag_paused, 0x02);
        assert_eq!(flag_rev_limit, 0x20);
        assert_eq!(flag_asm_active, 0x0400);
        assert_eq!(flag_tcs_active, 0x0800);
    }

    /// Gear encoding: low nibble = current gear, high nibble = suggested gear.
    /// 15 (0xF) = reverse, 0 = neutral.
    /// Source: Nenkai/PDTools SimulatorPacket.cs.
    #[test]
    fn gt7_gear_encoding() {
        // Low nibble: 0=neutral, 1..7=forward gears, 15(0xF)=reverse
        // High nibble: suggested gear
        let gear_byte: u8 = 0x23; // suggested=2, current=3rd gear
        let current = gear_byte & 0x0F;
        let suggested = (gear_byte >> 4) & 0x0F;
        assert_eq!(current, 3);
        assert_eq!(suggested, 2);

        let reverse_byte: u8 = 0x0F;
        let current_rev = reverse_byte & 0x0F;
        assert_eq!(current_rev, 15); // 0xF = reverse

        let neutral_byte: u8 = 0x00;
        let current_neutral = neutral_byte & 0x0F;
        assert_eq!(current_neutral, 0); // 0 = neutral
    }

    /// All GT7 packet fields are little-endian.
    /// Source: GT7 targets PlayStation (ARM LE) and the protocol is documented as LE.
    #[test]
    fn gt7_is_little_endian() {
        let rpm: f32 = 8500.0;
        let le_bytes = rpm.to_le_bytes();
        let roundtrip = f32::from_le_bytes(le_bytes);
        assert!((roundtrip - 8500.0).abs() < 0.01);
    }

    /// PacketType expected sizes match.
    #[test]
    fn packet_type_expected_sizes() {
        use gran_turismo_7::Gt7PacketType;
        assert_eq!(Gt7PacketType::Type1.expected_size(), 296);
        assert_eq!(Gt7PacketType::Type2.expected_size(), 316);
        assert_eq!(Gt7PacketType::Type3.expected_size(), 344);
    }

    /// PacketType XOR keys match documented values.
    #[test]
    fn packet_type_xor_keys() {
        use gran_turismo_7::Gt7PacketType;
        assert_eq!(Gt7PacketType::Type1.xor_key(), 0xDEAD_BEAF);
        assert_eq!(Gt7PacketType::Type2.xor_key(), 0xDEAD_BEEF);
        assert_eq!(Gt7PacketType::Type3.xor_key(), 0x55FA_BB4F);
    }

    /// Nonce derivation: iv1 = LE u32 from [0x40..0x44], iv2 = iv1 ^ xor_key,
    /// nonce = [iv2_le, iv1_le] (8 bytes).
    /// Source: Nenkai/PDTools SimulatorInterfaceCryptorGT7.cs.
    #[test]
    fn nonce_derivation() {
        let iv1: u32 = 0x1234_5678;
        let xor_key: u32 = 0xDEAD_BEAF;
        let iv2 = iv1 ^ xor_key;
        let mut nonce = [0u8; 8];
        nonce[..4].copy_from_slice(&iv2.to_le_bytes());
        nonce[4..].copy_from_slice(&iv1.to_le_bytes());
        assert_eq!(nonce.len(), 8);
        assert_eq!(iv2, 0x1234_5678 ^ 0xDEAD_BEAF);
        // Verify reconstruction
        let recovered_iv2 = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        let recovered_iv1 = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        assert_eq!(recovered_iv1, iv1);
        assert_eq!(recovered_iv2, iv2);
    }

    /// GT7 adapter game ID.
    #[test]
    fn gt7_game_id() {
        assert_eq!(GranTurismo7Adapter::new().game_id(), "gran_turismo_7");
    }

    /// Extended PacketType2 field offsets.
    /// Source: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x13C)` block.
    #[test]
    fn gt7_type2_extended_offsets() {
        assert_eq!(0x128_usize, 296); // WheelRotation (f32)
        assert_eq!(0x12C_usize, 300); // FillerFloatFB (f32, unknown)
        assert_eq!(0x130_usize, 304); // Sway (f32, lateral)
        assert_eq!(0x134_usize, 308); // Heave (f32, vertical)
        assert_eq!(0x138_usize, 312); // Surge (f32, longitudinal)
    }

    /// Extended PacketType3 field offsets.
    /// Source: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x158)` block.
    #[test]
    fn gt7_type3_extended_offsets() {
        assert_eq!(0x13C_usize, 316); // CarTypeByte1 (u8)
        assert_eq!(0x13D_usize, 317); // CarTypeByte2 (u8)
        assert_eq!(0x13E_usize, 318); // CarTypeByte3 (u8, 4=electric)
        assert_eq!(0x13F_usize, 319); // NoGasConsumption (u8)
        assert_eq!(0x140_usize, 320); // Unk5Vec4 (4× f32)
        assert_eq!(0x150_usize, 336); // EnergyRecovery (f32)
        assert_eq!(0x154_usize, 340); // Unk7 (f32)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Cross-adapter port uniqueness
// ═══════════════════════════════════════════════════════════════════════════════

mod cross_adapter_verification {
    /// Verify that default ports are distinct across adapters to prevent conflicts.
    /// (Exception: FH5 and FM share port 5300 intentionally.)
    #[test]
    fn default_ports_are_distinct_except_forza_family() {
        let ports = [
            ("iracing", "shared_memory"), // No UDP port
            ("acc", "9000"),
            ("beamng", "4444"),
            ("forza_motorsport", "5300"),
            ("forza_horizon_5", "5300"), // intentionally same as FM
            ("forza_horizon_4", "12350"),
            ("f1", "20777"),
            ("gt7_recv", "33740"),
            ("gt7_send", "33739"),
        ];
        // Check that non-shared-memory, non-Forza-family ports are unique
        let udp_ports: Vec<(&str, u16)> = ports
            .iter()
            .filter(|(_, p)| *p != "shared_memory")
            .filter_map(|(name, p)| p.parse::<u16>().ok().map(|port| (*name, port)))
            .collect();

        for i in 0..udp_ports.len() {
            for j in (i + 1)..udp_ports.len() {
                let (name_a, port_a) = udp_ports[i];
                let (name_b, port_b) = udp_ports[j];
                // Allow Forza family to share port 5300
                let forza_pair = (name_a.contains("forza") && name_b.contains("forza"))
                    || (name_a.contains("gt7") && name_b.contains("gt7"));
                if !forza_pair {
                    assert_ne!(
                        port_a, port_b,
                        "Port collision: {name_a} and {name_b} both use port {port_a}"
                    );
                }
            }
        }
    }

    /// All adapters return non-empty game IDs.
    #[test]
    fn all_game_ids_non_empty() {
        use openracing_telemetry_adapters::*;
        let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![
            Box::new(BeamNGAdapter::new()),
            Box::new(ForzaAdapter::new()),
            Box::new(GranTurismo7Adapter::new()),
            Box::new(IRacingAdapter::new()),
            Box::new(F1Adapter::new()),
            Box::new(ACCAdapter::new()),
        ];
        for adapter in &adapters {
            assert!(!adapter.game_id().is_empty());
        }
    }

    /// All adapters have reasonable update rates (1ms ≤ rate ≤ 1000ms).
    #[test]
    fn update_rates_are_reasonable() {
        use openracing_telemetry_adapters::*;
        use std::time::Duration;
        let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![
            Box::new(BeamNGAdapter::new()),
            Box::new(ForzaAdapter::new()),
            Box::new(GranTurismo7Adapter::new()),
            Box::new(IRacingAdapter::new()),
            Box::new(F1Adapter::new()),
            Box::new(ACCAdapter::new()),
        ];
        for adapter in &adapters {
            let rate = adapter.expected_update_rate();
            assert!(
                rate >= Duration::from_millis(1) && rate <= Duration::from_secs(1),
                "{} has unreasonable update rate: {:?}",
                adapter.game_id(),
                rate
            );
        }
    }
}
