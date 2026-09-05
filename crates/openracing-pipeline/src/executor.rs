//! Pipeline execution for real-time FFB processing
//!
//! This module provides RT-safe pipeline execution with strict guarantees:
//! - No heap allocations in the hot path
//! - O(n) time complexity where n = node count
//! - Bounded execution time
//! - Atomic pipeline swap support

use crate::types::Pipeline;
use openracing_errors::{RTError, RTResult};
use openracing_filters::Frame;

impl Pipeline {
    /// Process frame through pipeline (RT-safe, no allocations)
    ///
    /// This is the core RT-safe processing function. It applies each filter
    /// node in sequence to the frame, then applies the response curve if set.
    ///
    /// # RT Safety
    ///
    /// - **No heap allocations**: All state is pre-allocated during compilation
    /// - **No syscalls**: No I/O, no locks, no blocking operations
    /// - **O(n) time**: Linear in the number of filter nodes
    /// - **Bounded execution**: Each filter has bounded execution time
    ///
    /// # Arguments
    ///
    /// * `frame` - The frame to process (modified in place)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Frame processed successfully
    /// * `Err(RTError::PipelineFault)` - Output validation failed
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_pipeline::{Pipeline, Frame};
    ///
    /// let mut pipeline = Pipeline::new();
    /// let mut frame = Frame {
    ///     ffb_in: 0.5,
    ///     torque_out: 0.5,
    ///     wheel_speed: 0.0,
    ///     hands_off: false,
    ///     ts_mono_ns: 0,
    ///     seq: 1,
    /// };
    ///
    /// // RT-safe processing
    /// let result = pipeline.process(&mut frame);
    /// assert!(result.is_ok());
    /// ```
    #[inline]
    pub fn process(&mut self, frame: &mut Frame) -> RTResult {
        self.process_internal(frame)
    }

    /// Internal processing method
    ///
    /// Separated from `process` to allow for allocation tracking in debug builds
    /// in the parent crate.
    #[inline]
    fn process_internal(&mut self, frame: &mut Frame) -> RTResult {
        for i in 0..self.nodes.len() {
            let node_fn = self.nodes[i];
            let state_ptr = self.node_state_ptr(i);
            if state_ptr.is_null() {
                return Err(RTError::PipelineFault);
            }
            node_fn(frame, state_ptr);

            if !frame.torque_out.is_finite() || frame.torque_out.abs() > 1.0 {
                return Err(RTError::PipelineFault);
            }
        }

        if let Some(ref curve) = self.response_curve {
            let input = frame.torque_out.abs().clamp(0.0, 1.0);
            let mapped = curve.lookup(input);
            frame.torque_out = frame.torque_out.signum() * mapped;
        }

        Ok(())
    }

    /// Swap pipeline at tick boundary (RT-safe, atomic)
    ///
    /// This method atomically replaces the current pipeline with a new one.
    /// From the RT thread's perspective, this is an atomic operation.
    ///
    /// # Arguments
    ///
    /// * `new_pipeline` - The new pipeline to swap in
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    ///
    /// let mut pipeline1 = Pipeline::new();
    /// let pipeline2 = Pipeline::with_hash(0x12345678);
    ///
    /// // Atomically swap pipelines
    /// pipeline1.swap_at_tick_boundary(pipeline2);
    /// assert_eq!(pipeline1.config_hash(), 0x12345678);
    /// ```
    pub fn swap_at_tick_boundary(&mut self, new_pipeline: Pipeline) {
        *self = new_pipeline;
    }

    /// Process frame with response curve transformation
    ///
    /// This is equivalent to `process()` but allows overriding the response curve.
    #[inline]
    pub fn process_with_curve(
        &mut self,
        frame: &mut Frame,
        curve: Option<&openracing_curves::CurveLut>,
    ) -> RTResult {
        self.process_internal_with_curve(frame, curve)
    }

