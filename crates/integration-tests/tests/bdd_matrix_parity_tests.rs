//! BDD matrix parity tests for telemetry governance.
//!
//! Enforces precise set alignments between the authoritative `telemetry-support` matrix,
//! the implemented `telemetry-adapters`, and the `telemetry-config-writers` via BDD metrics.
//!
//! Uses `MATRIX_COMPLETE` coverage constraints, ensuring that every game present in the
//! matrix is implemented in both the adapters and config writers. Experimental
//! adapters and writers not yet in the official matrix are tolerated.

use tracing_test::traced_test;

use openracing_telemetry_adapters::adapter_factories;
use openracing_telemetry_config::config_writer_factories;
use racing_wheel_telemetry_integration::{
    CoveragePolicy, compare_runtime_registries_with_policies,
};
use racing_wheel_telemetry_support::matrix_game_id_set;

#[test]
#[traced_test]
fn execute_matrix_parity_bdd_scenario() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Given the canonical matrix of supported games.
    let matrix_ids = matrix_game_id_set()?;

    // 2. And the adapter registry containing implemented runtime targets.
    let adapters = adapter_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    // 3. And the config writer registry allocating target config behaviors.
    let writers = config_writer_factories()
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();

    // 4. When executing a runtime coverage comparison applying MATRIX_COMPLETE requirements.
    let report = compare_runtime_registries_with_policies(
        matrix_ids,
        adapters,
        writers,
        CoveragePolicy::MATRIX_COMPLETE,
        CoveragePolicy::MATRIX_COMPLETE,
    );

    // Get the deterministic BDD metrics directly from the integration policies.
    let bdd_metrics = report.bdd_metrics();

    // 5. Then matrix parity requirements should be satisfied without regressions.
    if !bdd_metrics.parity_ok {
        // Build an exhaustive report on why parity failed so developers can resolve constraints.
        let mut violations = String::new();

        if !bdd_metrics.adapter.parity_ok {
            violations.push_str(&format!(
                "Adapter Policy Failed\n  Missing Adapters: {:?}\n  Extra Adapters: {:?}\n",
                bdd_metrics.adapter.missing_game_ids, bdd_metrics.adapter.extra_game_ids
            ));
        }

        if !bdd_metrics.writer.parity_ok {
            violations.push_str(&format!(
                "Config Writer Policy Failed\n  Missing Writers: {:?}\n  Extra Writers: {:?}\n",
                bdd_metrics.writer.missing_game_ids, bdd_metrics.writer.extra_game_ids
            ));
        }

        panic!(
            "BDD Matrix Parity Check Failed!\n\n\
             Workspace Telemetry Parity violated MATRIX_COMPLETE constraint.\n\
             This means you likely added a game to the yaml matrix but didn't build \
             an adapter/writer, breaking coverage guarantees.\n\n\
             {}",
            violations
        );
    }

    Ok(())
}
