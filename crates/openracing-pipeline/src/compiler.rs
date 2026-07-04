//! Pipeline compiler for converting FilterConfig to executable pipeline
//!
//! This module provides the pipeline compiler that transforms filter configurations
//! into RT-safe executable pipelines.

use crate::hash::{calculate_config_hash, calculate_config_hash_with_curve};
use crate::types::{
    CompilationTask, CompiledPipeline, FilterNodeFn, Pipeline, PipelineError, SharedTaskQueue,
};
use crate::validation::PipelineValidator;
use openracing_curves::CurveType;
use openracing_filters::{
    BumpstopState, CurveState, DamperState, Frame, FrictionState, HandsOffState, InertiaState,
    NotchState, ReconstructionState, SlewRateState,
};
use racing_wheel_schemas::entities::FilterConfig;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tracing::debug;

unsafe fn state_mut<'a, T>(state: *mut u8) -> &'a mut T {
    debug_assert!(!state.is_null(), "pipeline node state pointer is null");
    debug_assert!(
        (state as usize).is_multiple_of(std::mem::align_of::<T>()),
        "pipeline node state pointer is not aligned for its state type"
    );
    // SAFETY: Callers pair each wrapper with `Pipeline::add_state_node::<T>`,
    // which allocates, aligns, and initializes exactly one `T` for the node.
    unsafe { &mut *state.cast::<T>() }
}

unsafe fn state_ref<'a, T>(state: *mut u8) -> &'a T {
    debug_assert!(!state.is_null(), "pipeline node state pointer is null");
    debug_assert!(
        (state as usize).is_multiple_of(std::mem::align_of::<T>()),
        "pipeline node state pointer is not aligned for its state type"
    );
    // SAFETY: Callers pair each wrapper with `Pipeline::add_state_node::<T>`,
    // which allocates, aligns, and initializes exactly one `T` for the node.
    unsafe { &*state.cast::<T>() }
}

fn reconstruction_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_reconstruction_filter_static` registers this wrapper with a
    // `ReconstructionState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::reconstruction_filter(frame, state_mut::<ReconstructionState>(state));
    }
}

fn friction_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_friction_filter_static` registers this wrapper with a
    // `FrictionState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::friction_filter(frame, state_ref::<FrictionState>(state));
    }
}

fn damper_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_damper_filter_static` registers this wrapper with a
    // `DamperState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::damper_filter(frame, state_ref::<DamperState>(state));
    }
}

fn inertia_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_inertia_filter_static` registers this wrapper with an
    // `InertiaState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::inertia_filter(frame, state_mut::<InertiaState>(state));
    }
}

fn notch_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_notch_filters_static` registers this wrapper with a
    // `NotchState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::notch_filter(frame, state_mut::<NotchState>(state));
    }
}

fn slew_rate_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_slew_rate_filter_static` registers this wrapper with a
    // `SlewRateState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::slew_rate_filter(frame, state_mut::<SlewRateState>(state));
    }
}

fn curve_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_curve_filter_static` registers this wrapper with a
    // `CurveState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::curve_filter(frame, state_ref::<CurveState>(state));
    }
}

fn torque_cap_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_torque_cap_filter_static` registers this wrapper with an
    // `f32` torque cap allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::torque_cap_filter(frame, *state_ref::<f32>(state));
    }
}

fn bumpstop_filter_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_bumpstop_filter_static` registers this wrapper with a
    // `BumpstopState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::bumpstop_filter(frame, state_mut::<BumpstopState>(state));
    }
}

fn hands_off_detector_wrapper(frame: &mut Frame, state: *mut u8) {
    // SAFETY: `add_hands_off_detector_static` registers this wrapper with a
    // `HandsOffState` allocated by `Pipeline::add_state_node`.
    unsafe {
        openracing_filters::hands_off_detector(frame, state_mut::<HandsOffState>(state));
    }
}

/// Pipeline compiler for converting FilterConfig to executable pipeline
///
/// The compiler transforms filter configurations into RT-safe pipelines that can
/// be executed at 1kHz with zero allocations.
///
/// # Example
///
/// ```ignore
/// use openracing_pipeline::PipelineCompiler;
/// use racing_wheel_schemas::entities::FilterConfig;
///
/// #[tokio::main]
/// async fn main() {
///     let compiler = PipelineCompiler::new();
///     let config = FilterConfig::default();
///
///     let result = compiler.compile_pipeline(config).await;
///     assert!(result.is_ok());
/// }
/// ```
#[derive(Debug)]
pub struct PipelineCompiler {
    /// Pending compilation tasks
    pending_compilations: SharedTaskQueue,
    /// Validator for configurations
    validator: PipelineValidator,
}

