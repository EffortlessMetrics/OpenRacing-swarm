//! Normalized telemetry input loading and validation.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

pub(super) const DEFAULT_RECORD_FRAME_PERIOD_NS: u64 = 16_666_667;

pub(super) fn read_normalized_telemetry_records(path: &str) -> Result<Vec<Value>> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("failed to read '{}'", path))?;
    let trimmed = contents.trim_start();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && let Ok(value) = serde_json::from_str::<Value>(&contents)
    {
        return normalized_records_from_value(&value);
    }

    let mut records = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSONL record {} in '{}'", line_index + 1, path))?;
        records.push(value);
    }
    Ok(records)
}

fn normalized_records_from_value(value: &Value) -> Result<Vec<Value>> {
    if let Some(array) = value.as_array() {
        return Ok(array.clone());
    }
    for key in ["frames", "records", "snapshots"] {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            return Ok(array.clone());
        }
    }
    Ok(vec![value.clone()])
}

pub(super) fn normalized_telemetry_payload(record: &Value) -> Option<&Value> {
    for key in [
        "data",
        "normalized",
        "normalized_snapshot",
        "snapshot",
        "telemetry",
    ] {
        if let Some(value) = record.get(key).filter(|value| value.is_object()) {
            return Some(value);
        }
    }
    record.is_object().then_some(record)
}

pub(super) fn normalized_f64(record: &Value, key: &str) -> Option<f64> {
    record.get(key).and_then(Value::as_f64)
}

fn normalized_i64(record: &Value, key: &str) -> Option<i64> {
    record.get(key).and_then(Value::as_i64)
}

pub(super) fn normalized_telemetry_payload_is_valid(record: &Value) -> bool {
    normalized_f64(record, "speed_ms")
        .map(|value| value.is_finite() && (0.0..=200.0).contains(&value))
        .unwrap_or(false)
        && normalized_f64(record, "steering_angle")
            .map(|value| value.is_finite() && (-40.0..=40.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "throttle")
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "brake")
            .map(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "rpm")
            .map(|value| value.is_finite() && (0.0..=30_000.0).contains(&value))
            .unwrap_or(false)
        && normalized_i64(record, "gear")
            .map(|value| (-1..=15).contains(&value))
            .unwrap_or(false)
        && normalized_f64(record, "ffb_scalar")
            .map(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
            .unwrap_or(false)
}

pub(super) fn validated_normalized_snapshots(input_path: &str) -> Result<Vec<Value>> {
    let records = read_normalized_telemetry_records(input_path)?;
    if records.is_empty() {
        return Err(anyhow!(
            "normalized telemetry input '{}' did not contain any snapshots",
            input_path
        ));
    }

    let mut snapshots = Vec::with_capacity(records.len());
    let mut previous_timestamp_ns = None;
    for (sequence, record) in records.iter().enumerate() {
        let mut snapshot = normalized_telemetry_payload(record)
            .ok_or_else(|| anyhow!("record {sequence} is not a JSON object"))?
            .clone();
        if !normalized_telemetry_payload_is_valid(&snapshot) {
            return Err(anyhow!(
                "record {sequence} is missing valid normalized telemetry fields"
            ));
        }

        let Some(object) = snapshot.as_object_mut() else {
            return Err(anyhow!("record {sequence} is not a JSON object"));
        };
        let expected_sequence =
            u64::try_from(sequence).context("too many normalized telemetry records")?;
        match object.get("sequence") {
            Some(value) if value.as_u64() == Some(expected_sequence) => {}
            Some(_) => {
                return Err(anyhow!(
                    "record {sequence} has non-contiguous sequence metadata"
                ));
            }
            None => {
                object.insert("sequence".to_string(), serde_json::json!(expected_sequence));
            }
        }

        let default_timestamp_ns = expected_sequence.saturating_mul(DEFAULT_RECORD_FRAME_PERIOD_NS);
        let timestamp_ns = match object.get("timestamp_ns") {
            Some(value) => value
                .as_u64()
                .ok_or_else(|| anyhow!("record {sequence} has invalid timestamp_ns"))?,
            None => {
                object.insert(
                    "timestamp_ns".to_string(),
                    serde_json::json!(default_timestamp_ns),
                );
                default_timestamp_ns
            }
        };
        if previous_timestamp_ns
            .map(|previous| timestamp_ns <= previous)
            .unwrap_or(false)
        {
            return Err(anyhow!(
                "record {sequence} has stale or non-monotonic timestamp_ns"
            ));
        }
        previous_timestamp_ns = Some(timestamp_ns);
        snapshots.push(snapshot);
    }

    Ok(snapshots)
}
