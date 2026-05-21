use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io;

use racing_wheel_hid_moza_protocol::serial::vendor_authority::{
    CODEC_STATUS, FORBIDDEN_VENDOR_CLASSES, MozaRiskClass, MozaSerialCodecStatus,
    REQUIRED_VENDOR_COMMANDS, command_by_group_command,
};
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn registry_fixture() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../fixtures/moza/r5/vendor-command-registry.json"
    ))
}

fn registry_schema() -> Result<Value, serde_json::Error> {
    serde_json::from_str(include_str!(
        "../../../schemas/moza-vendor-command-registry.schema.json"
    ))
}

fn array_field<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>, io::Error> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data(format!("missing array field `{field}`")))
}

fn str_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, io::Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_data(format!("missing string field `{field}`")))
}

fn bool_field(value: &Value, field: &str) -> Result<bool, io::Error> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_data(format!("missing bool field `{field}`")))
}

fn u8_field(value: &Value, field: &str) -> Result<u8, io::Error> {
    let number = value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_data(format!("missing integer field `{field}`")))?;
    u8::try_from(number).map_err(|_| invalid_data(format!("field `{field}` is outside u8 range")))
}

fn commands_by_id(registry: &Value) -> Result<BTreeMap<&str, &Value>, io::Error> {
    let mut commands = BTreeMap::new();
    for command in array_field(registry, "commands")? {
        let id = str_field(command, "id")?;
        if commands.insert(id, command).is_some() {
            return Err(invalid_data(format!("duplicate command id `{id}`")));
        }
    }
    Ok(commands)
}

fn required_fields(value: &Value) -> Result<BTreeSet<&str>, io::Error> {
    let mut fields = BTreeSet::new();
    for field in array_field(value, "required")? {
        let field_name = field
            .as_str()
            .ok_or_else(|| invalid_data("schema required field entry must be a string"))?;
        fields.insert(field_name);
    }
    Ok(fields)
}

#[test]
fn registry_fixture_is_complete_and_non_claiming() -> TestResult {
    let registry = registry_fixture()?;

    assert_eq!(
        str_field(&registry, "registry_completeness")?,
        "complete",
        "PR2 registry must be complete before codec or probe work builds on it"
    );
    assert_eq!(
        str_field(&registry, "claim_scope")?,
        "protocol_research_only"
    );
    assert!(!bool_field(&registry, "native_control_evidence")?);
    assert!(!bool_field(&registry, "hardware_output_authorized")?);
    assert!(!bool_field(&registry, "native_visible_ready")?);
    assert!(array_field(&registry, "missing_required_families")?.is_empty());
    assert_eq!(str_field(&registry, "codec_status")?, "semantic_only");
    assert!(!CODEC_STATUS.allows_hardware_writes());

    let commands = commands_by_id(&registry)?;
    assert_eq!(commands.len(), REQUIRED_VENDOR_COMMANDS.len());

    for expected in REQUIRED_VENDOR_COMMANDS {
        let fixture_command = commands
            .get(expected.id)
            .ok_or_else(|| invalid_data(format!("missing command `{}`", expected.id)))?;
        assert_eq!(str_field(fixture_command, "family")?, expected.family);
        assert_eq!(u8_field(fixture_command, "group")?, expected.group);
        assert_eq!(u8_field(fixture_command, "device_id")?, expected.device_id);
        assert_eq!(u8_field(fixture_command, "command")?, expected.command);
        assert_eq!(str_field(fixture_command, "name")?, expected.name);
        assert_eq!(
            str_field(fixture_command, "risk_class")?,
            expected.risk_class.as_registry_str()
        );
        assert_eq!(
            bool_field(fixture_command, "allowed_for_read_only_status_probe")?,
            expected.read_only_status_probe_allowed
        );
        assert!(!bool_field(fixture_command, "hardware_output_authorized")?);
    }

    Ok(())
}

