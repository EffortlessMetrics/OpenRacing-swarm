//! Accepted-value contracts for finite `CurveLut` tables.

use openracing_curves::{CurveError, CurveLut};
use serde::Deserialize;
use serde::de::IntoDeserializer;
use serde::de::value::{Error as ValueError, SeqDeserializer};
use std::error::Error;
use std::io;

type TestResult = Result<(), Box<dyn Error>>;

fn expect_invalid_index<T: std::fmt::Debug>(
    result: Result<T, CurveError>,
    index: usize,
) -> TestResult {
    match result {
        Err(CurveError::InvalidConfiguration(message)) => {
            assert!(message.contains(&format!("entry {index}")));
            assert!(message.contains("finite"));
            Ok(())
        }
        Err(other) => Err(io::Error::other(format!(
            "expected InvalidConfiguration for entry {index}, got {other:?}"
        ))
        .into()),
        Ok(value) => Err(io::Error::other(format!(
            "non-finite entry {index} was accepted: {value:?}"
        ))
        .into()),
    }
}

fn deserialize_table(values: [f32; CurveLut::SIZE]) -> Result<CurveLut, ValueError> {
    let iter = values.into_iter().map(IntoDeserializer::into_deserializer);
    CurveLut::deserialize(SeqDeserializer::new(iter))
}

#[test]
fn strict_raw_table_rejects_non_finite_entries_at_first_interior_and_final_indices() -> TestResult {
    for (index, value) in [
        (0usize, f32::NAN),
        (127usize, f32::INFINITY),
        (CurveLut::SIZE - 1, f32::NEG_INFINITY),
    ] {
        let mut table = [0.5_f32; CurveLut::SIZE];
        table[index] = value;
        expect_invalid_index(CurveLut::try_from_table(table), index)?;
    }
    Ok(())
}

#[test]
fn deserialization_rejects_non_finite_entries_without_dumping_table_contents() -> TestResult {
    for (index, value) in [
        (0usize, f32::NAN),
        (127usize, f32::INFINITY),
        (CurveLut::SIZE - 1, f32::NEG_INFINITY),
    ] {
        let mut table = [0.25_f32; CurveLut::SIZE];
        table[index] = value;
        match deserialize_table(table) {
            Err(error) => {
                let message = error.to_string();
                assert!(message.contains(&format!("entry {index}")));
                assert!(message.contains("finite"));
                assert!(!message.contains("0.25, 0.25"));
            }
            Ok(_) => {
                return Err(io::Error::other(format!(
                    "deserializer accepted non-finite entry {index}"
                ))
                .into());
            }
        }
    }
    Ok(())
}

#[test]
fn finite_boundary_table_is_accepted_and_lookup_stays_finite() -> TestResult {
    let mut table = [0.5_f32; CurveLut::SIZE];
    table[0] = 0.0;
    table[CurveLut::SIZE - 1] = 1.0;

    let lut = CurveLut::try_from_table(table)?;
    for input in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        assert!(lut.lookup(input).is_finite());
    }
    Ok(())
}

#[test]
fn strict_generator_reports_first_non_finite_output() -> TestResult {
    let target_index = 127usize;
    let target_input = target_index as f32 / (CurveLut::SIZE - 1) as f32;
    let result = CurveLut::try_from_fn(|input| {
        if input.to_bits() == target_input.to_bits() {
            f32::NAN
        } else {
            input
        }
    });

    expect_invalid_index(result, target_index)
}

#[test]
fn legacy_infallible_generator_sanitizes_non_finite_outputs_deterministically() {
    let lut = CurveLut::from_fn(|input| {
        if input > 0.4 && input < 0.6 {
            f32::INFINITY
        } else {
            input
        }
    });

    assert!(lut.table().iter().all(|value| value.is_finite()));
    let interior = lut.lookup(0.5);
    assert!(interior.is_finite());
    assert_eq!(interior, 0.0);

    let repeat = CurveLut::from_fn(|input| {
        if input > 0.4 && input < 0.6 {
            f32::INFINITY
        } else {
            input
        }
    });
    assert_eq!(lut, repeat);
}
