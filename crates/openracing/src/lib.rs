//! Facade SDK entrypoint for OpenRacing public crates.
//!
//! This crate is intentionally thin. It gives downstream users one stable
//! `openracing` package to depend on while implementation crates migrate toward
//! the public package surface defined in `docs/architecture/crate-surface.md`.
//!
//! Feature flags expose public crate families without moving their code:
//!
//! - `calibration` -> [`calibration`]
//! - `curves` -> [`curves`]
//! - `ffb` -> [`ffb`]
//! - `profile` -> [`profile`]
//! - `plugin-abi` -> [`plugin_abi`]
//! - `engine` -> [`engine`]
//! - `sdk` -> calibration, curves, ffb, profile, and plugin ABI
//! - `runtime` -> `sdk` plus engine

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

/// The OpenRacing package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "calibration")]
pub use openracing_calibration as calibration;

#[cfg(feature = "curves")]
pub use openracing_curves as curves;

#[cfg(feature = "engine")]
pub use openracing_engine as engine;

#[cfg(feature = "ffb")]
pub use openracing_ffb as ffb;

#[cfg(feature = "plugin-abi")]
pub use openracing_plugin_abi as plugin_abi;

#[cfg(feature = "profile")]
pub use openracing_profile as profile;

/// Common SDK imports for applications that enable the matching feature flags.
pub mod prelude {
    #[cfg(feature = "calibration")]
    pub use openracing_calibration::{AxisCalibration, DeviceCalibration};

    #[cfg(feature = "curves")]
    pub use openracing_curves::{BezierCurve, CurveError, CurveLut, CurveType};

    #[cfg(feature = "ffb")]
    pub use openracing_ffb::{ConstantEffect, DamperEffect, FfbDirection, FfbGain, SpringEffect};

    #[cfg(feature = "plugin-abi")]
    pub use openracing_plugin_abi::{
        PLUG_ABI_VERSION, PluginCapabilities, PluginHeader, TelemetryFrame, WASM_ABI_VERSION,
    };

    #[cfg(feature = "profile")]
    pub use openracing_profile::{CURRENT_SCHEMA_VERSION, WheelProfile};
}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn facade_exposes_workspace_version() {
        assert!(!VERSION.is_empty());
    }
}