#[test]
fn schema_requires_non_claiming_registry_gates() -> TestResult {
    let schema = registry_schema()?;
    let required = required_fields(&schema)?;

    for field in [
        "claim_scope",
        "native_control_evidence",
        "hardware_output_authorized",
        "native_visible_ready",
        "codec_status",
        "forbidden",
    ] {
        assert!(
            required.contains(field),
            "registry schema must require `{field}`"
        );
    }

    Ok(())
}

#[test]
fn registry_fixture_covers_every_required_command_family() -> TestResult {
    let registry = registry_fixture()?;
    let mut family_ids = BTreeSet::new();

    for family in array_field(&registry, "families")? {
        let id = str_field(family, "id")?;
        assert!(bool_field(family, "complete")?, "family `{id}` is partial");
        family_ids.insert(id);
    }

    for required in [
        "authority_state",
        "gain_safety",
        "temperatures",
        "compatibility_mode",
    ] {
        assert!(
            family_ids.contains(required),
            "missing required command family `{required}`"
        );
    }

    for expected in REQUIRED_VENDOR_COMMANDS {
        let round_trip = command_by_group_command(expected.group, expected.command)
            .ok_or_else(|| invalid_data(format!("missing command lookup for `{}`", expected.id)))?;
        assert_eq!(round_trip.id, expected.id);
    }

    Ok(())
}

#[test]
fn risk_policy_keeps_write_like_candidates_authorization_bound() -> TestResult {
    let registry = registry_fixture()?;
    let commands = commands_by_id(&registry)?;

    for expected in REQUIRED_VENDOR_COMMANDS {
        let fixture_command = commands
            .get(expected.id)
            .ok_or_else(|| invalid_data(format!("missing command `{}`", expected.id)))?;

        if expected.risk_class.requires_exact_authorization() {
            assert!(!expected.risk_class.can_send_without_exact_authorization());
            assert!(
                !bool_field(fixture_command, "allowed_for_read_only_status_probe")?,
                "write-like command `{}` must not be read-only probe eligible",
                expected.id
            );
            assert!(
                !bool_field(fixture_command, "allowed_for_native_plan")?,
                "write-like command `{}` must remain plan-blocked until exact authorization exists",
                expected.id
            );
        }

        if expected.risk_class == MozaRiskClass::VendorStatus {
            assert!(expected.risk_class.can_send_without_exact_authorization());
            assert!(
                bool_field(fixture_command, "allowed_for_read_only_status_probe")?,
                "status command `{}` should be read-only probe eligible",
                expected.id
            );
        }
    }

    Ok(())
}

#[test]
fn forbidden_and_unknown_classes_are_not_sendable() -> TestResult {
    assert_eq!(CODEC_STATUS, MozaSerialCodecStatus::SemanticOnly);

    for risk_class in [
        MozaRiskClass::FirmwareOrDfuForbidden,
        MozaRiskClass::UnknownDoNotSend,
    ] {
        assert!(!risk_class.is_encodable());
        assert!(!risk_class.can_send_without_exact_authorization());
        assert!(!risk_class.requires_exact_authorization());
    }

    for (_class_id, risk_class) in FORBIDDEN_VENDOR_CLASSES {
        assert!(
            !risk_class.can_send_without_exact_authorization(),
            "forbidden class must never be sendable without review"
        );
    }

    let registry = registry_fixture()?;
    let mut fixture_forbidden = BTreeMap::new();
    for forbidden in array_field(&registry, "forbidden")? {
        let id = str_field(forbidden, "id")?;
        assert!(!bool_field(forbidden, "sendable_without_review")?);
        fixture_forbidden.insert(id, forbidden);
    }

    for (class_id, risk_class) in FORBIDDEN_VENDOR_CLASSES {
        let fixture_forbidden = fixture_forbidden
            .get(class_id)
            .ok_or_else(|| invalid_data(format!("missing forbidden class `{class_id}`")))?;
        assert_eq!(
            str_field(fixture_forbidden, "risk_class")?,
            risk_class.as_registry_str()
        );
    }

    Ok(())
}
