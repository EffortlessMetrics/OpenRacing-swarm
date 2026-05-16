//! Small serde_json accessors used by Moza receipt validators.
//!
//! Keeping these helpers separate makes receipt validation call sites read as
//! domain logic instead of repeated JSON type plumbing.

use serde_json::Value;

pub(super) fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

pub(super) fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(super) fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

pub(super) fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(super) fn json_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}