impl PipelineCompiler {
    /// Create a new pipeline compiler
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending_compilations: Arc::new(Mutex::new(Vec::new())),
            validator: PipelineValidator::new(),
        }
    }

    /// Compile a FilterConfig into an executable pipeline (off-thread)
    ///
    /// This is the main compilation method that transforms a filter configuration
    /// into an RT-safe executable pipeline.
    ///
    /// # Arguments
    ///
    /// * `config` - The filter configuration to compile
    ///
    /// # Returns
    ///
    /// * `Ok(CompiledPipeline)` - The compiled pipeline ready for RT execution
    /// * `Err(PipelineError)` - Compilation failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let compiler = PipelineCompiler::new();
    /// let config = FilterConfig::default();
    /// let compiled = compiler.compile_pipeline(config).await?;
    /// ```
    pub async fn compile_pipeline(
        &self,
        config: FilterConfig,
    ) -> Result<CompiledPipeline, PipelineError> {
        debug!("Compiling pipeline from FilterConfig");

        self.validator.validate_config(&config)?;

        let config_hash = calculate_config_hash(&config);

        let mut pipeline = Pipeline::with_hash(config_hash);

        add_reconstruction_filter_static(&mut pipeline, config.reconstruction)?;
        add_friction_filter_static(&mut pipeline, config.friction)?;
        add_damper_filter_static(&mut pipeline, config.damper)?;
        add_inertia_filter_static(&mut pipeline, config.inertia)?;
        add_notch_filters_static(&mut pipeline, &config.notch_filters)?;
        add_slew_rate_filter_static(&mut pipeline, config.slew_rate)?;
        add_curve_filter_static(&mut pipeline, &config.curve_points)?;
        add_torque_cap_filter_static(&mut pipeline, config.torque_cap.value())?;
        add_bumpstop_filter_static(&mut pipeline, &config.bumpstop)?;
        add_hands_off_detector_static(&mut pipeline, &config.hands_off)?;

        debug!(
            "Pipeline compiled successfully with {} nodes, hash: {:x}",
            pipeline.node_count(),
            config_hash
        );

        Ok(CompiledPipeline {
            pipeline,
            config_hash,
        })
    }

    /// Compile a FilterConfig with a response curve (off-thread)
    ///
    /// Extends `compile_pipeline` by adding support for response curves.
    /// The response curve is pre-computed as a LUT at compile time for RT-safe evaluation.
    ///
    /// # Arguments
    ///
    /// * `config` - The filter configuration to compile
    /// * `response_curve` - Optional response curve type
    ///
    /// # Returns
    ///
    /// * `Ok(CompiledPipeline)` - The compiled pipeline with response curve
    /// * `Err(PipelineError)` - Compilation failed
    pub async fn compile_pipeline_with_response_curve(
        &self,
        config: FilterConfig,
        response_curve: Option<&CurveType>,
    ) -> Result<CompiledPipeline, PipelineError> {
        debug!("Compiling pipeline from FilterConfig with response curve");

        self.validator.validate_config(&config)?;

        if let Some(curve) = response_curve {
            self.validator.validate_response_curve(curve)?;
        }

        let config_hash = calculate_config_hash_with_curve(&config, response_curve);

        let mut pipeline = Pipeline::with_hash(config_hash);

        add_reconstruction_filter_static(&mut pipeline, config.reconstruction)?;
        add_friction_filter_static(&mut pipeline, config.friction)?;
        add_damper_filter_static(&mut pipeline, config.damper)?;
        add_inertia_filter_static(&mut pipeline, config.inertia)?;
        add_notch_filters_static(&mut pipeline, &config.notch_filters)?;
        add_slew_rate_filter_static(&mut pipeline, config.slew_rate)?;
        add_curve_filter_static(&mut pipeline, &config.curve_points)?;
        add_torque_cap_filter_static(&mut pipeline, config.torque_cap.value())?;
        add_bumpstop_filter_static(&mut pipeline, &config.bumpstop)?;
        add_hands_off_detector_static(&mut pipeline, &config.hands_off)?;

        if let Some(curve) = response_curve {
            pipeline.set_response_curve(curve.to_lut());
            debug!("Response curve set on pipeline");
        }

        debug!(
            "Pipeline compiled successfully with {} nodes, response_curve={}, hash: {:x}",
            pipeline.node_count(),
            response_curve.is_some(),
            config_hash
        );

        Ok(CompiledPipeline {
            pipeline,
            config_hash,
        })
    }

    /// Compile pipeline asynchronously and return immediately
    ///
    /// Returns a oneshot receiver that will receive the compilation result.
    ///
    /// # Arguments
    ///
    /// * `config` - The filter configuration to compile
    ///
    /// # Returns
    ///
    /// * `Ok(oneshot::Receiver)` - Channel to receive the compilation result
    /// * `Err(PipelineError)` - Failed to queue compilation
    pub async fn compile_pipeline_async(
        &self,
        config: FilterConfig,
    ) -> Result<oneshot::Receiver<Result<CompiledPipeline, PipelineError>>, PipelineError> {
        let (tx, rx) = oneshot::channel();

        let task = CompilationTask {
            config,
            response_tx: tx,
        };

        {
            let mut pending = self.pending_compilations.lock().await;
            pending.push(task);
        }

        let pending_compilations = Arc::clone(&self.pending_compilations);
        let validator = self.validator.clone();

        tokio::spawn(async move {
            let tasks = {
                let mut pending = pending_compilations.lock().await;
                std::mem::take(&mut *pending)
            };

            for task in tasks {
                let result = async {
                    validator.validate_config(&task.config)?;
                    let config_hash = calculate_config_hash(&task.config);
                    let mut pipeline = Pipeline::with_hash(config_hash);

                    add_reconstruction_filter_static(&mut pipeline, task.config.reconstruction)?;
                    add_friction_filter_static(&mut pipeline, task.config.friction)?;
                    add_damper_filter_static(&mut pipeline, task.config.damper)?;
                    add_inertia_filter_static(&mut pipeline, task.config.inertia)?;
                    add_notch_filters_static(&mut pipeline, &task.config.notch_filters)?;
                    add_slew_rate_filter_static(&mut pipeline, task.config.slew_rate)?;
                    add_curve_filter_static(&mut pipeline, &task.config.curve_points)?;
                    add_torque_cap_filter_static(&mut pipeline, task.config.torque_cap.value())?;
                    add_bumpstop_filter_static(&mut pipeline, &task.config.bumpstop)?;
                    add_hands_off_detector_static(&mut pipeline, &task.config.hands_off)?;

                    Ok(CompiledPipeline {
                        pipeline,
                        config_hash,
                    })
                }
                .await;

                let _ = task.response_tx.send(result);
            }
        });

        Ok(rx)
    }
}