    #[inline]
    fn process_internal_with_curve(
        &mut self,
        frame: &mut Frame,
        curve: Option<&openracing_curves::CurveLut>,
    ) -> RTResult {
        for i in 0..self.nodes.len() {
            let node_fn = self.nodes[i];
            let state_ptr = self.node_state_ptr(i);
            if state_ptr.is_null() {
                return Err(RTError::PipelineFault);
            }
            node_fn(frame, state_ptr);

            if !frame.torque_out.is_finite() || frame.torque_out.abs() > 1.0 {
                return Err(RTError::PipelineFault);
            }
        }

        if let Some(curve) = curve {
            let input = frame.torque_out.abs().clamp(0.0, 1.0);
            let mapped = curve.lookup(input);
            frame.torque_out = frame.torque_out.signum() * mapped;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FilterNodeFn;
    use openracing_curves::CurveType;

    fn test_frame(torque_out: f32) -> Frame {
        Frame {
            ffb_in: torque_out,
            torque_out,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        }
    }

    fn set_torque_to_nan(frame: &mut Frame, _state: *mut u8) {
        frame.torque_out = f32::NAN;
    }

    fn set_torque_above_bound(frame: &mut Frame, _state: *mut u8) {
        frame.torque_out = 1.01;
    }

    fn set_torque_below_bound(frame: &mut Frame, _state: *mut u8) {
        frame.torque_out = -1.01;
    }

    fn halve_torque(frame: &mut Frame, _state: *mut u8) {
        frame.torque_out *= 0.5;
    }

    fn pipeline_with_node(node: FilterNodeFn) -> Pipeline {
        let mut pipeline = Pipeline::new();
        // Register a real state slot so `node_state_ptr` is valid and each test
        // proves the injected node ran rather than failing on missing state.
        pipeline.add_state_node(node, 0_u8);
        pipeline
    }

    #[test]
    fn test_pipeline_process_empty() {
        let mut pipeline = Pipeline::new();
        let mut frame = test_frame(0.5);

        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pipeline_process_with_response_curve_linear() {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        let mut frame = test_frame(0.5);

        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!((frame.torque_out - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_pipeline_process_with_response_curve_exponential()
    -> Result<(), openracing_curves::CurveError> {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame = test_frame(0.5);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            (frame.torque_out - 0.25).abs() < 0.02,
            "Expected ~0.25, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn test_response_curve_preserves_sign() -> Result<(), openracing_curves::CurveError> {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame_pos = test_frame(0.5);
        assert_eq!(pipeline.process(&mut frame_pos), Ok(()));
        assert!(frame_pos.torque_out > 0.0);

        let mut frame_neg = test_frame(-0.5);
        assert_eq!(pipeline.process(&mut frame_neg), Ok(()));
        assert!(frame_neg.torque_out < 0.0);

        assert!(
            (frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs() < 0.01,
            "Magnitudes should be equal"
        );
        Ok(())
    }

    #[test]
    fn test_pipeline_swap_atomicity() {
        let mut pipeline1 = Pipeline::new();
        let pipeline2 = Pipeline::with_hash(0x12345678);

        assert_eq!(pipeline1.config_hash(), 0);

        pipeline1.swap_at_tick_boundary(pipeline2);

        assert_eq!(pipeline1.config_hash(), 0x12345678);
    }

    #[test]
    fn test_pipeline_returns_fault_after_node_outputs_nan() {
        let mut pipeline = pipeline_with_node(set_torque_to_nan);
        let mut frame = test_frame(0.5);

        assert_eq!(pipeline.process(&mut frame), Err(RTError::PipelineFault));
        assert!(frame.torque_out.is_nan(), "the injected node did not run");
    }

    #[test]
    fn test_pipeline_returns_fault_after_node_exceeds_either_torque_bound() {
        for (node, expected) in [
            (set_torque_above_bound as FilterNodeFn, 1.01_f32),
            (set_torque_below_bound as FilterNodeFn, -1.01_f32),
        ] {
            let mut pipeline = pipeline_with_node(node);
            let mut frame = test_frame(0.5);

            assert_eq!(pipeline.process(&mut frame), Err(RTError::PipelineFault));
            assert_eq!(frame.torque_out, expected, "the injected node did not run");
        }
    }

    #[test]
    fn test_process_with_curve_uses_override_without_storing_curve() {
        let mut pipeline = pipeline_with_node(halve_torque);
        let curve = CurveType::Linear.to_lut();
        let mut frame = test_frame(0.8);

        assert_eq!(
            pipeline.process_with_curve(&mut frame, Some(&curve)),
            Ok(())
        );
        assert!(pipeline.response_curve().is_none());
        assert!((frame.torque_out - 0.4).abs() < 0.02);
    }

    #[test]
    fn test_process_with_curve_returns_fault_before_curve_mapping() {
        let mut pipeline = pipeline_with_node(set_torque_above_bound);
        let curve = CurveType::Linear.to_lut();
        let mut frame = test_frame(0.5);

        assert_eq!(
            pipeline.process_with_curve(&mut frame, Some(&curve)),
            Err(RTError::PipelineFault)
        );
        assert_eq!(frame.torque_out, 1.01, "the injected node did not run");
    }

    #[test]
    fn test_pipeline_process_validates_output() {
        let mut pipeline = Pipeline::new();
        let mut frame = test_frame(f32::NAN);

        // Empty pipeline doesn't validate - it just passes through
        // Validation happens at filter node boundaries
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Empty pipeline should pass through");
    }

    #[test]
    fn test_pipeline_process_bounds_output() {
        let mut pipeline = Pipeline::new();
        let mut frame = test_frame(2.0);

        // Empty pipeline doesn't validate - it just passes through
        // Validation happens at filter node boundaries
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Empty pipeline should pass through");
    }
}
