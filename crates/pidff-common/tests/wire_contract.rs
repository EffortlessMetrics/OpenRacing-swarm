//! Focused byte-level contracts for PIDFF reports and Block Load parsing.

use openracing_pidff_common::{
    BlockLoadReport, BlockLoadStatus, DURATION_INFINITE, EffectType, encode_device_gain,
    encode_set_effect, parse_block_load, report_ids,
};

#[test]
fn set_effect_keeps_reserved_fields_zero_and_no_trigger_marker() {
    let report = encode_set_effect(
        u8::MAX,
        EffectType::Sine,
        DURATION_INFINITE,
        u8::MAX,
        u16::MAX,
    );

    assert_eq!(report[0], report_ids::SET_EFFECT);
    assert_eq!(&report[5..9], &[0, 0, 0, 0]);
    assert_eq!(report[10], 0xff);
    assert_eq!(report[13], 0);
}

#[test]
fn device_gain_keeps_reserved_byte_zero_and_clamps_at_boundary() {
    for gain in [0, 1, 9_999, 10_000, 10_001, u16::MAX] {
        let report = encode_device_gain(gain);
        assert_eq!(report[0], report_ids::DEVICE_GAIN);
        assert_eq!(report[1], 0);
        assert_eq!(
            u16::from_le_bytes([report[2], report[3]]),
            gain.min(10_000)
        );
    }
}

#[test]
fn block_load_accepts_trailing_bytes_but_uses_only_the_declared_prefix() {
    let expected = Some(BlockLoadReport {
        block_index: 9,
        status: BlockLoadStatus::Error,
        ram_pool_available: 0xabcd,
    });

    assert_eq!(parse_block_load(&[0x12, 9, 3, 0xcd, 0xab]), expected);
    assert_eq!(
        parse_block_load(&[0x12, 9, 3, 0xcd, 0xab, 0xff, 0x00, 0x7e]),
        expected
    );
}

#[test]
fn block_load_rejects_every_undefined_status_byte() {
    for status in u8::MIN..=u8::MAX {
        if (1..=3).contains(&status) {
            continue;
        }
        assert!(
            parse_block_load(&[report_ids::BLOCK_LOAD, 0, status, 0, 0]).is_none(),
            "undefined Block Load status {status} was accepted"
        );
    }
}

#[test]
fn block_load_rejects_every_other_report_id() {
    for report_id in u8::MIN..=u8::MAX {
        if report_id == report_ids::BLOCK_LOAD {
            continue;
        }
        assert!(
            parse_block_load(&[report_id, 1, 1, 0, 0]).is_none(),
            "report id {report_id:#04x} was accepted as Block Load"
        );
    }
}