fn add_reconstruction_filter_static(
    pipeline: &mut Pipeline,
    level: u8,
) -> Result<(), PipelineError> {
    if level == 0 {
        return Ok(());
    }

    let state = ReconstructionState::new(level);
    pipeline.add_state_node(reconstruction_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_friction_filter_static(
    pipeline: &mut Pipeline,
    friction: racing_wheel_schemas::prelude::Gain,
) -> Result<(), PipelineError> {
    if friction.value() == 0.0 {
        return Ok(());
    }

    let state = FrictionState::new(friction.value(), true);
    pipeline.add_state_node(friction_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_damper_filter_static(
    pipeline: &mut Pipeline,
    damper: racing_wheel_schemas::prelude::Gain,
) -> Result<(), PipelineError> {
    if damper.value() == 0.0 {
        return Ok(());
    }

    let state = DamperState::new(damper.value(), true);
    pipeline.add_state_node(damper_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_inertia_filter_static(
    pipeline: &mut Pipeline,
    inertia: racing_wheel_schemas::prelude::Gain,
) -> Result<(), PipelineError> {
    if inertia.value() == 0.0 {
        return Ok(());
    }

    let state = InertiaState::new(inertia.value());
    pipeline.add_state_node(inertia_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_notch_filters_static(
    pipeline: &mut Pipeline,
    filters: &[racing_wheel_schemas::entities::NotchFilter],
) -> Result<(), PipelineError> {
    for filter in filters {
        let state = NotchState::new(
            filter.frequency.value(),
            filter.q_factor,
            filter.gain_db,
            1000.0,
        );
        pipeline.add_state_node(notch_filter_wrapper as FilterNodeFn, state);
    }
    Ok(())
}

fn add_slew_rate_filter_static(
    pipeline: &mut Pipeline,
    slew_rate: racing_wheel_schemas::prelude::Gain,
) -> Result<(), PipelineError> {
    if slew_rate.value() >= 1.0 {
        return Ok(());
    }

    let state = SlewRateState::new(slew_rate.value());
    pipeline.add_state_node(slew_rate_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_curve_filter_static(
    pipeline: &mut Pipeline,
    curve_points: &[racing_wheel_schemas::prelude::CurvePoint],
) -> Result<(), PipelineError> {
    if curve_points.len() == 2
        && curve_points[0].input == 0.0
        && curve_points[0].output == 0.0
        && curve_points[1].input == 1.0
        && curve_points[1].output == 1.0
    {
        return Ok(());
    }

    let curve_tuples: Vec<(f32, f32)> = curve_points.iter().map(|p| (p.input, p.output)).collect();
    let state = CurveState::new(&curve_tuples);
    pipeline.add_state_node(curve_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_torque_cap_filter_static(
    pipeline: &mut Pipeline,
    torque_cap: f32,
) -> Result<(), PipelineError> {
    if torque_cap >= 1.0 {
        return Ok(());
    }

    pipeline.add_state_node(torque_cap_filter_wrapper as FilterNodeFn, torque_cap);
    Ok(())
}

fn add_bumpstop_filter_static(
    pipeline: &mut Pipeline,
    bumpstop_config: &racing_wheel_schemas::entities::BumpstopConfig,
) -> Result<(), PipelineError> {
    if !bumpstop_config.enabled {
        return Ok(());
    }

    let state = BumpstopState::new(
        bumpstop_config.enabled,
        bumpstop_config.start_angle,
        bumpstop_config.max_angle,
        bumpstop_config.stiffness,
        bumpstop_config.damping,
    );
    pipeline.add_state_node(bumpstop_filter_wrapper as FilterNodeFn, state);
    Ok(())
}

fn add_hands_off_detector_static(
    pipeline: &mut Pipeline,
    config: &racing_wheel_schemas::entities::HandsOffConfig,
) -> Result<(), PipelineError> {
    if !config.enabled {
        return Ok(());
    }

    let state = HandsOffState::new(config.enabled, config.threshold, config.timeout_seconds);
    pipeline.add_state_node(hands_off_detector_wrapper as FilterNodeFn, state);
    Ok(())
}

impl Clone for PipelineCompiler {
    fn clone(&self) -> Self {
        Self {
            pending_compilations: Arc::clone(&self.pending_compilations),
            validator: self.validator.clone(),
        }
    }
}

impl Default for PipelineCompiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must() failed: {:?}", e),
        }
    }

    fn create_test_config() -> FilterConfig {
        FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_pipeline_compilation_basic() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();

        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert!(compiled.pipeline.node_count() > 0);
        assert!(compiled.config_hash != 0);
    }

    #[tokio::test]
    async fn test_pipeline_compilation_deterministic() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();

        let result1 = compiler.compile_pipeline(config.clone()).await.unwrap();
        let result2 = compiler.compile_pipeline(config).await.unwrap();

        assert_eq!(result1.config_hash, result2.config_hash);
        assert_eq!(result1.pipeline.node_count(), result2.pipeline.node_count());
    }

    #[tokio::test]
    async fn test_pipeline_compilation_different_configs() {
        let compiler = PipelineCompiler::new();
        let config1 = create_test_config();
        let config2 = FilterConfig::default();

        let result1 = compiler.compile_pipeline(config1).await.unwrap();
        let result2 = compiler.compile_pipeline(config2).await.unwrap();

        assert_ne!(result1.config_hash, result2.config_hash);
    }

    #[tokio::test]
    async fn test_pipeline_compilation_with_response_curve() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();
        let curve = CurveType::exponential(2.0).unwrap();

        let result = compiler
            .compile_pipeline_with_response_curve(config, Some(&curve))
            .await;

        assert!(result.is_ok());
        let compiled = result.unwrap();
        assert!(compiled.pipeline.response_curve().is_some());
    }

    #[tokio::test]
    async fn test_pipeline_compilation_empty_config() {
        let compiler = PipelineCompiler::new();
        let mut config = FilterConfig::default();
        // Disable bumpstop and hands-off to get a truly empty pipeline
        config.bumpstop.enabled = false;
        config.hands_off.enabled = false;

        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert!(compiled.pipeline.is_empty());
    }
}
